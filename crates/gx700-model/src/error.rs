//! Error and result types for the GX-700 stack.
//!
//! This is the shared error vocabulary for the whole GX-700 stack (model and
//! protocol). It lives in the model crate so both layers can use one `Error`; the
//! data model produces the validation/patch variants, and the protocol crate
//! produces the transport ones (mapping its link errors in — see
//! `rackctl-gx700`'s `RawMidi`).

use thiserror::Error;

/// Convenience alias for results returned across the GX-700 crates.
pub type Result<T> = core::result::Result<T, Error>;

/// Everything that can go wrong while modelling or talking to a BOSS GX-700.
///
/// The variants deliberately avoid exposing backend-specific error types
/// (e.g. `alsa::Error` or `std::io::Error`) so the surface stays
/// transport-agnostic (rust-coding-rules RS-63); a transport folds its own
/// errors into [`Error::Transport`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No ALSA rawmidi port matched the requested `hw:CARD,DEV` address.
    #[error("MIDI port not found: {0}")]
    PortNotFound(String),

    /// Another rackctl process already holds the advisory lock for this port.
    /// Only one accessor may drive a MIDI interface at a time: two readers split
    /// the device's reply stream between them, so neither receives a complete
    /// message (rust-coding-rules notwithstanding, this is a hardware truth, not
    /// a software limit — see `docs/midi-arbitration.adoc`).
    #[error("MIDI port {0} is in use by another rackctl process")]
    PortBusy(String),

    /// A parameter key did not resolve to any cataloged parameter.
    #[error("unknown parameter {0:?}")]
    UnknownParam(String),

    /// An integer/enum value lies outside the parameter's permitted range.
    #[error("value {value} out of range for {param} (expected {min}..={max})")]
    ValueOutOfRange {
        /// Human-readable parameter key.
        param: &'static str,
        /// The offending value.
        value: i32,
        /// Inclusive minimum.
        min: i32,
        /// Inclusive maximum.
        max: i32,
    },

    /// The supplied [`crate::Value`] kind does not match the parameter's kind.
    #[error("type mismatch for {param}: expected {expected}")]
    TypeMismatch {
        /// Human-readable parameter key.
        param: &'static str,
        /// The value kind the parameter expects.
        expected: &'static str,
    },

    /// A System Exclusive message was malformed: bad header, wrong manufacturer
    /// or model id, or a checksum mismatch. The string carries the detail.
    #[error("sysex error: {0}")]
    Sysex(String),

    /// The underlying MIDI transport failed; the string carries its message.
    #[error("transport error: {0}")]
    Transport(String),

    /// A reply was expected from the device but none arrived in time.
    #[error("timed out waiting for a device reply")]
    Timeout,

    /// A patch could not be captured or applied (wrong kind for a parameter, or
    /// a value that does not fit it).
    #[error("patch error: {0}")]
    Patch(String),
}
