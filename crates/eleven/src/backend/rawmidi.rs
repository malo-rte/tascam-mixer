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
use rackctl_eleven_model::backup::{BlockData, PatchBackup, RestoreAction};
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

/// Gap between the block reads of a patch capture, so the paced reads never look
/// like a flood (which can wedge the unit's editor parser).
const CAPTURE_PACE: Duration = Duration::from_millis(15);

/// The block ids probed when capturing a patch. The aggregate `0x01` is captured
/// for reference (it is read-only and never written back); `0x05` name, `0x07`
/// config and `0x08` FX-chain are patch metadata; `0x1E..=0x3F` covers the
/// model-dependent parameter blocks (the densest, `0x21`, holds the amp). The
/// directory/commit blocks `0x02..0x04` are deliberately not read — they are the
/// store sequence's own, not patch content.
fn patch_block_probe() -> Vec<u8> {
    let mut ids = vec![0x01u8, 0x05, 0x07, 0x08];
    ids.extend(0x1E..=0x3F);
    ids
}

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

/// Extract a printable-ASCII patch name from a `0x05` name-block payload (trailing
/// NUL / padding trimmed).
fn ascii_name(payload: &[u8]) -> String {
    payload
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|&&b| (0x20..0x7f).contains(&b))
        .map(|&b| b as char)
        .collect::<String>()
        .trim()
        .to_owned()
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

    /// Send a MIDI Control Change: `Bn <cc> <value>` on `channel` (1..=16). This is
    /// the native remote-control path — the unit moves the mapped parameter, the
    /// same as a foot controller. Values are masked to 7 bits.
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure.
    pub fn send_cc(&mut self, channel: u8, cc: u8, value: u8) -> Result<()> {
        let status = 0xB0 | (channel.saturating_sub(1) & 0x0f);
        self.port
            .write_all(&[status, cc & 0x7f, value & 0x7f])
            .map_err(midi_err)?;
        Ok(())
    }

    /// Select a patch: Bank Select (`CC 32`, `1` = Factory, `0` = User) then a
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
    /// The save sequence (hardware-confirmed): set the edit-buffer name (block
    /// `0x05`), then `00 03 <lo> <hi>` (save content) / `00 04 <hi> <lo> <name>`
    /// (directory name) / `00 02 <hi> <lo>` (commit). This *persists* the current
    /// sound to the slot and writes only that slot.
    ///
    /// The two slot-address bytes are ordered **differently per message**: the
    /// content save `00 03` takes the slot low-byte first, while the directory and
    /// commit take it high-byte first. Getting `00 03` wrong (big-endian) made every
    /// store also clobber slot 0 with truncated content — see the byte-order note in
    /// the body; verified against hardware across slots 0/4/6.
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
        // 00 05 <name> 00 : set edit-buffer name.
        let mut m = vec![0xF0, 0x13, 0x0B, dev, 0x00, 0x05];
        m.extend_from_slice(&nm);
        m.extend_from_slice(&[0x00, 0xF7]);
        send(&mut self.port, &m)?;
        // 00 03 <lo> <hi> : save the edit buffer's content to the slot. The slot is
        // addressed **little-endian** here (low 7 bits first) — hardware-verified: a
        // big-endian `<hi> <lo>` made `00 03 00 06` read as slot 0, so every store
        // silently overwrote slot 0 with truncated content. (The `00 04` directory
        // write below addresses the slot the other way round, `<hi> <lo>`.)
        send(
            &mut self.port,
            &[0xF0, 0x13, 0x0B, dev, 0x00, 0x03, lo, hi, 0xF7],
        )?;
        // 00 04 <hi> <lo> <name> 00 : directory entry (name) for the slot.
        let mut m = vec![0xF0, 0x13, 0x0B, dev, 0x00, 0x04, hi, lo];
        m.extend_from_slice(&nm);
        m.extend_from_slice(&[0x00, 0xF7]);
        send(&mut self.port, &m)?;
        // 00 02 <hi> <lo> : commit.
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

    /// Write a whole block into the edit buffer: `00 <block> <payload>`. The
    /// payload is the bytes a `01 <block>` read returned. This affects the *edit
    /// buffer* only; [`Self::store`] persists it to a slot.
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure.
    pub fn write_block(&mut self, block: u8, payload: &[u8]) -> Result<()> {
        let mut msg = vec![0xF0, 0x13, 0x0B, self.device_id, 0x00, block];
        msg.extend_from_slice(payload);
        msg.push(0xF7);
        self.port.write_all(&msg).map_err(midi_err)?;
        Ok(())
    }

    /// Capture the *currently selected* patch as a device-faithful [`PatchBackup`]:
    /// read each candidate block (`0x01`, `0x05`, `0x07`, `0x08`, `0x1E..=0x3F`) and
    /// keep the ones that answer with a payload. Reads are paced so the sweep never
    /// floods the unit. Select the patch first (`select_rig` + a settle delay).
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure. A block that does not answer is
    /// simply omitted (not every id is populated), so this does not fail on a
    /// missing block.
    pub fn capture_patch(&mut self) -> Result<PatchBackup> {
        let mut blocks = Vec::new();
        for id in patch_block_probe() {
            if let Ok(bytes) = self.read_block(&[id])
                && !bytes.is_empty()
            {
                blocks.push(BlockData { id, bytes });
            }
            sleep(CAPTURE_PACE);
        }
        let name = blocks
            .iter()
            .find(|b| b.id == 0x05)
            .map_or_else(String::new, |b| ascii_name(&b.bytes));
        Ok(PatchBackup::new(name, blocks))
    }

    /// Restore a [`PatchBackup`] into User `slot`, then persist it with the store
    /// sequence.
    ///
    /// The full patch lives in the aggregate block `0x01`, which is *writable*: the
    /// normal path writes that one block into the edit buffer and stores it — a
    /// complete, byte-exact load. Only a backup lacking `0x01` falls back to the
    /// per-block replay (the restorable blocks; system/device blocks skipped, see
    /// [`BlockData::is_restorable`]).
    ///
    /// The caller should verify afterwards by re-reading the target.
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure during a block write or the store.
    pub fn restore_patch(&mut self, slot: u16, patch: &PatchBackup) -> Result<()> {
        // Preferred path: write the whole packed patch (aggregate block `0x01`) into
        // the edit buffer, then store. `0x01` is writable (hardware-verified: the
        // slot's aggregate reads back byte-identical to the source), so this loads
        // *any* captured patch in full — every block — with no per-block replay.
        if let Some(agg) = patch.blocks.iter().find(|b| b.id == 0x01)
            && !agg.bytes.is_empty()
        {
            self.write_block(0x01, &agg.bytes)?;
            sleep(STORE_PACE);
            self.store(slot, &patch.name)?;
            return Ok(());
        }
        // Fallback (a backup that carries no aggregate block): per-block restore.
        for b in &patch.blocks {
            match b.restore_action() {
                // System/device blocks are NEVER written — writing them blindly
                // (e.g. the `0x22` device-info register) can put the unit into a bad
                // global state. See `SAFE_FLAT_BLOCKS`.
                RestoreAction::Skip => {}
                // Safe flat patch-content blocks (name, FX-chain): write whole.
                RestoreAction::WholeBlock => {
                    self.write_block(b.id, &b.bytes)?;
                    sleep(STORE_PACE);
                }
                // Parameter-table blocks (e.g. the amp `0x21`): a whole-block write
                // does not take, and the physical index is reassigned on reload.
                // Re-read the *live* table to map each stable `target` to its current
                // index, then write the value there (`00 11 <id> <index> <value>`).
                RestoreAction::PerParam => {
                    let Some(want) = b.param_records() else {
                        continue;
                    };
                    let live = self
                        .read_block(&[b.id])
                        .ok()
                        .and_then(|bytes| BlockData { id: b.id, bytes }.param_records())
                        .unwrap_or_default();
                    for r in want {
                        let Some(live_index) =
                            live.iter().find(|l| l.target == r.target).map(|l| l.index)
                        else {
                            continue;
                        };
                        let val = RawValue::from_bytes([r.value, 0, 0, 0, 0x10]);
                        self.write(&[0x11, b.id, live_index], &val)?;
                        sleep(STORE_PACE);
                    }
                }
            }
        }
        self.store(slot, &patch.name)?;
        Ok(())
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
