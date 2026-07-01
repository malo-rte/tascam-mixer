//! Control-surface library for the **Avid/Digidesign Eleven Rack** guitar
//! amp/effects processor.
//!
//! The Eleven Rack is edited over USB-MIDI using Digidesign address-mapped System
//! Exclusive: `F0 13 0B <dev> <opcode> <addr..> <value..> F7`, where `13` is
//! Digidesign, `0B` the Eleven Rack model id, `01` a read request, `12` its read
//! reply, `02` an unsolicited change report, and `00` a write/set.
//! This crate wraps that surface in a typed API: the `SysEx` codec ([`sysex`]), a
//! [`Transport`] seam with a mock and a real ALSA-rawmidi implementation, and an
//! [`Eleven`] device facade.
//!
//! The protocol was reverse-engineered from hardware (no public spec exists); see
//! `docs/eleven-rack-sysex-protocol.adoc`. Parameter addresses, value scaling and
//! the typed patch model are filled in later (`docs/eleven-rack-roadmap.adoc`).
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
//! use rackctl_eleven::{Eleven, MockTransport};
//!
//! let mut dev = Eleven::new(MockTransport::new());
//! let addr = [0x11, 0x21, 0x0D]; // amp Gain
//! dev.write(&addr, 0x6D)?;
//! assert_eq!(dev.read(&addr)?, 0x6D);
//! # Ok::<(), rackctl_eleven::Error>(())
//! ```
//!
//! NOTE: Eleven Rack, Digidesign and Avid are trademarks of Avid Technology, Inc.
//! This is an independent, unofficial project, not affiliated with Avid.
#![forbid(unsafe_code)]

mod backend;
mod device;

pub mod sysex;

// The data model lives in `rackctl-eleven-model`; re-export it so this crate's
// public surface carries the shared `Error`/`Result` and the value codec.
pub use rackctl_eleven_model::{
    AMP_GAIN, Block, BlockData, Error, Param, ParamRecord, Patch, PatchBackup, RawValue,
    RestoreAction, Result, backup, error, param, tfx, value,
};

#[cfg(feature = "alsa")]
pub use backend::RawMidi;
pub use backend::{MockTransport, Transport};
pub use device::Eleven;
pub use sysex::{DigiMessage, Identity};
