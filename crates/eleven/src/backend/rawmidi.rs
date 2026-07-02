//! The real Eleven Rack [`Transport`], layering Digidesign read/write `SysEx`
//! over the byte-level [`MidiPort`] link from `rackctl-midi`.
//!
//! This file owns only the *Digidesign-specific* protocol: building read/write
//! messages, framing replies with [`crate::sysex::Framer`], and matching a reply
//! to its request address. Opening the port, listing ports, the advisory lock and
//! the raw byte I/O all live in `rackctl-midi`.
//!
//! This path is exercised only on hardware; CI and tests use the mock.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use rackctl_midi::MidiPort;

use super::Transport;
use crate::sysex::{self, CHANGE_REPORT, Framer, Identity, READ_REPLY};
use rackctl_eleven_model::backup::{BlockData, PatchBackup, RestoreAction};
use rackctl_eleven_model::error::{Error, Result};
use rackctl_eleven_model::value::{RawValue, VALUE_LEN};

/// Pause between non-blocking read polls while waiting for a reply.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// How many *consecutive idle* [`POLL_INTERVAL`] polls (no bytes) to wait before
/// giving up on a reply — about 1.2 s. The counter resets whenever data arrives,
/// so a large multi-chunk reply never times out mid-stream; only real silence
/// ends the wait. Sized well above the unit's periodic ~0.6 s housekeeping stall
/// (seen in `--midi-log` captures), which would otherwise abort a read that is
/// simply waiting for the unit to resume.
const REPLY_POLLS: u32 = 1200;

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

/// A byte-level MIDI I/O log: one line per port write/read — a millisecond
/// timestamp (relative to when logging began), a direction (`>` out / `<` in),
/// the byte count, then the bytes as hex. Enabled by the CLI `--midi-log` flag to
/// capture exactly what crosses the wire when diagnosing device I/O.
#[derive(Debug)]
struct MidiLog {
    out: BufWriter<File>,
    start: Instant,
}

impl MidiLog {
    /// Create (truncating) the log file at `path`.
    ///
    /// Uses a real monotonic clock for the elapsed-time column — the whole point
    /// of this log is to show inter-message *timing*. RS-80's "inject a clock" is
    /// about test determinism; this is a hardware-only diagnostic path never run
    /// in tests, so a direct [`Instant`] is correct here.
    #[allow(
        clippy::disallowed_methods,
        reason = "diagnostic MIDI log needs a real clock; hardware-only, never in tests"
    )]
    fn create(path: &Path) -> Result<Self> {
        let file = File::create(path)
            .map_err(|e| Error::Transport(format!("opening MIDI log {}: {e}", path.display())))?;
        Ok(Self {
            out: BufWriter::new(file),
            start: Instant::now(),
        })
    }

    /// Append one line for `bytes` moving in direction `dir` (`>` out / `<` in).
    /// Flushed immediately so a hang or crash still leaves a complete log.
    /// Best-effort: a logging error never disturbs the device I/O it records.
    fn record(&mut self, dir: char, bytes: &[u8]) {
        let ms = self.start.elapsed().as_millis();
        let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
        let _ = writeln!(
            self.out,
            "{ms:>8} {dir} {:4} {}",
            bytes.len(),
            hex.join(" ")
        );
        let _ = self.out.flush();
    }
}

