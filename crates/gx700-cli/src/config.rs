//! On-disk location of saved GX-700 patches and scenes.
//!
//! Both live under the suite's per-device settings directory, mirroring the
//! US-16x08 tool's layout: individual patches in `<config>/rackctl/gx700/patches/`
//! and whole-device scenes in `<config>/rackctl/gx700/scenes/`.

use std::path::PathBuf;

use directories::ProjectDirs;

/// The per-device settings root (`<config>/rackctl/gx700/`), or `None` if no home
/// directory can be determined.
fn gx700_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "malo-rte", "rackctl").map(|dirs| dirs.config_dir().join("gx700"))
}

/// The directory holding saved patch files, or `None` if no home directory can
/// be determined.
pub(crate) fn patches_dir() -> Option<PathBuf> {
    gx700_dir().map(|dir| dir.join("patches"))
}

/// The directory holding saved scene files (whole-device snapshots).
pub(crate) fn scenes_dir() -> Option<PathBuf> {
    gx700_dir().map(|dir| dir.join("scenes"))
}

/// The path of the saved patch named `name` (`<patches_dir>/<name>.json`).
pub(crate) fn patch_path(name: &str) -> Option<PathBuf> {
    patches_dir().map(|dir| dir.join(format!("{}.json", sanitize(name))))
}

/// The path of the saved scene named `name` (`<scenes_dir>/<name>.json`).
pub(crate) fn scene_path(name: &str) -> Option<PathBuf> {
    scenes_dir().map(|dir| dir.join(format!("{}.json", sanitize(name))))
}

/// The names (file stems) of every saved patch, sorted.
pub(crate) fn saved_patches() -> Vec<String> {
    json_stems(patches_dir())
}

/// The names (file stems) of every saved scene, sorted.
pub(crate) fn saved_scenes() -> Vec<String> {
    json_stems(scenes_dir())
}

/// The sorted `.json` file stems in `dir`. Empty if the directory is missing.
fn json_stems(dir: Option<PathBuf>) -> Vec<String> {
    let Some(dir) = dir else {
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
