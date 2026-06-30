//! Device-neutral on-disk **library** for the Rackctl suite.
//!
//! Every Rackctl tool saves named items — scenes, patches, per-block presets — to
//! its own on-disk libraries. This crate owns the two device-neutral mechanics of
//! that, so the device crates and the frontends don't each reimplement (and drift
//! on) them:
//!
//! * **The file-format envelope** — every saved item is wrapped in a small,
//!   self-identifying envelope (`version` + `device` id + the typed `payload`), so a
//!   file can be matched to the device it belongs to and version-checked on load.
//!   See [`encode_item`] / [`decode_item`].
//! * **The path conventions** — libraries live under
//!   `<config>/rackctl/<device>/<kind>/<name>.json`, where `<kind>` is `scenes`,
//!   `patches`, `blocks/<block-type>`, … See [`library_dir`] / [`item_path`].
//!
//! The crate is intentionally device-agnostic: it knows nothing about MIDI, ALSA,
//! patches or blocks — only how to wrap/unwrap a serialisable payload and where the
//! files live. A device crate supplies its own `device` id, format version, and the
//! concrete payload types (and any legacy-format fallback the envelope can't cover).
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::Serialize;
use serde::de::DeserializeOwned;

// ---- File-format envelope ----

/// Wrap `payload` in the library envelope (format `version` + `device` id) and
/// render it as pretty JSON. `Err` only if the payload can't be serialised.
///
/// The result round-trips through [`decode_item`] with the same `device`.
pub fn encode_item<T: Serialize>(
    device: &str,
    version: u32,
    payload: &T,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct Envelope<'a, T> {
        version: u32,
        device: &'a str,
        payload: &'a T,
    }
    serde_json::to_string_pretty(&Envelope {
        version,
        device,
        payload,
    })
    .map_err(|e| e.to_string())
}

/// Extract the raw `payload` of envelope `text` as a JSON value, after the device
/// and version checks.
///
/// Returns `None` if `text` is **not one of our envelopes**; `Some(Err(reason))` if
/// it is an envelope but from another device or a newer-than-`max_version` format;
/// `Some(Ok(payload))` otherwise. Use this (rather than [`decode_item`]) when one
/// envelope may carry more than one possible payload *shape* and the caller wants to
/// try each in turn.
#[must_use]
pub fn decode_payload(
    device: &str,
    max_version: u32,
    text: &str,
) -> Option<Result<serde_json::Value, String>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let obj = value.as_object()?;
    if !(obj.contains_key("version") && obj.contains_key("device") && obj.contains_key("payload")) {
        return None; // not our envelope — let the caller try a bare/legacy parse
    }
    let dev = obj
        .get("device")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if dev != device {
        return Some(Err(format!("saved from a different device ({dev})")));
    }
    let version = obj
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(u32::MAX);
    if version > max_version {
        return Some(Err(format!("saved by a newer version (v{version})")));
    }
    Some(Ok(obj
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null)))
}

/// Read a library item from envelope `text`.
///
/// Returns `None` if `text` is **not one of our envelopes** (the caller may then try
/// a bare/legacy parse); `Some(Err(reason))` if it *is* an envelope but from another
/// device, a newer-than-`max_version` format, or a payload that doesn't match `T`;
/// `Some(Ok(payload))` for a valid, compatible item.
#[must_use]
pub fn decode_item<T: DeserializeOwned>(
    device: &str,
    max_version: u32,
    text: &str,
) -> Option<Result<T, String>> {
    match decode_payload(device, max_version, text)? {
        Ok(payload) => Some(serde_json::from_value(payload).map_err(|e| e.to_string())),
        Err(e) => Some(Err(e)),
    }
}

// ---- Paths ----

/// The suite's per-user config root (`<config>/rackctl`), or `None` if no home
/// directory can be determined.
#[must_use]
pub fn config_root() -> Option<PathBuf> {
    ProjectDirs::from("", "malo-rte", "rackctl").map(|d| d.config_dir().to_path_buf())
}

