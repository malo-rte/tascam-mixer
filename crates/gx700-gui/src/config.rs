//! Persisted GUI state and an on-disk cache of the patch bank, so a relaunch shows
//! the list instantly instead of re-reading 100 patches (~1 minute) every time.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Default interface zoom factor (egui zoom), used when no config exists.
pub(crate) const DEFAULT_ZOOM: f32 = 1.5;

/// GUI state saved between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GuiConfig {
    /// Interface zoom factor, restored on startup.
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    /// Saved window inner size in logical points (`[width, height]`), or `None` for
    /// the default size.
    #[serde(default)]
    pub window: Option<[f32; 2]>,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            zoom: DEFAULT_ZOOM,
            window: None,
        }
    }
}

fn default_zoom() -> f32 {
    DEFAULT_ZOOM
}

/// One cached patch-list row (mirrors `PatchHeader`, made serializable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedRow {
    pub slot: u16,
    pub name: String,
    pub output_level: u8,
    pub chain: Vec<u8>,
}

/// The suite's per-device settings directory: `<config>/rackctl/gx700`. `None` if
/// no home directory can be determined.
pub(crate) fn settings_dir() -> Option<PathBuf> {
    Some(
        ProjectDirs::from("", "malo-rte", "rackctl")?
            .config_dir()
            .join("gx700"),
    )
}

fn config_path() -> Option<PathBuf> {
    settings_dir().map(|dir| dir.join("gui-config.json"))
}

fn cache_path() -> Option<PathBuf> {
    settings_dir().map(|dir| dir.join("bank-cache.json"))
}

