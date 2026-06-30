//! Transport abstraction: the [`Transport`] trait and its implementations.
//!
//! [`Transport`] is the narrow seam between the typed device API and the actual
//! MIDI link. It works at the Roland DT1/RQ1 level (address + data), not at the
//! raw `SysEx` byte level, so the device layer never builds frames itself. Keeping
//! it a trait lets the whole library run against an in-memory [`MockTransport`]
//! with no MIDI hardware or `libasound` present (rust-coding-rules RS-80).
//!
//! [`RawMidi`] keeps the Roland-specific protocol; its byte-level link — the ALSA
//! port and the cross-process lock — now lives in the shared `rackctl-midi` crate.

mod mock;
pub use mock::MockTransport;

#[cfg(feature = "alsa")]
mod rawmidi;
#[cfg(feature = "alsa")]
pub use rawmidi::RawMidi;

use crate::error::Result;

/// Address-mapped access to the device, at the Roland DT1/RQ1 level.
///
/// Implementors translate these calls into `SysEx` (or, for the mock, into an
/// in-memory map). `addr` is the raw address bytes; for the GX-700 catalog this
/// is a single byte today (see [`crate::param`]).
pub trait Transport {
    /// Write `data` at `addr` (a DT1 "set").
    fn send(&mut self, addr: &[u8], data: &[u8]) -> Result<()>;

    /// Request `len` bytes from `addr` (an RQ1) and return the reply data.
    fn request(&mut self, addr: &[u8], len: usize) -> Result<Vec<u8>>;

    /// Request a region (an RQ1 to a patch base) and return every DT1 the device
    /// streams in reply, as `(address, data)` pairs. Used to read a whole patch,
    /// which the GX-700 answers with one message per sub-block.
    fn request_blocks(&mut self, addr: &[u8], size: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Send a MIDI Program Change, selecting patch `program`.
    fn program_change(&mut self, program: u8) -> Result<()>;
}

/// Lets a `Gx700<Box<dyn Transport>>` hold either transport chosen at runtime
/// (e.g. mock vs hardware behind a command-line flag) without a wrapper enum.
/// Generic over the boxed type, so it also covers `Box<dyn Transport + Send>`.
impl<T: Transport + ?Sized> Transport for Box<T> {
    fn send(&mut self, addr: &[u8], data: &[u8]) -> Result<()> {
        (**self).send(addr, data)
    }
    fn request(&mut self, addr: &[u8], len: usize) -> Result<Vec<u8>> {
        (**self).request(addr, len)
    }
    fn request_blocks(&mut self, addr: &[u8], size: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        (**self).request_blocks(addr, size)
    }
    fn program_change(&mut self, program: u8) -> Result<()> {
        (**self).program_change(program)
    }
}
