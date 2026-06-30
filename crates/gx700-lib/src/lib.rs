//! GX-700 **library and patch-management** layer.
//!
//! This is the device-specific half of the extracted library stack: it sits on
//! [`rackctl_gx700`] (the device protocol + typed model) and [`rackctl_core`] (the
//! device-neutral on-disk library) and provides what both the CLI and the GUI need
//! but neither should own privately — so they can't drift on it again:
//!
//! * [`mod@format`] — the saved-file formats and **multi-format parsing** (a
//!   rackctl-core envelope, a bare typed value, or a legacy form), plus the
//!   canonical [`Scene`] (a whole-device snapshot).
//! * [`library`] — the named on-disk libraries: save / load / list patches, scenes,
//!   and per-block-type presets, located via rackctl-core's path conventions.
//!
//! The device-touching management operations (backup a bank, capture / restore a
//! scene, copy a slot) move here in a later step; for now they live in the CLI.
#![forbid(unsafe_code)]

use rackctl_gx700::Block;

pub mod format;
pub mod library;

pub use format::{Scene, parse_block, parse_patch, parse_scene};

/// This device's stable id, stamped into every saved library item (the rackctl-core
/// envelope) so a file is matched to the GX-700 on load.
pub const DEVICE_ID: &str = "gx700";

/// Current on-disk library format version. Bump when the envelope or a payload shape
/// changes; older versions load (with migration), newer ones are refused.
pub const LIB_VERSION: u32 = 1;

/// Stable on-disk subdirectory name for a block type's preset library
/// (`blocks/<name>`). It is part of the file path, so it must stay stable.
#[must_use]
pub fn block_dir_name(block: Block) -> &'static str {
    match block {
        Block::Compressor => "compressor",
        Block::Wah => "wah",
        Block::Distortion => "distortion",
        Block::Preamp => "preamp",
        Block::Loop => "loop",
        Block::Equalizer => "equalizer",
        Block::SpeakerSim => "speaker_sim",
        Block::NoiseSuppressor => "noise_suppressor",
        Block::Modulation => "modulation",
        Block::Delay => "delay",
        Block::Chorus => "chorus",
        Block::TremoloPan => "tremolo_pan",
        Block::Reverb => "reverb",
        Block::LevelChain => "level_chain",
        _ => "other",
    }
}
