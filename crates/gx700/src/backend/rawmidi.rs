//! The real MIDI [`Transport`], backed by ALSA's rawmidi interface.
//!
//! Opens a `hw:CARD,DEV` rawmidi endpoint for both input and output, writes
//! Roland DT1/RQ1 `SysEx`, and frames replies with [`crate::sysex::Framer`]. This
//! file and its port enumeration are manufacturer-independent and are a prime
//! candidate to lift into a shared `rackctl-core` later.
//!
//! This path is exercised only on hardware; CI and tests use the mock.

use std::ffi::CString;
use std::io::{Read, Write};
use std::time::Duration;

use ::alsa::Direction;
use ::alsa::ctl::Ctl;
use ::alsa::rawmidi::{Iter as RawmidiIter, Rawmidi};

use super::Transport;
use crate::error::{Error, Result};
use crate::sysex::{self, DT1, Framer};

/// Default device id used in the Roland `SysEx` header (`F0 41 <dev> 79 ...`).
///
/// The GX-700 ships listening on device id `0x00`; firm up if a unit is found
/// configured otherwise.
const DEFAULT_DEVICE_ID: u8 = 0x00;

/// Pause between non-blocking read polls while waiting for a reply.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// How many [`POLL_INTERVAL`] polls to wait for a DT1 reply before giving up
/// (about 500 ms). A bounded poll count avoids reaching for an injectable
/// clock on this hardware-only path.
const REPLY_POLLS: u32 = 500;

/// Consecutive silent [`POLL_INTERVAL`] polls that end an input drain (~35 ms).
/// Comfortably longer than the ~20 ms gap between the messages of a streamed
/// whole-patch reply, so a single gap does not end the drain early.
const DRAIN_QUIET_POLLS: u32 = 35;

/// Silence (~300 ms) that ends a whole-patch collection even if the final
/// sub-block was not recognised: a generous fallback past the longer gaps the
/// device leaves before large sub-blocks (e.g. Modulation).
const STREAM_QUIET_POLLS: u32 = 300;

/// Sub-block offset of the last block in a patch (Reverb); once collected, the
/// patch stream is complete.
const LAST_SUB_BLOCK: u8 = 0x0D;

/// A live connection to a GX-700 over ALSA rawmidi.
pub struct RawMidi {
    output: Rawmidi,
    input: Rawmidi,
    device_id: u8,
}

impl std::fmt::Debug for RawMidi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawMidi")
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

fn transport_err(e: ::alsa::Error) -> Error {
    Error::Transport(e.to_string())
}

fn io_err(e: &std::io::Error) -> Error {
    Error::Transport(e.to_string())
}

/// Whether a read error just means "no data available yet" on a non-blocking
/// endpoint. ALSA reports `EAGAIN` with its negative-errno convention (`-11`),
/// which `io::Error::kind()` does not map to [`WouldBlock`][std::io::ErrorKind],
/// so check the raw code for both signs as well.
fn is_would_block(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::WouldBlock || matches!(e.raw_os_error(), Some(11 | -11))
}

