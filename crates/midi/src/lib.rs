//! `rackctl-midi` — the byte-level MIDI link for the Rackctl suite.
//!
//! A [`MidiPort`] is a raw `hw:CARD,DEV` ALSA rawmidi endpoint: open it, send and
//! (non-blocking) receive **bytes**, list the available ports, and exclude other
//! rackctl processes from the same interface via a [`PortLock`]. It is
//! manufacturer-independent — a device crate layers its own protocol (Roland `SysEx`
//! for the GX-700, …) on top, building and framing messages from these bytes.
//!
//! The link is the seam the rest of the stack is written against. Today the only
//! implementation is this direct ALSA port; when the arbitration daemon lands, a
//! daemon-client link slots in beside it (a tool that owns the device serving
//! several clients), and nothing above the link changes.
#![forbid(unsafe_code)]

mod lock;
mod port;

pub use lock::PortLock;
pub use port::MidiPort;

use thiserror::Error;

/// An error from the MIDI link.
#[derive(Debug, Error)]
pub enum MidiError {
    /// The requested `hw:CARD,DEV` port does not exist (or the name is invalid).
    #[error("MIDI port not found: {0}")]
    PortNotFound(String),
    /// Another rackctl process already holds this port's advisory lock.
    #[error("MIDI port {0} is in use by another rackctl process")]
    PortBusy(String),
    /// An ALSA or I/O failure talking to the port.
    #[error("MIDI I/O error: {0}")]
    Io(String),
}
