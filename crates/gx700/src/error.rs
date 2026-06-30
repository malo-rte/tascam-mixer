//! Error and result types for the crate.

use thiserror::Error;

/// Convenience alias for results returned across this crate.
pub type Result<T> = core::result::Result<T, Error>;

/// Everything that can go wrong while talking to a BOSS GX-700.
///
/// The variants deliberately avoid exposing backend-specific error types
/// (e.g. `alsa::Error` or `std::io::Error`) so the public surface stays
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

/// Map a byte-level MIDI link error onto this crate's [`enum@Error`], so the Roland
/// transport can use `?` over `rackctl-midi` calls.
#[cfg(feature = "alsa")]
impl From<rackctl_midi::MidiError> for Error {
    fn from(e: rackctl_midi::MidiError) -> Self {
        match e {
            rackctl_midi::MidiError::PortBusy(p) => Error::PortBusy(p),
            rackctl_midi::MidiError::PortNotFound(p) => Error::PortNotFound(p),
            rackctl_midi::MidiError::Io(s) => Error::Transport(s),
        }
    }
}