/// A live connection to an Eleven Rack: the Digidesign protocol over a
/// [`MidiPort`] (the "Eleven Rack Rig" rawmidi port).
#[derive(Debug)]
pub struct RawMidi {
    port: MidiPort,
    device_id: u8,
    /// Optional byte-level I/O log (see [`RawMidi::enable_midi_log`]).
    log: Option<MidiLog>,
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
            log: None,
        })
    }

    /// Log every MIDI byte sent and received to `path` (truncating it), for
    /// diagnosing device I/O. Wired to the CLI `--midi-log` flag; call once, right
    /// after opening, so the whole session is captured.
    ///
    /// # Errors
    /// [`Error::Transport`] if the log file cannot be created.
    pub fn enable_midi_log(&mut self, path: &Path) -> Result<()> {
        self.log = Some(MidiLog::create(path)?);
        Ok(())
    }

    /// Send a raw message to the port, logging it first if a log is enabled. All
    /// outbound MIDI goes through here so the log is complete.
    fn write_port(&mut self, bytes: &[u8]) -> Result<()> {
        if let Some(log) = self.log.as_mut() {
            log.record('>', bytes);
        }
        self.port.write_all(bytes).map_err(midi_err)
    }

    /// Read from the port into `buf`, logging any bytes received. All inbound MIDI
    /// goes through here so the log captures replies, reports and drained bytes.
    fn read_port(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.port.read(buf).map_err(midi_err)?;
        if n > 0
            && let Some(log) = self.log.as_mut()
        {
            log.record('<', buf.get(..n).unwrap_or(&[]));
        }
        Ok(n)
    }

    /// Send one message of a paced sequence, then wait [`STORE_PACE`].
    fn send_paced(&mut self, msg: &[u8]) -> Result<()> {
        self.write_port(msg)?;
        sleep(STORE_PACE);
        Ok(())
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
            match self.read_port(&mut buf)? {
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
        self.write_port(&sysex::build_identity_request())?;
        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        for _ in 0..REPLY_POLLS {
            match self.read_port(&mut buf)? {
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
        self.write_port(&[status, cc & 0x7f, value & 0x7f])?;
        Ok(())
    }

    /// Select a patch: Bank Select (`CC 32`, `1` = Factory, `0` = User) then a
    /// Program Change. Give the unit a moment to load before reading it.
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure.
    pub fn select_rig(&mut self, bank: u8, program: u8) -> Result<()> {
        self.write_port(&[0xB0, 0x20, bank & 0x7f])?;
        self.write_port(&[0xC0, program & 0x7f])?;
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
        // 00 05 <name> 00 : set edit-buffer name.
        let mut m = vec![0xF0, 0x13, 0x0B, dev, 0x00, 0x05];
        m.extend_from_slice(&nm);
        m.extend_from_slice(&[0x00, 0xF7]);
        self.send_paced(&m)?;
        // 00 03 <lo> <hi> : save the edit buffer's content to the slot. The slot is
        // addressed **little-endian** here (low 7 bits first) — hardware-verified: a
        // big-endian `<hi> <lo>` made `00 03 00 06` read as slot 0, so every store
        // silently overwrote slot 0 with truncated content. (The `00 04` directory
        // write below addresses the slot the other way round, `<hi> <lo>`.)
        self.send_paced(&[0xF0, 0x13, 0x0B, dev, 0x00, 0x03, lo, hi, 0xF7])?;
        // 00 04 <hi> <lo> <name> 00 : directory entry (name) for the slot.
        let mut m = vec![0xF0, 0x13, 0x0B, dev, 0x00, 0x04, hi, lo];
        m.extend_from_slice(&nm);
        m.extend_from_slice(&[0x00, 0xF7]);
        self.send_paced(&m)?;
        // 00 02 <hi> <lo> : commit.
        self.send_paced(&[0xF0, 0x13, 0x0B, dev, 0x00, 0x02, hi, lo, 0xF7])?;
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
        self.write_port(&sysex::build_read_request(self.device_id, addr))?;
        let mut framer = Framer::new();
        let mut buf = [0u8; 512];
        // Wait on *idle* time, not total iterations: reset the counter whenever
        // bytes arrive so a large reply streaming in over many small reads (or a
        // reply delayed by the unit's periodic stall) is never abandoned mid-flight.
        let mut idle = 0u32;
        while idle < REPLY_POLLS {
            match self.read_port(&mut buf)? {
                0 => {
                    idle += 1;
                    sleep(POLL_INTERVAL);
                }
                n => {
                    idle = 0;
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
        self.write_port(&msg)?;
        Ok(())
    }

    /// Set a live parameter to a knob value (`0..=127`) by its full address bytes
    /// (after the `0x11` prefix): the editor sets a parameter with the *block write*
    /// opcode `0x00` and address `11 <tfx-block> <sub> <index>` (the `.tfx` block
    /// ids — `0x49` is the amp); the unit *reports* the same change back with opcode
    /// `0x02`. Affects the edit buffer only; a store persists it.
    ///
    /// # Errors
    /// [`Error::Transport`] on a link failure.
    pub fn write_param(&mut self, addr: &[u8], value: u8) -> Result<()> {
        let word = RawValue::from_bytes([value, 0, 0, 0, 0x10]);
        let mut full = vec![0x11];
        full.extend_from_slice(addr);
        let msg = sysex::build_write(self.device_id, &full, &word);
        self.write_port(&msg)
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
            match self.read_port(&mut buf)? {
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
            match self.read_port(&mut buf) {
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
            match self.read_port(&mut buf) {
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
        self.write_port(&msg)?;

        let mut framer = Framer::new();
        let mut buf = [0u8; 256];
        // Idle-based wait (see `read_block`): reset on any data so a reply delayed
        // by the unit's periodic stall isn't abandoned.
        let mut idle = 0u32;
        while idle < REPLY_POLLS {
            match self.read_port(&mut buf)? {
                0 => {
                    idle += 1;
                    sleep(POLL_INTERVAL);
                }
                n => {
                    idle = 0;
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
        self.write_port(&msg)?;
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
        self.write_port(&batch)?;
        Ok(self.collect_replies())
    }
}

/// Drive the real unit through the management-level [`ElevenDevice`](crate::ElevenDevice)
/// interface, so
/// the `manage` layer and a GUI treat hardware and [`MockEleven`](crate::MockEleven)
/// alike. Each method delegates to the inherent [`RawMidi`] method of the same name.
impl crate::bank::ElevenDevice for RawMidi {
    fn select_rig(&mut self, bank: u8, slot: u8) -> Result<()> {
        RawMidi::select_rig(self, bank, slot)
    }
    fn capture_patch(&mut self) -> Result<PatchBackup> {
        RawMidi::capture_patch(self)
    }
    fn restore_patch(&mut self, slot: u16, patch: &PatchBackup) -> Result<()> {
        RawMidi::restore_patch(self, slot, patch)
    }
    fn store(&mut self, slot: u16, name: &str) -> Result<()> {
        RawMidi::store(self, slot, name)
    }
    fn read_block(&mut self, addr: &[u8]) -> Result<Vec<u8>> {
        RawMidi::read_block(self, addr)
    }
    fn write_param(&mut self, addr: &[u8], value: u8) -> Result<()> {
        RawMidi::write_param(self, addr, value)
    }
    fn send_cc(&mut self, channel: u8, cc: u8, value: u8) -> Result<()> {
        RawMidi::send_cc(self, channel, cc, value)
    }
}
