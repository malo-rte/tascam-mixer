//! Persisted GUI state and an on-disk cache of the patch-bank names, so a relaunch
//! shows the list instantly instead of re-reading the whole directory every time.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// The on-disk library format + path conventions live in the shared crates; these
// helpers delegate so the GUI and CLI can't drift.
pub(crate) use rackctl_eleven_lib::DEVICE_ID;

/// Default interface zoom factor (egui zoom), used when no config exists.
pub(crate) const DEFAULT_ZOOM: f32 = 1.5;

/// GUI state saved between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GuiConfig {
    /// Interface zoom factor, restored on startup.
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    /// Saved window inner size in logical points, or `None` for the default size.
    #[serde(default)]
    pub window: Option<[f32; 2]>,
    /// Stable key of the last-active tab (see `Tab::as_key`), restored on startup.
    #[serde(default)]
    pub tab: Option<String>,
    /// Last ALSA rawmidi port used, reused on next launch when no `--port` is given.
    #[serde(default)]
    pub port: Option<String>,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            zoom: DEFAULT_ZOOM,
            window: None,
            tab: None,
            port: None,
        }
    }
}

fn default_zoom() -> f32 {
    DEFAULT_ZOOM
}

/// One cached patch-list row: the slot and its stored name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedRow {
    pub slot: u8,
    pub name: String,
}

/// The suite's per-device settings directory: `<config>/rackctl/eleven`.
pub(crate) fn settings_dir() -> Option<PathBuf> {
    rackctl_core::device_dir(DEVICE_ID)
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

/// Load the cached patch-bank names (empty if absent or unreadable).
pub(crate) fn load_cache() -> Vec<CachedRow> {
    let Some(path) = cache_path() else {
        return Vec::new();
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Best-effort save of the patch-bank name cache.
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

// ---- On-disk libraries: backups (loadable), .tfx patches (view), and scenes ----

/// Library directory for device backups (loadable `PatchBackup`s).
pub(crate) fn backups_dir() -> Option<PathBuf> {
    rackctl_core::library_dir(DEVICE_ID, "backups")
}

/// Library directory for imported `.tfx` patches (typed, view-only).
pub(crate) fn patches_dir() -> Option<PathBuf> {
    rackctl_core::library_dir(DEVICE_ID, "patches")
}

/// Library directory for whole-bank scenes.
pub(crate) fn scenes_dir() -> Option<PathBuf> {
    rackctl_core::library_dir(DEVICE_ID, "scenes")
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

/// Delete a library file by its directory + name. `Err` on failure.
pub(crate) fn delete_named(dir: Option<PathBuf>, name: &str) -> Result<(), String> {
    let path = dir
        .map(|d| d.join(format!("{}.json", rackctl_core::sanitize(name))))
        .ok_or_else(|| "no config directory".to_owned())?;
    rackctl_core::delete_file(&path)
}
