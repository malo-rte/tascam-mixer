//! The named on-disk libraries: save / load / list patches, scenes, and per-block
//! presets, located via rackctl-core's path conventions and written in the
//! envelope format. The frontends call these instead of touching files directly.

use std::path::PathBuf;

use rackctl_gx700::Block;
use rackctl_gx700::typed::{BlockData, Patch};

use crate::format::{Scene, parse_block, parse_patch, parse_scene};
use crate::{DEVICE_ID, LIB_VERSION, block_dir_name};

/// Library kind (subdirectory) for single patches.
const PATCHES: &str = "patches";
/// Library kind (subdirectory) for whole-device scenes.
const SCENES: &str = "scenes";

fn no_dir() -> String {
    "no config directory available".to_owned()
}

/// The `blocks/<type>` kind for a block's preset library.
fn block_kind(block: Block) -> String {
    format!("blocks/{}", block_dir_name(block))
}

fn read(kind: &str, name: &str) -> Result<String, String> {
    let file = rackctl_core::item_path(DEVICE_ID, kind, name).ok_or_else(no_dir)?;
    rackctl_core::read_text(&file).ok_or_else(|| format!("could not read {}", file.display()))
}

// ---- Patches ----

/// Save `patch` to the patch library as `name`, returning the file path.
///
/// # Errors
/// If no config directory is available, or the write fails.
pub fn save_patch(name: &str, patch: &Patch) -> Result<PathBuf, String> {
    let file = rackctl_core::item_path(DEVICE_ID, PATCHES, name).ok_or_else(no_dir)?;
    rackctl_core::save_item(&file, DEVICE_ID, LIB_VERSION, patch)?;
    Ok(file)
}

/// Load the named patch from the patch library (any supported format).
///
/// # Errors
/// If the file is missing/unreadable or matches no known format.
pub fn load_patch(name: &str) -> Result<Patch, String> {
    parse_patch(&read(PATCHES, name)?)
}

/// The names of saved patches, sorted.
#[must_use]
pub fn list_patches() -> Vec<String> {
    rackctl_core::list_stems(DEVICE_ID, PATCHES)
}

// ---- Scenes ----

/// Save `scene` to the scene library under its own `name`, returning the file path.
///
/// # Errors
/// If `scene.name` is unusable as a path, or the write fails.
pub fn save_scene(scene: &Scene) -> Result<PathBuf, String> {
    let file = rackctl_core::item_path(DEVICE_ID, SCENES, &scene.name).ok_or_else(no_dir)?;
    rackctl_core::save_item(&file, DEVICE_ID, LIB_VERSION, scene)?;
    Ok(file)
}

/// Load the named scene (any supported format). A legacy file that carried no name
/// in its payload gets `name` filled in.
///
/// # Errors
/// If the file is missing/unreadable or matches no known format.
pub fn load_scene(name: &str) -> Result<Scene, String> {
    let mut scene = parse_scene(&read(SCENES, name)?)?;
    if scene.name.is_empty() {
        name.clone_into(&mut scene.name);
    }
    Ok(scene)
}

/// The names of saved scenes, sorted.
#[must_use]
pub fn list_scenes() -> Vec<String> {
    rackctl_core::list_stems(DEVICE_ID, SCENES)
}

// ---- Per-block presets ----

/// Save `data` to `block`'s preset library as `name`, returning the file path.
///
/// # Errors
/// If no config directory is available, or the write fails.
pub fn save_block(block: Block, name: &str, data: &BlockData) -> Result<PathBuf, String> {
    let file = rackctl_core::item_path(DEVICE_ID, &block_kind(block), name).ok_or_else(no_dir)?;
    rackctl_core::save_item(&file, DEVICE_ID, LIB_VERSION, data)?;
    Ok(file)
}

/// Load the named preset from `block`'s preset library (any supported format).
///
/// # Errors
/// If the file is missing/unreadable or matches no known format.
pub fn load_block(block: Block, name: &str) -> Result<BlockData, String> {
    parse_block(&read(&block_kind(block), name)?)
}

/// The names of saved presets for `block`, sorted.
#[must_use]
pub fn list_blocks(block: Block) -> Vec<String> {
    rackctl_core::list_stems(DEVICE_ID, &block_kind(block))
}
