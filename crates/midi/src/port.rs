//! A byte-level MIDI port over ALSA rawmidi.

use std::ffi::CString;
use std::io::{Read, Write};

use ::alsa::Direction;
use ::alsa::ctl::Ctl;
use ::alsa::rawmidi::{Iter as RawmidiIter, Rawmidi};

use crate::MidiError;
use crate::lock::PortLock;

/// A live, byte-level MIDI connection over ALSA rawmidi: input and output on one
/// `hw:CARD,DEV` endpoint, holding the per-port advisory [`PortLock`] for its
/// lifetime. The device protocol (message building, framing, pacing) is the
/// caller's concern; this just moves bytes.
pub struct MidiPort {
    output: Rawmidi,
    input: Rawmidi,
    /// Advisory lock excluding other rackctl processes from this port; held for the
    /// lifetime of the connection and released when it (or the process) ends.
    _lock: PortLock,
}

impl std::fmt::Debug for MidiPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiPort").finish_non_exhaustive()
    }
}

fn alsa_err(e: ::alsa::Error) -> MidiError {
    MidiError::Io(e.to_string())
}

/// Whether a read error just means "no data available yet" on a non-blocking
/// endpoint. ALSA reports `EAGAIN` with its negative-errno convention (`-11`),
/// which `io::Error::kind()` does not map to [`WouldBlock`][std::io::ErrorKind],
/// so check the raw code for both signs as well.
fn is_would_block(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::WouldBlock || matches!(e.raw_os_error(), Some(11 | -11))
}

impl MidiPort {
    /// Enumerate the ALSA rawmidi ports available on the system, as `hw:CARD,DEV`
    /// strings suitable for [`Self::open`].
    ///
    /// # Errors
    /// [`MidiError::Io`] if ALSA reports an error while iterating cards or devices.
    pub fn list_ports() -> Result<Vec<String>, MidiError> {
        let mut ports = Vec::new();
        for card in ::alsa::card::Iter::new() {
            let card = card.map_err(alsa_err)?;
            let index = card.get_index();
            let ctl = Ctl::new(&format!("hw:{index}"), false).map_err(alsa_err)?;
            for info in RawmidiIter::new(&ctl) {
                let info = info.map_err(alsa_err)?;
                // Each output device gives one addressable endpoint; list those.
                if info.get_stream() == Direction::Playback {
                    let port = format!("hw:{index},{}", info.get_device());
                    if !ports.contains(&port) {
                        ports.push(port);
                    }
                }
            }
        }
        Ok(ports)
    }

    /// Open the rawmidi port at `port` (a `hw:CARD,DEV` address) for both input and
    /// output, taking the advisory lock first.
    ///
    /// # Errors
    /// [`MidiError::PortBusy`] if another rackctl process already holds this port;
    /// [`MidiError::PortNotFound`] if the address contains an interior NUL;
    /// [`MidiError::Io`] if ALSA cannot open the input or output stream.
    pub fn open(port: &str) -> Result<Self, MidiError> {
        // Take the advisory lock first: if another rackctl process owns the port,
        // fail fast with a clear error rather than opening it and corrupting both
        // sides' reply streams.
        let lock = PortLock::acquire(port)?;
        let cname = CString::new(port).map_err(|_| MidiError::PortNotFound(port.to_owned()))?;
        let output = Rawmidi::open(&cname, Direction::Playback, false).map_err(alsa_err)?;
        // Open the input non-blocking so reads can poll for a timeout.
        let input = Rawmidi::open(&cname, Direction::Capture, true).map_err(alsa_err)?;
        Ok(Self {
            output,
            input,
            _lock: lock,
        })
    }

    /// Write a complete byte buffer to the output port.
    ///
    /// # Errors
    /// [`MidiError::Io`] if the write fails.
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), MidiError> {
        self.output
            .io()
            .write_all(bytes)
            .map_err(|e| MidiError::Io(e.to_string()))
    }

    /// Read whatever input is available right now into `buf`, without blocking:
    /// `Ok(0)` when there is no data yet (the caller polls and retries), `Ok(n)`
    /// for `n` bytes read, `Err` on a real failure.
    ///
    /// # Errors
    /// [`MidiError::Io`] if the read fails for a reason other than "no data yet".
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, MidiError> {
        match self.input.io().read(buf) {
            Ok(n) => Ok(n),
            Err(e) if is_would_block(&e) => Ok(0),
            Err(e) => Err(MidiError::Io(e.to_string())),
        }
    }
}
