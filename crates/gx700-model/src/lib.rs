//! The **BOSS GX-700 data model** — pure data, no MIDI and no I/O.
//!
//! Everything here describes *what a GX-700 patch is*, independently of how it is
//! read from or written to the hardware (that is the protocol crate,
//! `rackctl-gx700`). So a tool that only needs to read, edit, validate or convert
//! GX-700 patch *files* can depend on this crate alone — no ALSA, no transport.
//!
//! * [`typed`] — the working representation: the patch as named per-block structs
//!   ([`typed::Compressor`], [`typed::Reverb`], …, [`typed::Patch`]) plus the
//!   patch-global fields, every parameter typed and range-checked.
//! * [`patch`] — the byte-exact [`RawPatch`] (the on-the-wire form), the
//!   patch-global helpers (name, output level, signal chain), and the cataloged
//!   [`Patch`] snapshot. The `typed` <-> raw conversion is **byte-exact**.
//! * [`param`] — the parameter catalog: every parameter's block, kind, range,
//!   encoding and default; plus value validation and the control-assign target map.
//! * [`units`] — display formatting for parameter values (dB, ms, %, …).
//! * [`scene`] — the legacy whole-bank scene snapshot.
//!
//! The dependencies are only `serde` (for the on-disk JSON form) and `thiserror`.
#![forbid(unsafe_code)]

pub mod error;
pub mod param;
pub mod patch;
pub mod scene;
pub mod typed;
pub mod units;

pub use error::{Error, Result};
pub use param::{Block, Encoding, Kind, Param, Value};
pub use patch::{
    NAME_LEN, PATCH_VERSION, Patch, PatchHeader, RawPatch, Scalar, decode_name, encode_name,
    patch_base,
};
pub use scene::{SCENE_VERSION, Scene, USER_PATCH_COUNT};
