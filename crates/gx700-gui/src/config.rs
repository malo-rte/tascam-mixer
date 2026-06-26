//! Persisted GUI state and an on-disk cache of the patch bank, so a relaunch shows
//! the list instantly instead of re-reading 100 patches (~1 minute) every time.

use std::path::PathBuf;

use directories::ProjectDirs;
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
