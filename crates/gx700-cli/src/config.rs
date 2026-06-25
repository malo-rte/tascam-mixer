//! On-disk location of saved GX-700 patches.
//!
//! Patches are stored as JSON files under the suite's per-device settings
//! directory, `<config>/rackctl/gx700/patches/`, mirroring the US-16x08 tool's
//! layout.

use std::path::PathBuf;

use directories::ProjectDirs;

/// The directory holding saved patch files, or `None` if no home directory can
/// be determined.
pub(crate) fn patches_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "malo-rte", "rackctl")
        .map(|dirs| dirs.config_dir().join("gx700").join("patches"))
}

/// The path of the saved patch named `name` (`<patches_dir>/<name>.json`).
pub(crate) fn patch_path(name: &str) -> Option<PathBuf> {
    patches_dir().map(|dir| dir.join(format!("{}.json", sanitize(name))))
}

/// The names (file stems) of every saved patch, sorted. Empty if the directory
/// does not exist.
pub(crate) fn saved_patches() -> Vec<String> {
    let Some(dir) = patches_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            e.path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}

/// Keep a filename safe: alphanumerics, space, dash, and underscore pass; any
/// other character becomes an underscore.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}