/// Load the saved GUI config, falling back to defaults on any error.
pub(crate) fn load() -> GuiConfig {
    let Some(path) = config_path() else {
        return GuiConfig::default();
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Best-effort save of the GUI config; failures are ignored (not critical).
pub(crate) fn save(config: &GuiConfig) {
    write_json(config_path(), config);
}

/// Load the cached patch bank (empty if absent or unreadable).
pub(crate) fn load_cache() -> Vec<CachedRow> {
    let Some(path) = cache_path() else {
        return Vec::new();
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Best-effort save of the patch-bank cache.
pub(crate) fn save_cache(rows: &[CachedRow]) {
    write_json(cache_path(), rows);
}

fn write_json<T: Serialize + ?Sized>(path: Option<PathBuf>, value: &T) {
    let Some(path) = path else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(value) {
        let _ = std::fs::write(&path, text);
    }
}

// ---- On-disk libraries: single patches, single blocks, and whole-bank scenes ----

/// Library directory for saved single patches (`<settings>/patches`).
pub(crate) fn patches_dir() -> Option<PathBuf> {
    settings_dir().map(|d| d.join("patches"))
}

/// Library directory for saved single effect blocks (`<settings>/blocks`).
pub(crate) fn blocks_dir() -> Option<PathBuf> {
    settings_dir().map(|d| d.join("blocks"))
}

/// Library directory for saved scenes — whole-bank snapshots (`<settings>/scenes`).
pub(crate) fn scenes_dir() -> Option<PathBuf> {
    settings_dir().map(|d| d.join("scenes"))
}

/// Turn a user-entered name into a safe `.json` file stem.
pub(crate) fn sanitize(name: &str) -> String {
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

/// Path of `name`.json inside `dir` (sanitised).
pub(crate) fn lib_path(dir: Option<PathBuf>, name: &str) -> Option<PathBuf> {
    dir.map(|d| d.join(format!("{}.json", sanitize(name))))
}

/// Sorted `.json` file stems in `dir` (empty if the directory is missing).
pub(crate) fn json_stems(dir: Option<PathBuf>) -> Vec<String> {
    let Some(dir) = dir else {
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

/// Save `value` as pretty JSON to `path`, creating parent dirs. `Err` on failure.
pub(crate) fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Read a file to a string, or `None` if it can't be read.
pub(crate) fn read_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// This device's stable model id, stamped into every saved library item so a file
/// can be matched to the device it belongs to (see `architecture.adoc`).
pub(crate) const DEVICE_ID: &str = "gx700";
/// Current on-disk library format version (bump when the envelope or payload shape
/// changes; older versions are migrated forward, newer ones refused).
pub(crate) const LIB_VERSION: u32 = 1;

/// Save `payload` to `path` wrapped in the library envelope (format version +
/// device id), so it is self-identifying and version-checked on load.
pub(crate) fn save_item<T: Serialize>(path: &Path, payload: &T) -> Result<(), String> {
    #[derive(Serialize)]
    struct Envelope<'a, T> {
        version: u32,
        device: &'a str,
        payload: &'a T,
    }
    save_json(
        path,
        &Envelope {
            version: LIB_VERSION,
            device: DEVICE_ID,
            payload,
        },
    )
}

/// Read a library item from envelope `text`. Returns `None` if it is not one of our
/// envelopes (the caller may then try a bare/legacy form); `Some(Err(reason))` if it
/// is an envelope but from another device or a newer format; `Some(Ok(payload))` for
/// a valid, compatible item.
pub(crate) fn load_item<T: DeserializeOwned>(text: &str) -> Option<Result<T, String>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let obj = value.as_object()?;
    if !(obj.contains_key("version") && obj.contains_key("device") && obj.contains_key("payload")) {
        return None; // not our envelope — let the caller try a bare/legacy parse
    }
    let device = obj
        .get("device")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if device != DEVICE_ID {
        return Some(Err(format!("saved from a different device ({device})")));
    }
    let version = obj
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(u32::MAX);
    if version > LIB_VERSION {
        return Some(Err(format!("saved by a newer version (v{version})")));
    }
    let payload = obj
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Some(serde_json::from_value(payload).map_err(|e| e.to_string()))
}

/// Delete a file. `Err` on failure.
pub(crate) fn delete_file(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Payload {
        a: u32,
        b: String,
    }

    fn payload() -> Payload {
        Payload {
            a: 7,
            b: "hi".to_owned(),
        }
    }

    #[test]
    fn save_item_writes_an_envelope_that_load_item_round_trips() {
        let path = std::env::temp_dir().join("rackctl-gx700-save-round-trip.json");
        save_item(&path, &payload()).expect("save");
        let text = read_text(&path).expect("read back");
        let got: Payload = load_item(&text)
            .expect("is an envelope")
            .expect("compatible");
        assert_eq!(got, payload());
        let _ = delete_file(&path);
    }

    #[test]
    fn load_item_accepts_the_current_device_and_version() {
        let text = format!(
            r#"{{"version":{LIB_VERSION},"device":"{DEVICE_ID}","payload":{{"a":9,"b":"ok"}}}}"#
        );
        let got: Payload = load_item(&text)
            .expect("is an envelope")
            .expect("compatible");
        assert_eq!(
            got,
            Payload {
                a: 9,
                b: "ok".to_owned()
            }
        );
    }

    #[test]
    fn load_item_rejects_a_different_device() {
        let text = r#"{"version":1,"device":"us16x08","payload":{"a":1,"b":"x"}}"#;
        let res: Option<Result<Payload, String>> = load_item(text);
        let err = res.expect("is an envelope").expect_err("wrong device");
        assert!(
            err.contains("us16x08"),
            "reason should name the device: {err}"
        );
    }

    #[test]
    fn load_item_rejects_a_newer_version() {
        let text = format!(
            r#"{{"version":{},"device":"{DEVICE_ID}","payload":{{"a":1,"b":"x"}}}}"#,
            LIB_VERSION + 1
        );
        let res: Option<Result<Payload, String>> = load_item(&text);
        let err = res.expect("is an envelope").expect_err("newer version");
        assert!(
            err.contains("newer"),
            "reason should mention the version: {err}"
        );
    }

    #[test]
    fn load_item_returns_none_for_a_bare_payload() {
        // A file that is not one of our envelopes; the caller falls back to a bare parse.
        let text = r#"{"a":1,"b":"x"}"#;
        let res: Option<Result<Payload, String>> = load_item(text);
        assert!(res.is_none());
    }
}
