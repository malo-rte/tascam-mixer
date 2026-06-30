//! Control-surface library for the **BOSS GX-700** guitar effects processor.
//!
//! The GX-700 is edited over DIN MIDI using Roland address-mapped System
//! Exclusive: `F0 41 <dev> 79 <cmd> <addr..> <data..> <checksum> F7`, where
//! `41` is Roland, `79` the GX-700 model id, `12` a DT1 (set) and `11` an RQ1
//! (request). This crate wraps that surface in a typed API: a parameter catalog
//! ([`param`]), a [`Transport`] seam with a mock and a real ALSA-rawmidi
//! implementation, a [`Gx700`] device facade, and a JSON [`Patch`] model.
//!
//! The `SysEx` codec ([`sysex`]) is split into a manufacturer-independent framer
//! and a Roland-specific builder/parser, so the generic half lifts cleanly into
//! a shared crate when more MIDI devices join the suite.
//!
//! # Backends
//!
//! All access goes through the [`Transport`] trait:
//! - [`RawMidi`] (feature `alsa`, on by default) talks to real hardware via
//!   ALSA rawmidi.
//! - [`MockTransport`] is an in-memory stand-in needing no MIDI port or
//!   `libasound`, for development and tests.
//!
//! # Example
//!
//! ```
//! use rackctl_gx700::{Gx700, MockTransport, Param, Value};
//!
//! let mut dev = Gx700::new(MockTransport::new());
//! let preamp_volume = Param::from_key("preamp-volume").expect("known parameter");
//!
//! dev.set(preamp_volume, Value::Int(80))?;
//! assert_eq!(dev.get(preamp_volume)?, Value::Int(80));
//! # Ok::<(), rackctl_gx700::Error>(())
//! ```
//!
//! # Catalog status
//!
//! Parameter addresses and ranges are transcribed from the Roland *GX-700 MIDI
//! Implementation* and confirmed against hardware (see
//! `docs/gx700-sysex-protocol.adoc`). Values are raw device units; a few
//! multi-byte parameters and the Modulation per-type matrix are not yet exposed.

mod backend;
mod device;
mod patch_io;

pub mod monitor;
pub mod sysex;

// The data model lives in `rackctl-gx700-model`; re-export it so this crate's
// public surface is unchanged (`rackctl_gx700::typed::Compressor`, `::param`, the
// `RawPatch`/`Patch` types, the shared `Error`, …). Internally, `crate::param`,
// `crate::error`, etc. resolve to these re-exports too.
pub use rackctl_gx700_model::{
    Block, Encoding, Error, Kind, NAME_LEN, PATCH_VERSION, Param, Patch, PatchHeader, RawPatch,
    Result, SCENE_VERSION, Scalar, Scene, USER_PATCH_COUNT, Value, decode_name, encode_name,
    patch_base,
};
pub use rackctl_gx700_model::{error, param, patch, scene, typed, units};

#[cfg(feature = "alsa")]
pub use backend::RawMidi;
pub use backend::{MockTransport, Transport};
pub use device::Gx700;
pub use monitor::MidiDecoder;
pub use sysex::{Framer, RolandMessage};
