//! The real Eleven Rack [`Transport`], layering Digidesign read/write `SysEx`
//! over the byte-level [`MidiPort`] link from `rackctl-midi`.
//!
//! This file owns only the *Digidesign-specific* protocol: building read/write
//! messages, framing replies with [`crate::sysex::Framer`], and matching a reply
//! to its request address. Opening the port, listing ports, the advisory lock and
//! the raw byte I/O all live in `rackctl-midi`.
//!
//! This path is exercised only on hardware; CI and tests use the mock.

use std::thread::sleep;
use std::time::Duration;

use rackctl_midi::MidiPort;

use super::Transport;
use crate::sysex::{self, CHANGE_REPORT, Framer, Identity, READ_REPLY};
use rackctl_eleven_model::error::{Error, Result};
use rackctl_eleven_model::value::{RawValue, VALUE_LEN};

/// Pause between non-blocking read polls while waiting for a reply.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// How many [`POLL_INTERVAL`] polls to wait for a read reply before giving up
/// (about 500 ms).
const REPLY_POLLS: u32 = 500;

/// Consecutive silent [`POLL_INTERVAL`] polls that end an input drain (~20 ms).
const DRAIN_QUIET_POLLS: u32 = 20;

/// Silence (~50 ms) that ends collecting a batch scan's replies, once at least
/// one has arrived. Comfortably longer than the gap between streamed replies.
const SCAN_QUIET_POLLS: u32 = 50;

/// Gap left after each message of the store sequence, so the unit processes each
/// directory/commit write before the next.
const STORE_PACE: Duration = Duration::from_millis(40);

/// Fold a byte-level link error into this crate's [`Error`]. (The shared `Error`
/// lives in `rackctl-eleven-model`, so a blanket `From<MidiError>` would be an
/// orphan impl; map it here at the protocol edge instead.)
fn midi_err(e: rackctl_midi::MidiError) -> Error {
    match e {
        rackctl_midi::MidiError::PortBusy(p) => Error::PortBusy(p),
        rackctl_midi::MidiError::PortNotFound(p) => Error::PortNotFound(p),
        rackctl_midi::MidiError::Io(s) => Error::Transport(s),
    }
}

