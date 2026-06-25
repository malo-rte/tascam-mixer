//! Persisted GUI-only state. The stereo-link grouping is not a hardware control
//! (the driver has no link element), so it lives here rather than in the
//! device's JSON presets.

use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Default interface zoom factor (egui zoom), used when no config exists.
pub(crate) const DEFAULT_ZOOM: f32 = 1.5;

/// GUI state saved between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GuiConfig {
    /// Stereo-link state for the eight adjacent channel pairs (0/1 .. 14/15).
    #[serde(default)]
    pub links: [bool; 8],
    /// Interface zoom factor, restored on startup and on `Load default`.
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    /// Saved window inner size in logical points (`[width, height]`), or `None`
    /// to use the default size. Restored on startup and on `Load default`.
    #[serde(default)]
    pub window: Option<[f32; 2]>,
    /// User-given names for the 16 input channels (GUI-only), empty when unset.
    #[serde(default = "default_names")]
    pub names: [String; 16],
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            links: [false; 8],
            zoom: DEFAULT_ZOOM,
            window: None,
            names: default_names(),
        }
    }
}

fn default_zoom() -> f32 {
    DEFAULT_ZOOM
}

fn default_names() -> [String; 16] {
    std::array::from_fn(|_| String::new())
}

fn config_path() -> Option<PathBuf> {
    ProjectDirs::from("de", "paraair", "tascam-mixer")
        .map(|dirs| dirs.config_dir().join("config.json"))
}

/// Path to the shared default-mixer preset, in the same config directory the
/// CLI's `default` command uses. `None` if no home directory can be determined.
pub(crate) fn default_preset_path() -> Option<PathBuf> {
    ProjectDirs::from("de", "paraair", "tascam-mixer")
        .map(|dirs| dirs.config_dir().join("default-preset.json"))
}

/// Directory holding the user's saved scenes (whole-mixer presets), under the
/// config directory. `None` if no home directory can be determined.
pub(crate) fn scenes_dir() -> Option<PathBuf> {
    ProjectDirs::from("de", "paraair", "tascam-mixer").map(|dirs| dirs.config_dir().join("scenes"))
}

/// Directory holding the user's saved channel presets (single-channel strips),
/// under the config directory. `None` if no home directory can be determined.
pub(crate) fn strips_dir() -> Option<PathBuf> {
    ProjectDirs::from("de", "paraair", "tascam-mixer").map(|dirs| dirs.config_dir().join("strips"))
}

/// Directory holding the user's saved EQ presets, under the config directory.
pub(crate) fn eq_dir() -> Option<PathBuf> {
    ProjectDirs::from("de", "paraair", "tascam-mixer").map(|dirs| dirs.config_dir().join("eq"))
}

/// Directory holding the user's saved compressor presets, under the config
/// directory.
pub(crate) fn comp_dir() -> Option<PathBuf> {
    ProjectDirs::from("de", "paraair", "tascam-mixer").map(|dirs| dirs.config_dir().join("comp"))
}

/// Load the saved config, falling back to defaults on any error.
pub(crate) fn load() -> GuiConfig {
    let Some(path) = config_path() else {
        return GuiConfig::default();
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Best-effort save; failures are ignored (GUI state is not critical).
pub(crate) fn save(config: &GuiConfig) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(&path, text);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::GuiConfig;

    #[test]
    fn config_without_names_loads_with_empty_defaults() {
        // A config written before channel names existed must still load.
        let json = r#"{"links":[false,false,false,false,false,false,false,false],"zoom":1.5}"#;
        let cfg: GuiConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.names.len(), 16);
        assert!(cfg.names.iter().all(String::is_empty));
    }

    #[test]
    fn names_round_trip_through_json() {
        let mut cfg = GuiConfig::default();
        cfg.names[0] = "Kick".to_owned();
        cfg.names[15] = "Vox".to_owned();
        let text = serde_json::to_string(&cfg).unwrap();
        let back: GuiConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(back.names[0], "Kick");
        assert_eq!(back.names[15], "Vox");
    }
}
