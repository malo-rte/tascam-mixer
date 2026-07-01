//! Eleven Rack **library and patch-management** layer (scaffold).
//!
//! This is the device-specific half of the library stack: it sits on
//! [`rackctl_eleven`] (the device protocol + value model) and, once the patch work
//! lands, on `rackctl-core` (the device-neutral on-disk library), providing what
//! both a CLI and a GUI need but neither should own privately.
//!
//! It provides `.tfx` patch import; the named on-disk libraries (patches, device-faithful
//! **patch backups**, and whole-bank **scenes**, all via `rackctl-core`); and — with
//! the `alsa` feature — the device-touching [`manage`] operations (capture / restore
//! / bank backup / scene capture+restore / preset copy / named MIDI-CC) the CLI and
//! GUI share.
//!
//! NOTE: Eleven Rack, Digidesign and Avid are trademarks of Avid Technology, Inc.
//! This is an independent, unofficial project.
#![forbid(unsafe_code)]

pub mod format;
#[cfg(feature = "alsa")]
pub mod manage;

use std::path::{Path, PathBuf};

use rackctl_eleven::{Patch, PatchBackup};

pub use format::{Scene, parse_scene};
// Re-export the protocol crate so a frontend can depend on this one crate.
pub use rackctl_eleven as device;

/// A User patch slot as a stable library name: slot `0` -> `"U000"`.
#[must_use]
pub fn slot_label(slot: u8) -> String {
    format!("U{slot:03}")
}

/// This device's stable id, stamped into every saved library item (the
/// rackctl-core envelope) so a file is matched to the Eleven Rack on load.
pub const DEVICE_ID: &str = "eleven";

/// Current on-disk library format version. Bump when the envelope or a payload
/// shape changes; older versions load (with migration), newer ones are refused.
pub const LIB_VERSION: u32 = 1;

/// On-disk library subdirectory for saved patches.
const PATCHES: &str = "patches";

/// On-disk library subdirectory for device-faithful patch backups (the MIDI
/// snapshot form, distinct from interchange `.tfx` patches).
const BACKUPS: &str = "backups";

/// On-disk library subdirectory for whole-bank scenes.
const SCENES: &str = "scenes";

fn no_dir() -> String {
    "no config directory available".to_owned()
}

/// Parse an Eleven Rack `.tfx` patch file from disk into a typed [`Patch`].
///
/// # Errors
/// If the file cannot be read, or its contents are not a valid `.tfx` patch.
pub fn import_tfx(path: &Path) -> Result<Patch, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    rackctl_eleven::tfx::parse(&bytes).map_err(|e| e.to_string())
}

/// Save `patch` to the patch library as `name`, returning the file path.
///
/// # Errors
/// If no config directory is available, or the write fails.
pub fn save_patch(name: &str, patch: &Patch) -> Result<PathBuf, String> {
    let file = rackctl_core::item_path(DEVICE_ID, PATCHES, name).ok_or_else(no_dir)?;
    rackctl_core::save_item(&file, DEVICE_ID, LIB_VERSION, patch)?;
    Ok(file)
}

/// Load the named patch from the patch library.
///
/// # Errors
/// If the file is missing/unreadable, or matches no known format.
pub fn load_patch(name: &str) -> Result<Patch, String> {
    let file = rackctl_core::item_path(DEVICE_ID, PATCHES, name).ok_or_else(no_dir)?;
    let text = rackctl_core::read_text(&file)
        .ok_or_else(|| format!("could not read {}", file.display()))?;
    rackctl_core::decode_item::<Patch>(DEVICE_ID, LIB_VERSION, &text)
        .unwrap_or_else(|| Err("unrecognised patch file".to_owned()))
}

/// The names of saved patches, sorted.
#[must_use]
pub fn list_patches() -> Vec<String> {
    rackctl_core::list_stems(DEVICE_ID, PATCHES)
}

/// Save a device-faithful [`PatchBackup`] to the backup library as `name`.
///
/// # Errors
/// If no config directory is available, or the write fails.
pub fn save_backup(name: &str, patch: &PatchBackup) -> Result<PathBuf, String> {
    let file = rackctl_core::item_path(DEVICE_ID, BACKUPS, name).ok_or_else(no_dir)?;
    rackctl_core::save_item(&file, DEVICE_ID, LIB_VERSION, patch)?;
    Ok(file)
}

/// Load the named [`PatchBackup`] from the backup library.
///
/// # Errors
/// If the file is missing/unreadable, or matches no known format.
pub fn load_backup(name: &str) -> Result<PatchBackup, String> {
    let file = rackctl_core::item_path(DEVICE_ID, BACKUPS, name).ok_or_else(no_dir)?;
    let text = rackctl_core::read_text(&file)
        .ok_or_else(|| format!("could not read {}", file.display()))?;
    rackctl_core::decode_item::<PatchBackup>(DEVICE_ID, LIB_VERSION, &text)
        .unwrap_or_else(|| Err("unrecognised backup file".to_owned()))
}

/// The names of saved patch backups, sorted.
#[must_use]
pub fn list_backups() -> Vec<String> {
    rackctl_core::list_stems(DEVICE_ID, BACKUPS)
}

/// Save a whole-bank [`Scene`] to the scene library (its own `name`).
///
/// # Errors
/// If no config directory is available, or the write fails.
pub fn save_scene(scene: &Scene) -> Result<PathBuf, String> {
    let file = rackctl_core::item_path(DEVICE_ID, SCENES, &scene.name).ok_or_else(no_dir)?;
    rackctl_core::save_item(&file, DEVICE_ID, LIB_VERSION, scene)?;
    Ok(file)
}

/// Load the named [`Scene`] from the scene library.
///
/// # Errors
/// If the file is missing/unreadable, or matches no known format.
pub fn load_scene(name: &str) -> Result<Scene, String> {
    let file = rackctl_core::item_path(DEVICE_ID, SCENES, name).ok_or_else(no_dir)?;
    let text = rackctl_core::read_text(&file)
        .ok_or_else(|| format!("could not read {}", file.display()))?;
    parse_scene(&text)
}

/// The names of saved scenes, sorted.
#[must_use]
pub fn list_scenes() -> Vec<String> {
    rackctl_core::list_stems(DEVICE_ID, SCENES)
}