/// Format a change-report payload as `<addr> -> <value>  (word 0x..)`, or as raw
/// hex when it is too short to carry a value (some status reports do).
fn format_report(payload: &[u8]) -> String {
    let hex = |bytes: &[u8]| -> String {
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    if payload.len() >= VALUE_LEN {
        let split = payload.len() - VALUE_LEN;
        let (addr, value) = payload.split_at(split);
        if let Ok(bytes) = <[u8; VALUE_LEN]>::try_from(value) {
            let v = RawValue::from_bytes(bytes);
            return format!("{} -> {}  (word {:#x})", hex(addr), hex(value), v.decode());
        }
    }
    format!("(raw) {}", hex(payload))
}

/// A live connection to an Eleven Rack: the Digidesign protocol over a
/// [`MidiPort`] (the "Eleven Rack Rig" rawmidi port).
#[derive(Debug)]
pub struct RawMidi {
    port: MidiPort,
    device_id: u8,
}

impl RawMidi {
    /// Enumerate the ALSA rawmidi ports available on the system, as
    /// `hw:CARD,DEV` strings suitable for [`Self::open`].
    ///
    /// # Errors
    /// [`Error::Transport`] if ALSA reports an error while iterating ports.
    pub fn ports() -> Result<Vec<String>> {
        MidiPort::list_ports().map_err(midi_err)
    }

    /// Open the rawmidi port at `port` (a `hw:CARD,DEV` address) for both input
    /// and output.
    ///
    /// # Errors
    /// [`Error::PortBusy`] if another rackctl process already holds this port;
    /// [`Error::PortNotFound`] if the address is invalid;
    /// [`Error::Transport`] if ALSA cannot open the stream.
    pub fn open(port: &str) -> Result<Self> {
        Ok(Self {
            port: MidiPort::open(port).map_err(midi_err)?,
            device_id: sysex::DEFAULT_DEVICE_ID,
        })
    }

    /// Print every incoming complete `SysEx` message as hex, one per line, until
    /// interrupted. A reverse-engineering aid for mapping the addresses the unit
    /// emits when its knobs move.
    ///
    /// # Errors
    /// [`Error::Transport`] if a read fails for a reason other than no data yet.
    pub fn watch_sysex(&mut self) -> Result<()> {
        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        loop {
            match self.port.read(&mut buf).map_err(midi_err)? {
                0 => sleep(POLL_INTERVAL),
                n => {
                    for msg in framer.push(buf.get(..n).unwrap_or(&[])) {
                        let hex: Vec<String> = msg.iter().map(|b| format!("{b:02X}")).collect();
                        println!("{}", hex.join(" "));
                    }
                }
            }
        }
    }

    /// Probe the unit's identity (Universal Identity Request) and decode the
    /// reply: manufacturer, family, model and firmware version.
    ///
    /// # Errors
    /// [`Error::Timeout`] if the unit does not answer; [`Error::Transport`] on a
    /// link failure.
    pub fn identity(&mut self) -> Result<Identity> {
        self.drain_input();
        self.port
            .write_all(&sysex::build_identity_request())
            .map_err(midi_err)?;
        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        for _ in 0..REPLY_POLLS {
            match self.port.read(&mut buf).map_err(midi_err)? {
                0 => sleep(POLL_INTERVAL),
                n => {
                    for msg in framer.push(buf.get(..n).unwrap_or(&[])) {
                        if let Ok(id) = sysex::parse_identity_reply(&msg) {
                            return Ok(id);
                        }
                    }
                }
            }
        }
        Err(Error::Timeout)
    }

    /// Select a rig: Bank Select (`CC 32`, `1` = Factory, `0` = User) then a
    /// Program Change. Give the unit a moment to load before reading it.
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure.
    pub fn select_rig(&mut self, bank: u8, program: u8) -> Result<()> {
        self.port
            .write_all(&[0xB0, 0x20, bank & 0x7f])
            .map_err(midi_err)?;
        self.port
            .write_all(&[0xC0, program & 0x7f])
            .map_err(midi_err)?;
        Ok(())
    }

    /// Store the current edit buffer to a patch `slot`, naming it `name`.
    ///
    /// The captured editor save sequence (hardware-confirmed): set the edit-buffer
    /// name (block `0x05`), then `00 03 <hi> <lo>` / `00 04 <hi> <lo> <name>` /
    /// `00 02 <hi> <lo>`, where the slot is the two-byte (`hi`,`lo`) directory index.
    /// This *persists* the current sound to the slot; it writes only that slot.
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure.
    pub fn store(&mut self, slot: u16, name: &str) -> Result<()> {
        let hi = u8::try_from((slot >> 7) & 0x7f).unwrap_or(0);
        let lo = u8::try_from(slot & 0x7f).unwrap_or(0);
        // Name: printable ASCII, capped to a slot-name length, NUL-terminated.
        let nm: Vec<u8> = name
            .bytes()
            .filter(|b| (0x20..0x7f).contains(b))
            .take(16)
            .collect();
        let dev = self.device_id;
        let send = |port: &mut MidiPort, msg: &[u8]| -> Result<()> {
            port.write_all(msg).map_err(midi_err)?;
            sleep(STORE_PACE);
            Ok(())
        };
        // 00 05 <name> 00 : set edit-buffer name
        let mut m = vec![0xF0, 0x13, 0x0B, dev, 0x00, 0x05];
        m.extend_from_slice(&nm);
        m.extend_from_slice(&[0x00, 0xF7]);
        send(&mut self.port, &m)?;
        // 00 03 <hi> <lo> : begin
        send(
            &mut self.port,
            &[0xF0, 0x13, 0x0B, dev, 0x00, 0x03, hi, lo, 0xF7],
        )?;
        // 00 04 <hi> <lo> <name> 00 : directory entry for the slot
        let mut m = vec![0xF0, 0x13, 0x0B, dev, 0x00, 0x04, hi, lo];
        m.extend_from_slice(&nm);
        m.extend_from_slice(&[0x00, 0xF7]);
        send(&mut self.port, &m)?;
        // 00 02 <hi> <lo> : commit
        send(
            &mut self.port,
            &[0xF0, 0x13, 0x0B, dev, 0x00, 0x02, hi, lo, 0xF7],
        )?;
        Ok(())
    }

    /// Bulk-read a block of the current patch: send `01 <addr>` and return the
    /// block's payload (everything after the echoed address in the reply). A
    /// 1-byte `addr` reads a whole block (`[0x01]` the full packed patch, `[0x05]`
    /// the name); a multi-byte `addr` reads a sub-entry (`[0x04, hi, lo]` is the
    /// directory entry for a slot).
    ///
    /// # Errors
    /// [`Error::Timeout`] if the unit does not answer; [`Error::Transport`] on a
    /// link failure.
    pub fn read_block(&mut self, addr: &[u8]) -> Result<Vec<u8>> {
        self.drain_input();
        self.port
            .write_all(&sysex::build_read_request(self.device_id, addr))
            .map_err(midi_err)?;
        let mut framer = Framer::new();
        let mut buf = [0u8; 512];
        for _ in 0..REPLY_POLLS {
            match self.port.read(&mut buf).map_err(midi_err)? {
                0 => sleep(POLL_INTERVAL),
                n => {
                    for msg in framer.push(buf.get(..n).unwrap_or(&[])) {
                        let Ok(parsed) = sysex::parse(&msg) else {
                            continue;
                        };
                        if parsed.opcode != READ_REPLY {
                            continue;
                        }
                        if let Some(rest) = parsed.payload.strip_prefix(addr) {
                            return Ok(rest.to_vec());
                        }
                    }
                }
            }
        }
        Err(Error::Timeout)
    }

    /// Stream the unit's unsolicited change reports, one decoded line per moved
    /// parameter, until interrupted. This is the knob-sweep aid for mapping which
    /// address a front-panel control drives. Each line is `<addr> -> <value>`.
    ///
    /// # Errors
    /// [`Error::Transport`] if a read fails for a reason other than no data yet.
    pub fn monitor(&mut self) -> Result<()> {
        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        loop {
            match self.port.read(&mut buf).map_err(midi_err)? {
                0 => sleep(POLL_INTERVAL),
                n => {
                    for msg in framer.push(buf.get(..n).unwrap_or(&[])) {
                        let Ok(parsed) = sysex::parse(&msg) else {
                            continue;
                        };
                        if parsed.opcode != CHANGE_REPORT {
                            continue;
                        }
                        println!("{}", format_report(&parsed.payload));
                    }
                }
            }
        }
    }

    /// Collect every read reply the unit streams in response to a batch of
    /// requests, until the input falls silent. Each reply's payload is split into
    /// its address (all but the trailing [`VALUE_LEN`] bytes) and value. An empty
    /// result simply means nothing answered — not an error.
    fn collect_replies(&mut self) -> Vec<(Vec<u8>, RawValue)> {
        let mut framer = Framer::new();
        let mut buf = [0u8; 512];
        let mut out: Vec<(Vec<u8>, RawValue)> = Vec::new();
        let mut quiet = 0u32;
        let mut waited = 0u32;
        loop {
            match self.port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    quiet = 0;
                    for msg in framer.push(buf.get(..n).unwrap_or(&[])) {
                        let Ok(parsed) = sysex::parse(&msg) else {
                            continue;
                        };
                        if parsed.opcode != READ_REPLY || parsed.payload.len() < VALUE_LEN {
                            continue;
                        }
                        let split = parsed.payload.len() - VALUE_LEN;
                        let (addr, value) = parsed.payload.split_at(split);
                        if let Ok(bytes) = <[u8; VALUE_LEN]>::try_from(value) {
                            out.push((addr.to_vec(), RawValue::from_bytes(bytes)));
                        }
                    }
                }
                _ => {
                    quiet = quiet.saturating_add(1);
                    waited = waited.saturating_add(1);
                    if !out.is_empty() && quiet >= SCAN_QUIET_POLLS {
                        break;
                    }
                    if out.is_empty() && waited >= REPLY_POLLS {
                        break;
                    }
                    sleep(POLL_INTERVAL);
                }
            }
        }
        out
    }

    /// Discard any pending input, so a stale reply cannot be mistaken for the
    /// answer to the next request.
    fn drain_input(&mut self) {
        let mut buf = [0u8; 256];
        let mut quiet = 0u32;
        while quiet < DRAIN_QUIET_POLLS {
            match self.port.read(&mut buf) {
                Ok(n) if n > 0 => quiet = 0,
                _ => {
                    quiet = quiet.saturating_add(1);
                    sleep(POLL_INTERVAL);
                }
            }
        }
    }
}

