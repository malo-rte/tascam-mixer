//! Command handlers, generic over the [`Transport`] so the same logic drives
//! the mock and the real ALSA rawmidi device.

use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rackctl_gx700::{Gx700, Kind, Param, RawPatch, Scene, Transport, param};

use crate::config;
use crate::value::{format_value, parse_value};

/// Pause between patch reads when listing a whole bank. Each read makes the
/// GX-700 stream a full patch (~14 messages); back-to-back, that sustained burst
/// can overrun the US-16x08's MIDI input until it stalls. A short gap between
/// reads keeps it from flooding.
const BANK_READ_PACE: Duration = Duration::from_millis(40);

/// Print the full parameter catalog. Backend-independent.
pub(crate) fn list() {
    println!("{:<22} {:<18} {:<24} PATCH OFFSET", "KEY", "BLOCK", "KIND");
    for &p in param::ALL {
        println!(
            "{:<22} {:<18} {:<24} {}",
            p.key(),
            p.block_label(),
            kind_str(p),
            hex4(p.patch_offset()),
        );
    }
}

/// Format a 4-byte address as space-separated hex.
fn hex4(addr: [u8; 4]) -> String {
    addr.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Print detailed metadata for one parameter. Backend-independent.
pub(crate) fn info(key: &str) -> Result<()> {
    let p = resolve(key)?;
    println!("{}  ({})", p.key(), p.block_label());
    println!("  patch offset: {}", hex4(p.patch_offset()));
    println!("  live address: {}", hex4(p.address()));
    match p.kind() {
        Kind::Bool => println!("  kind:  bool (on/off/true/false/1/0/yes/no)"),
        Kind::Int { min, max, default } => {
            println!("  kind:  int (raw device units)");
            println!("  range: {min}..={max} (default {default})");
        }
        Kind::Enum { values, default } => {
            println!("  kind:  enum (default {default})");
            let listed: Vec<String> = values
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{i}={v}"))
                .collect();
            println!("  values: {}", listed.join("  "));
        }
        _ => println!("  kind:  ?"),
    }
    Ok(())
}

/// Read and print one parameter's value.
pub(crate) fn get<T: Transport>(dev: &mut Gx700<T>, key: &str) -> Result<()> {
    let p = resolve(key)?;
    let value = dev.get(p)?;
    println!("{}", format_value(p, value));
    Ok(())
}

/// Parse and write one parameter's value. Silent on success.
pub(crate) fn set<T: Transport>(dev: &mut Gx700<T>, key: &str, raw_value: &str) -> Result<()> {
    let p = resolve(key)?;
    let value = parse_value(p, raw_value)?;
    dev.set(p, value)?;
    Ok(())
}

/// Print a patch in readable form: the current sound, or device slot `patch`.
pub(crate) fn dump_device<T: Transport>(dev: &mut Gx700<T>, patch: Option<u16>) -> Result<()> {
    let raw = read_from_device(dev, patch)?;
    print!("{}", raw.describe());
    Ok(())
}

/// Print a saved patch file in readable form. Backend-free.
pub(crate) fn dump_file(name: &str) -> Result<()> {
    print!("{}", read_saved(name)?.describe());
    Ok(())
}

/// Save a whole patch (the current sound, or device slot `patch`) to disk as a
/// lossless JSON file under the gx700 patches directory.
pub(crate) fn save<T: Transport>(dev: &mut Gx700<T>, name: &str, slot: Option<u16>) -> Result<()> {
    let raw = read_from_device(dev, slot)?;
    let path = write_patch_file(name, &raw)?;
    eprintln!("saved {:?} to {}", raw.name, path.display());
    Ok(())
}

/// Save every patch in a bank to disk: the 100 user patches, or (with `preset`)
/// the 100 preset patches. Each is written as `U001.json` / `P001.json` in the
/// patches directory — the same library `load`, `dump --file`, and
/// `patches --disk` read, so a backed-up patch can be inspected or restored by
/// name straight away. Reads are paced like [`patches`]; the port lock makes
/// this run the device's sole accessor from start to finish.
pub(crate) fn backup<T: Transport>(dev: &mut Gx700<T>, preset: bool) -> Result<()> {
    let (slots, tag) = if preset {
        (101u16..=200, 'P')
    } else {
        (1u16..=100, 'U')
    };
    let dir = config::patches_dir().context("could not determine the patches directory")?;
    let mut count = 0u32;
    for slot in slots {
        let raw = dev
            .read_patch(slot)
            .with_context(|| format!("reading patch {slot}"))?;
        let n = if preset { slot - 100 } else { slot };
        let name = format!("{tag}{n:03}");
        write_patch_file(&name, &raw)?;
        println!("{name}  {:<12}", raw.name);
        count += 1;
        sleep(BANK_READ_PACE); // ease off the US-16x08's MIDI input between reads
    }
    eprintln!("backed up {count} patches to {}", dir.display());
    Ok(())
}

/// Write `raw` to the patch library as `<name>.json`, creating the directory if
/// needed, and return the path written.
fn write_patch_file(name: &str, raw: &RawPatch) -> Result<PathBuf> {
    let path = config::patch_path(name).context("could not determine the patches directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(raw).context("serializing patch")?;
    fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Load a saved whole-patch file onto the device: the current sound, or (with
/// `to_patch`) a user patch memory slot (which it overwrites).
pub(crate) fn load<T: Transport>(
    dev: &mut Gx700<T>,
    name: &str,
    to_patch: Option<u16>,
) -> Result<()> {
    let raw = read_saved(name)?;
    let blocks = match to_patch {
        Some(slot) => dev.write_patch(slot, &raw)?,
        None => dev.write_current_patch(&raw)?,
    };
    let dest = to_patch.map_or_else(
        || "the current sound".to_owned(),
        |slot| format!("patch memory {slot}"),
    );
    eprintln!("loaded {:?} ({blocks} sub-blocks) into {dest}", raw.name);
    Ok(())
}

/// Read the current sound, or device patch memory slot `patch`, as a [`RawPatch`].
fn read_from_device<T: Transport>(dev: &mut Gx700<T>, patch: Option<u16>) -> Result<RawPatch> {
    match patch {
        Some(slot) => dev.read_patch(slot).map_err(Into::into),
        None => dev.read_current_patch().map_err(Into::into),
    }
}

/// Read and parse a saved patch file by name. Backend-free.
fn read_saved(name: &str) -> Result<RawPatch> {
    let path = config::patch_path(name).context("could not determine the patches directory")?;
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Save all 100 user patches to disk as a single named scene (a whole-device
/// snapshot). Reads are paced like [`patches`]; the port lock makes this the
/// device's sole accessor for the run.
pub(crate) fn scene_save<T: Transport>(dev: &mut Gx700<T>, name: &str) -> Result<()> {
    let mut scene = Scene::new(name.to_owned());
    for slot in 1u16..=100 {
        let raw = dev
            .read_patch(slot)
            .with_context(|| format!("reading patch {slot}"))?;
        println!("U{slot:03}  {:<12}", raw.name);
        scene.patches.insert(slot, raw);
        sleep(BANK_READ_PACE); // ease off the US-16x08's MIDI input between reads
    }
    let path = config::scene_path(name).context("could not determine the scenes directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&scene).context("serializing scene")?;
    fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    eprintln!(
        "saved scene {name:?} ({} patches) to {}",
        scene.patches.len(),
        path.display()
    );
    Ok(())
}

/// Restore a named scene to the device, overwriting the user patch bank. This is
/// destructive, so `confirm` must be set (the CLI's `--yes`).
pub(crate) fn scene_restore<T: Transport>(
    dev: &mut Gx700<T>,
    name: &str,
    confirm: bool,
) -> Result<()> {
    let path = config::scene_path(name).context("could not determine the scenes directory")?;
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let scene: Scene =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if !confirm {
        bail!(
            "restoring scene {name:?} overwrites {} user patches on the device; \
             re-run with --yes to confirm",
            scene.patches.len()
        );
    }
    let mut count = 0u32;
    for (&slot, raw) in &scene.patches {
        dev.write_patch(slot, raw)
            .with_context(|| format!("writing patch {slot}"))?;
        println!("U{slot:03}  {:<12}", raw.name);
        count += 1;
        sleep(BANK_READ_PACE); // pace writes too, for the same reason
    }
    eprintln!("restored scene {name:?} ({count} patches) to the device");
    Ok(())
}

/// List scenes saved on disk (file stems). Backend-free.
pub(crate) fn scenes_list() {
    let names = config::saved_scenes();
    if names.is_empty() {
        eprintln!("no saved scenes");
    }
    for name in names {
        println!("{name}");
    }
}

/// List patches saved on disk (file stems). Backend-free.
pub(crate) fn patches_disk() {
    let names = config::saved_patches();
    if names.is_empty() {
        eprintln!("no saved patches");
    }
    for name in names {
        println!("{name}");
    }
}

/// Select a patch memory by Program Change.
pub(crate) fn select<T: Transport>(dev: &mut Gx700<T>, n: u8) -> Result<()> {
    dev.select_patch(n)?;
    Ok(())
}

/// List patch-memory slots with their names and output level: the 100 user
/// patches, or (with `preset`) the 100 preset patches.
pub(crate) fn patches<T: Transport>(dev: &mut Gx700<T>, preset: bool) -> Result<()> {
    let (slots, tag) = if preset {
        (101u16..=200, 'P')
    } else {
        (1u16..=100, 'U')
    };
    for slot in slots {
        let header = dev
            .read_patch_header(slot)
            .with_context(|| format!("reading patch {slot}"))?;
        sleep(BANK_READ_PACE); // ease off the US-16x08's MIDI input between dumps
        let n = if preset { slot - 100 } else { slot };
        let level = rackctl_gx700::Param::from_key("output-level").map_or_else(
            || header.output_level.to_string(),
            |p| {
                rackctl_gx700::units::display(
                    p,
                    rackctl_gx700::Value::Int(i32::from(header.output_level)),
                )
            },
        );
        println!("{tag}{n:03}  {:<12}  out {level}", header.name);
    }
    Ok(())
}

fn resolve(key: &str) -> Result<Param> {
    Param::from_key(key).with_context(|| format!("unknown parameter {key:?} (try `list`)"))
}

fn kind_str(p: Param) -> String {
    match p.kind() {
        Kind::Bool => "bool".to_owned(),
        Kind::Int { min, max, .. } => format!("int {min}..={max}"),
        Kind::Enum { values, .. } => format!("enum[{}]", values.len()),
        _ => "?".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use rackctl_gx700::MockTransport;

    fn dev() -> Gx700<MockTransport> {
        Gx700::new(MockTransport::new())
    }

    #[test]
    fn set_then_get_round_trips() {
        let mut d = dev();
        set(&mut d, "preamp-volume", "77").unwrap();
        // get() prints; assert via the device directly.
        let p = Param::from_key("preamp-volume").unwrap();
        assert_eq!(format_value(p, d.get(p).unwrap()), "77");
    }

    #[test]
    fn unknown_param_errors() {
        assert!(resolve("nonsuch").is_err());
    }
}
