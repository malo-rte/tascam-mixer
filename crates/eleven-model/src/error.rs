//! Error and result types for the Eleven Rack stack.
//!
//! This is the shared error vocabulary for the whole Eleven Rack stack (model and
//! protocol). It lives in the model crate so both layers can use one [`enum@Error`];
//! the protocol crate folds its byte-level link errors into [`Error::Transport`]
//! and friends at the edge.

use thiserror::Error;

/// Convenience alias for results returned across the Eleven Rack crates.
pub type Result<T> = core::result::Result<T, Error>;

/// Everything that can go wrong while modelling or talking to an Eleven Rack.
///
/// The variants deliberately avoid exposing backend-specific error types
/// (e.g. `alsa::Error`) so the surface stays transport-agnostic
/// (rust-coding-rules RS-63); a transport folds its own errors into
/// [`Error::Transport`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No ALSA rawmidi port matched the requested `hw:CARD,DEV` address.
    #[error("MIDI port not found: {0}")]
    PortNotFound(String),

    /// Another rackctl process already holds the advisory lock for this port.
    /// Only one accessor may drive a MIDI interface at a time (see
    /// `docs/midi-arbitration.adoc`).
    #[error("MIDI port {0} is in use by another rackctl process")]
    PortBusy(String),

    /// A System Exclusive message was malformed: bad framing, wrong manufacturer
    /// or model id, or an unexpected length. The string carries the detail.
    #[error("sysex error: {0}")]
    Sysex(String),

    /// The underlying MIDI transport failed; the string carries its message.
    #[error("transport error: {0}")]
    Transport(String),

    /// A reply was expected from the device but none arrived in time.
    #[error("timed out waiting for a device reply")]
    Timeout,

    /// A patch (`.tfx`) file was malformed: too short, or a bad block header.
    #[error("tfx error: {0}")]
    Tfx(String),
}