impl Transport for RawMidi {
    fn read(&mut self, addr: &[u8]) -> Result<RawValue> {
        self.drain_input();
        let msg = sysex::build_read_request(self.device_id, addr);
        self.port.write_all(&msg).map_err(midi_err)?;

        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        for _ in 0..REPLY_POLLS {
            match self.port.read(&mut buf).map_err(midi_err)? {
                0 => sleep(POLL_INTERVAL),
                n => {
                    for reply in framer.push(buf.get(..n).unwrap_or(&[])) {
                        let Ok(parsed) = sysex::parse(&reply) else {
                            continue;
                        };
                        if parsed.opcode != READ_REPLY {
                            continue;
                        }
                        if let Some(value) = parsed.value_at(addr) {
                            return Ok(value);
                        }
                    }
                }
            }
        }
        Err(Error::Timeout)
    }

    fn write(&mut self, addr: &[u8], value: &RawValue) -> Result<()> {
        let msg = sysex::build_write(self.device_id, addr, value);
        self.port.write_all(&msg).map_err(midi_err)?;
        Ok(())
    }

    fn scan(&mut self, addrs: &[Vec<u8>]) -> Result<Vec<(Vec<u8>, RawValue)>> {
        self.drain_input();
        // Send every read request back-to-back, then collect the replies that
        // stream back; far faster than a request/await round trip per address.
        let mut batch = Vec::new();
        for addr in addrs {
            batch.extend_from_slice(&sysex::build_read_request(self.device_id, addr));
        }
        self.port.write_all(&batch).map_err(midi_err)?;
        Ok(self.collect_replies())
    }
}