impl RawMidi {
    /// Enumerate the ALSA rawmidi ports available on the system, as
    /// `hw:CARD,DEV` strings suitable for [`Self::open`].
    ///
    /// # Errors
    /// [`Error::Transport`] if ALSA reports an error while iterating cards or
    /// devices.
    pub fn ports() -> Result<Vec<String>> {
        let mut ports = Vec::new();
        for card in ::alsa::card::Iter::new() {
            let card = card.map_err(transport_err)?;
            let index = card.get_index();
            let ctl = Ctl::new(&format!("hw:{index}"), false).map_err(transport_err)?;
            for info in RawmidiIter::new(&ctl) {
                let info = info.map_err(transport_err)?;
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

    /// Open the rawmidi port at `port` (a `hw:CARD,DEV` address) for both input
    /// and output.
    ///
    /// # Errors
    /// [`Error::PortNotFound`] if the address contains an interior NUL;
    /// [`Error::Transport`] if ALSA cannot open the input or output stream.
    pub fn open(port: &str) -> Result<Self> {
        let cname = CString::new(port).map_err(|_| Error::PortNotFound(port.to_owned()))?;
        let output = Rawmidi::open(&cname, Direction::Playback, false).map_err(transport_err)?;
        // Open the input non-blocking so reads can poll for a timeout.
        let input = Rawmidi::open(&cname, Direction::Capture, true).map_err(transport_err)?;
        Ok(Self {
            output,
            input,
            device_id: DEFAULT_DEVICE_ID,
        })
    }

    /// Print every incoming complete `SysEx` message as hex, one per line,
    /// until interrupted. A reverse-engineering aid for mapping the parameter
    /// addresses the unit emits when its knobs move or it dumps a patch.
    ///
    /// # Errors
    /// [`Error::Transport`] if a read fails for a reason other than there being
    /// no data yet.
    pub fn watch_sysex(&mut self) -> Result<()> {
        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        loop {
            match self.input.io().read(&mut buf) {
                Ok(0) => std::thread::sleep(POLL_INTERVAL),
                Ok(n) => {
                    let chunk = buf.get(..n).unwrap_or(&[]);
                    for msg in framer.push(chunk) {
                        let hex: Vec<String> = msg.iter().map(|b| format!("{b:02X}")).collect();
                        println!("{}", hex.join(" "));
                    }
                }
                Err(e) if is_would_block(&e) => {
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(e) => return Err(io_err(&e)),
            }
        }
    }

    /// Print every incoming MIDI message, decoded to one line each, until
    /// interrupted. A general listener (notes, CC, program change, pitch bend,
    /// `SysEx`, real-time), for debugging the link.
    ///
    /// # Errors
    /// [`Error::Transport`] if a read fails for a reason other than no data yet.
    pub fn watch_midi(&mut self) -> Result<()> {
        let mut decoder = crate::monitor::MidiDecoder::new();
        let mut buf = [0u8; 256];
        loop {
            match self.input.io().read(&mut buf) {
                Ok(0) => std::thread::sleep(POLL_INTERVAL),
                Ok(n) => {
                    let chunk = buf.get(..n).unwrap_or(&[]);
                    for line in decoder.push(chunk) {
                        println!("{line}");
                    }
                }
                Err(e) if is_would_block(&e) => std::thread::sleep(POLL_INTERVAL),
                Err(e) => return Err(io_err(&e)),
            }
        }
    }

    /// Write a complete byte buffer to the output port.
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.output.io().write_all(bytes).map_err(|e| io_err(&e))
    }

    /// Discard any pending input, so a stale reply left over from a previous
    /// request cannot be mistaken for the answer to the next one.
    fn drain_input(&mut self) {
        let mut buf = [0u8; 256];
        // Drain until the input has been silent for DRAIN_QUIET_POLLS in a row.
        // A whole-patch reply streams as many messages ~20 ms apart, so a single
        // gap is not the end; only sustained silence is.
        let mut quiet = 0u32;
        while quiet < DRAIN_QUIET_POLLS {
            match self.input.io().read(&mut buf) {
                Ok(n) if n > 0 => quiet = 0,
                _ => {
                    quiet = quiet.saturating_add(1);
                    std::thread::sleep(POLL_INTERVAL);
                }
            }
        }
    }

    /// Read and frame `SysEx` replies until a DT1 message *for `addr`* arrives,
    /// or the poll budget runs out. Returns the data bytes that follow the
    /// echoed address. DT1s for other addresses are skipped, so a single-byte
    /// read cannot be satisfied by a leftover reply to a different request.
    fn read_dt1_reply(&mut self, addr: &[u8]) -> Result<Vec<u8>> {
        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        for _ in 0..REPLY_POLLS {
            match self.input.io().read(&mut buf) {
                Ok(0) => std::thread::sleep(POLL_INTERVAL),
                Ok(n) => {
                    let chunk = buf.get(..n).unwrap_or(&[]);
                    for msg in framer.push(chunk) {
                        let Ok(parsed) = sysex::parse_roland(&msg) else {
                            continue;
                        };
                        if parsed.command != DT1 {
                            continue;
                        }
                        if let Some(data) = parsed.body.strip_prefix(addr) {
                            return Ok(data.to_vec());
                        }
                    }
                }
                Err(e) if is_would_block(&e) => {
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(e) => return Err(io_err(&e)),
            }
        }
        Err(Error::Timeout)
    }

    /// Collect every DT1 the device streams, until it has been silent for a
    /// drain-quiet window (after at least one message). Each is split into its
    /// 4-byte address and the data that follows.
    fn collect_dt1_stream(&mut self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        const ADDR_LEN: usize = 4;
        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        let mut idle = 0u32;
        let mut waited = 0u32;
        loop {
            match self.input.io().read(&mut buf) {
                Ok(n) if n > 0 => {
                    idle = 0;
                    let chunk = buf.get(..n).unwrap_or(&[]);
                    for msg in framer.push(chunk) {
                        let Ok(parsed) = sysex::parse_roland(&msg) else {
                            continue;
                        };
                        if parsed.command != DT1 {
                            continue;
                        }
                        let split = parsed.body.len().min(ADDR_LEN);
                        let (addr, data) = parsed.body.split_at(split);
                        out.push((addr.to_vec(), data.to_vec()));
                    }
                }
                _ => {
                    idle = idle.saturating_add(1);
                    waited = waited.saturating_add(1);
                    // Done once the final sub-block has arrived and briefly
                    // settled; or after a long silence (in case the last block
                    // was not recognised); or, with nothing yet, after the reply
                    // budget.
                    let have_last = out.iter().any(|(addr, _)| addr.get(2) == Some(&LAST_SUB_BLOCK));
                    if !out.is_empty() && have_last && idle >= DRAIN_QUIET_POLLS {
                        break;
                    }
                    if !out.is_empty() && idle >= STREAM_QUIET_POLLS {
                        break;
                    }
                    if out.is_empty() && waited >= REPLY_POLLS {
                        break;
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
            }
        }
        if out.is_empty() {
            return Err(Error::Timeout);
        }
        Ok(out)
    }
}

impl Transport for RawMidi {
    fn send(&mut self, addr: &[u8], data: &[u8]) -> Result<()> {
        let msg = sysex::build_dt1(self.device_id, addr, data);
        self.write_all(&msg)
    }

    fn request(&mut self, addr: &[u8], len: usize) -> Result<Vec<u8>> {
        // Clear any unread reply from a previous request first.
        self.drain_input();

        // RQ1 size field mirrors the address width, big-endian.
        let mut size = vec![0u8; addr.len().max(1)];
        if let Some(last) = size.last_mut() {
            *last = u8::try_from(len & 0x7f).unwrap_or(0x7f);
        }
        let msg = sysex::build_rq1(self.device_id, addr, &size);
        self.write_all(&msg)?;

        let mut out = self.read_dt1_reply(addr)?;
        out.resize(len, 0);
        Ok(out)
    }

    fn request_blocks(&mut self, addr: &[u8], size: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.drain_input();
        // Encode size as 7-bit bytes, big-endian, across the size field (the same
        // width as the address). A single low byte would truncate e.g. 0x200 to 0.
        let mut sz = vec![0u8; addr.len().max(1)];
        let mut remaining = size;
        for byte in sz.iter_mut().rev() {
            *byte = u8::try_from(remaining & 0x7f).unwrap_or(0);
            remaining >>= 7;
        }
        let msg = sysex::build_rq1(self.device_id, addr, &sz);
        self.write_all(&msg)?;
        self.collect_dt1_stream()
    }

    fn program_change(&mut self, program: u8) -> Result<()> {
        self.write_all(&[0xC0, program & 0x7f])
    }
}