/// A device's config directory (`<config>/rackctl/<device>`).
#[must_use]
pub fn device_dir(device: &str) -> Option<PathBuf> {
    config_root().map(|d| d.join(device))
}

/// A device's library directory for items of `kind` (`<device>/<kind>`). `kind` is
/// a relative path such as `"patches"`, `"scenes"`, or `"blocks/distortion"`.
#[must_use]
pub fn library_dir(device: &str, kind: &str) -> Option<PathBuf> {
    device_dir(device).map(|d| d.join(kind))
}

/// The on-disk path of item `name` in a device's `kind` library
/// (`<device>/<kind>/<sanitised-name>.json`).
#[must_use]
pub fn item_path(device: &str, kind: &str, name: &str) -> Option<PathBuf> {
    library_dir(device, kind).map(|d| d.join(format!("{}.json", sanitize(name))))
}

/// Turn a user-entered name into a safe `.json` file stem: keep alphanumerics and
/// ` -_.`, replace anything else with `_`, and fall back to `untitled` if empty.
#[must_use]
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "untitled".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// The sorted `.json` file stems in a device's `kind` library (empty if the
/// directory is missing or unreadable).
#[must_use]
pub fn list_stems(device: &str, kind: &str) -> Vec<String> {
    let Some(dir) = library_dir(device, kind) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

// ---- File IO ----

/// Encode `payload` (see [`encode_item`]) and write it to `path`, creating parent
/// directories. `Err` on a serialise or write failure.
pub fn save_item<T: Serialize>(
    path: &Path,
    device: &str,
    version: u32,
    payload: &T,
) -> Result<(), String> {
    let text = encode_item(device, version, payload)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Read a file to a string, or `None` if it can't be read.
#[must_use]
pub fn read_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Delete a file. `Err` on failure.
pub fn delete_file(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        n: u32,
    }

    fn sample() -> Sample {
        Sample {
            name: "tone".to_owned(),
            n: 7,
        }
    }

    #[test]
    fn envelope_round_trips() {
        let text = encode_item("gx700", 1, &sample()).unwrap();
        // Self-identifying: the envelope fields are present in the JSON.
        assert!(text.contains("\"device\": \"gx700\""), "{text}");
        assert!(text.contains("\"version\": 1"), "{text}");
        let got: Sample = decode_item("gx700", 1, &text).unwrap().unwrap();
        assert_eq!(got, sample());
    }

    #[test]
    fn bare_json_is_not_an_envelope() {
        // A plain (un-enveloped) payload returns None so the caller can fall back.
        let bare = serde_json::to_string(&sample()).unwrap();
        assert!(decode_item::<Sample>("gx700", 1, &bare).is_none());
    }

    #[test]
    fn rejects_other_device_and_newer_version() {
        let text = encode_item("gx700", 1, &sample()).unwrap();
        // Wrong device.
        let err = decode_item::<Sample>("us16x08", 1, &text)
            .unwrap()
            .unwrap_err();
        assert!(err.contains("different device"), "{err}");
        // Newer format than we understand.
        let newer = encode_item("gx700", 9, &sample()).unwrap();
        let err = decode_item::<Sample>("gx700", 1, &newer)
            .unwrap()
            .unwrap_err();
        assert!(err.contains("newer version"), "{err}");
    }

    #[test]
    fn sanitize_keeps_safe_chars_and_defaults() {
        assert_eq!(sanitize("My Tone-1.json"), "My Tone-1.json");
        assert_eq!(sanitize("a/b:c"), "a_b_c");
        assert_eq!(sanitize("   "), "untitled");
    }

    #[test]
    fn item_path_layout() {
        // Only assert the relative shape when a home dir is available.
        if let Some(p) = item_path("gx700", "blocks/distortion", "Lead Boost") {
            assert!(
                p.ends_with("rackctl/gx700/blocks/distortion/Lead Boost.json"),
                "{p:?}"
            );
        }
    }
}
