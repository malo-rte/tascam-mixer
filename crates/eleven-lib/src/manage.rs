//! Device-touching management operations, shared by the CLI and any GUI.
//!
//! These drive an [`ElevenDevice`] (Program Change, block reads/writes, the store
//! sequence, MIDI CC) and combine it with the on-disk library and the parameter
//! catalog, so a frontend never reimplements capture/restore/backup/scene logic.
//!
//! Long operations take a `progress` callback, invoked per slot with the slot and
//! patch name, so a caller can stream progress without this module knowing how it
//! is displayed. Errors are `String` messages, matching the rest of this crate.

use std::fmt;
use std::thread::sleep;
use std::time::Duration;

use rackctl_eleven::backup::AGGREGATE_BLOCK;
use rackctl_eleven::param::{self, Kind, Slot};
use rackctl_eleven::{BlockData, ElevenDevice, ParamRecord, PatchBackup, RestoreAction};

use crate::format::Scene;
use crate::slot_label;

/// Pause after a Program Change for the unit to load the patch before reading it.
/// The swap is timing-sensitive; too short and a capture/store reads the *previous*
/// edit buffer, so this is deliberately generous.
pub const SETTLE: Duration = Duration::from_millis(500);
/// The name block (`0x05`): the current sound's name, NUL-terminated from byte 0.
const NAME_BLOCK: u8 = 0x05;
/// Pause between slots when sweeping a whole bank, to avoid overrunning the unit.
pub const BANK_PACE: Duration = Duration::from_millis(60);
/// The User bank (Bank Select `0`); the Factory bank is `1`.
pub const USER_BANK: u8 = 0;
/// The Factory bank (read-only presets).
pub const FACTORY_BANK: u8 = 1;

/// Select `bank`/`slot` and wait for the unit to load it.
fn select_settle<D: ElevenDevice + ?Sized>(dev: &mut D, bank: u8, slot: u8) -> Result<(), String> {
    dev.select_rig(bank, slot)
        .map_err(|e| format!("selecting bank {bank} slot {slot}: {e}"))?;
    sleep(SETTLE);
    Ok(())
}

/// Capture the current sound, or User `slot` if given, as a [`PatchBackup`].
///
/// # Errors
/// If selecting the slot or reading the unit fails.
pub fn capture<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    slot: Option<u8>,
) -> Result<PatchBackup, String> {
    if let Some(s) = slot {
        select_settle(dev, USER_BANK, s)?;
    }
    dev.capture_patch().map_err(|e| e.to_string())
}

/// The outcome of a restore's read-back verify.
#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    /// Blocks written back (the restorable ones).
    pub written: usize,
    /// Of those, how many read back matching.
    pub matched: usize,
    /// Block ids that did not match on read-back.
    pub mismatched: Vec<u8>,
}

impl VerifyReport {
    /// Whether every written block verified.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.mismatched.is_empty()
    }
}

impl fmt::Display for VerifyReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ok() {
            write!(f, "all {} blocks match", self.matched)
        } else {
            let ids: Vec<String> = self
                .mismatched
                .iter()
                .map(|id| format!("{id:#04X}"))
                .collect();
            write!(
                f,
                "{} matched, {} differ ({})",
                self.matched,
                self.mismatched.len(),
                ids.join(", ")
            )
        }
    }
}

/// Compare a restored `patch` against a fresh capture `after`, keyed by the stable
/// `target` for parameter-table blocks (their physical index is reassigned on
/// reload) and byte-exact for flat blocks.
fn verify_blocks(patch: &PatchBackup, after: &PatchBackup) -> VerifyReport {
    let mut report = VerifyReport::default();
    for b in patch
        .blocks
        .iter()
        .filter(|b| b.restore_action() != RestoreAction::Skip)
    {
        report.written += 1;
        let after_b = after.blocks.iter().find(|x| x.id == b.id);
        let matched = if let Some(want) = b.param_values_by_target() {
            after_b.and_then(BlockData::param_values_by_target) == Some(want)
        } else {
            after_b.map(|x| x.bytes.as_slice()) == Some(b.bytes.as_slice())
        };
        if matched {
            report.matched += 1;
        } else {
            report.mismatched.push(b.id);
        }
    }
    report
}

/// Restore `patch` into User `slot`, then re-read and verify. See
/// [`ElevenDevice::restore_patch`] for the mechanism (a full aggregate-block write when
/// the backup carries `0x01`, else a per-block replay).
///
/// # Errors
/// If any device operation fails.
pub fn restore<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    slot: u8,
    patch: &PatchBackup,
) -> Result<VerifyReport, String> {
    select_settle(dev, USER_BANK, slot)?;
    dev.restore_patch(u16::from(slot), patch)
        .map_err(|e| format!("restoring to slot {slot}: {e}"))?;
    select_settle(dev, USER_BANK, slot)?;
    // A backup with the full aggregate loads in full, so verify against it (the
    // whole packed image); a backup without one used the per-block replay, so fall
    // back to the target-keyed per-block compare.
    if let Some(want) = patch
        .blocks
        .iter()
        .find(|b| b.id == AGGREGATE_BLOCK)
        .filter(|b| !b.bytes.is_empty())
    {
        let after = read_aggregate(dev)?;
        Ok(verify_aggregate(&want.bytes, &after))
    } else {
        let after = dev.capture_patch().map_err(|e| e.to_string())?;
        Ok(verify_blocks(patch, &after))
    }
}

/// Capture the current sound (or User `slot`) and save it to the backup library as
/// `name`. Returns the captured patch.
///
/// # Errors
/// If the capture or the library save fails.
pub fn capture_to_library<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    name: &str,
    slot: Option<u8>,
) -> Result<PatchBackup, String> {
    let patch = capture(dev, slot)?;
    crate::save_backup(name, &patch)?;
    Ok(patch)
}

/// Load a named backup from the library and restore it into User `slot`, verifying.
/// Returns the loaded patch and the verify report.
///
/// # Errors
/// If the backup is missing/unreadable, or a device operation fails.
pub fn restore_from_library<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    name: &str,
    slot: u8,
) -> Result<(PatchBackup, VerifyReport), String> {
    let patch = crate::load_backup(name)?;
    let report = restore(dev, slot, &patch)?;
    Ok((patch, report))
}

/// Read one User slot's name from the directory (block `0x04 <hi> <lo>`), or `None`
/// if the slot does not answer. Used by [`patch_directory`] and by a GUI's
/// background bank reader to stream the list slot-by-slot.
///
/// # Errors
/// Never returns `Err`; a non-answering slot is `None`.
#[must_use]
pub fn slot_name<D: ElevenDevice + ?Sized>(dev: &mut D, slot: u8) -> Option<String> {
    let hi = (slot >> 7) & 0x7f;
    let lo = slot & 0x7f;
    dev.read_block(&[0x04, hi, lo]).ok().map(|b| block_name(&b))
}

/// Read a bank's on-device patch directory (block `0x04`): `(slot, name)` for each
/// slot that answers, up to `count`. `bank` is [`USER_BANK`] / [`FACTORY_BANK`].
///
/// # Errors
/// Never returns `Err` for a non-answering slot (it is skipped); reserved for a
/// hard link failure.
pub fn patch_directory<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    _bank: u8,
    count: u8,
) -> Result<Vec<(u8, String)>, String> {
    let mut out = Vec::new();
    for slot in 0..count {
        if let Some(name) = slot_name(dev, slot) {
            out.push((slot, name));
        }
    }
    Ok(out)
}

/// Capture the whole User bank to the backup library, one file per slot named by
/// its [`slot_label`]. Stops when the bank wraps (a repeated first name) or after
/// `count` slots. Returns how many were saved.
///
/// # Errors
/// If a device read or a library save fails.
pub fn backup_bank<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    count: u8,
    mut progress: impl FnMut(u8, &str),
) -> Result<u32, String> {
    let mut saved = 0;
    let mut first: Option<String> = None;
    for slot in 0..count {
        select_settle(dev, USER_BANK, slot)?;
        let patch = dev.capture_patch().map_err(|e| e.to_string())?;
        if slot > 0 && first.as_deref() == Some(patch.name.as_str()) {
            break; // bank wrapped
        }
        first.get_or_insert_with(|| patch.name.clone());
        progress(slot, &patch.name);
        crate::save_backup(&slot_label(slot), &patch)?;
        saved += 1;
        sleep(BANK_PACE);
    }
    Ok(saved)
}

/// Capture the whole User bank into a [`Scene`] named `name` (not yet saved).
/// Stops when the bank wraps or after `count` slots.
///
/// # Errors
/// If a device read fails.
pub fn capture_scene<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    name: &str,
    count: u8,
    mut progress: impl FnMut(u8, &str),
) -> Result<Scene, String> {
    let mut scene = Scene::new(name);
    let mut first: Option<String> = None;
    for slot in 0..count {
        select_settle(dev, USER_BANK, slot)?;
        let patch = dev.capture_patch().map_err(|e| e.to_string())?;
        if slot > 0 && first.as_deref() == Some(patch.name.as_str()) {
            break;
        }
        first.get_or_insert_with(|| patch.name.clone());
        progress(slot, &patch.name);
        scene.patches.insert(slot, patch);
        sleep(BANK_PACE);
    }
    Ok(scene)
}

/// Restore a whole [`Scene`] to the device, overwriting each captured User slot.
/// Returns an aggregate verify report over all slots.
///
/// # Errors
/// If a device operation fails.
pub fn restore_scene<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    scene: &Scene,
    mut progress: impl FnMut(u8, &str),
) -> Result<VerifyReport, String> {
    let mut agg = VerifyReport::default();
    for (&slot, patch) in &scene.patches {
        progress(slot, &patch.name);
        let r = restore(dev, slot, patch)?;
        agg.written += r.written;
        agg.matched += r.matched;
        agg.mismatched.extend(r.mismatched);
    }
    Ok(agg)
}

/// Copy the patch at `from_bank`/`from_slot` (e.g. a Factory preset) into User
/// `to_slot`, verifying. This is how a factory preset is placed in a user slot.
///
/// # Errors
/// If any device operation fails.
pub fn copy_slot<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    from_bank: u8,
    from_slot: u8,
    to_slot: u8,
) -> Result<VerifyReport, String> {
    // Load the whole source sound into the edit buffer, then persist it to the
    // target with the device's native store — a *full* copy of every block, not the
    // restorable subset. (Store commits the current edit buffer; see
    // [`ElevenDevice::store`].) Verified by re-reading the target's packed image.
    select_settle(dev, from_bank, from_slot)?;
    let name = read_name(dev)?;
    let source = read_aggregate(dev)?;
    dev.store(u16::from(to_slot), &name)
        .map_err(|e| format!("storing to slot {to_slot}: {e}"))?;
    select_settle(dev, USER_BANK, to_slot)?;
    let after = read_aggregate(dev)?;
    Ok(verify_aggregate(&source, &after))
}

/// Rename User `slot` in place: select it (so the edit buffer holds its sound), then
/// store it under `name`. The patch content is unchanged; only its name is rewritten.
///
/// # Errors
/// If selecting or storing fails.
pub fn rename<D: ElevenDevice + ?Sized>(dev: &mut D, slot: u8, name: &str) -> Result<(), String> {
    select_settle(dev, USER_BANK, slot)?;
    dev.store(u16::from(slot), name)
        .map_err(|e| format!("renaming slot {slot}: {e}"))
}

/// How many times to retry a required single-block read. The unit occasionally
/// drops one reply; a whole-bank capture tolerates that (it skips the block), but
/// a single *required* read (name, aggregate) would otherwise abort the operation.
const READ_TRIES: u32 = 4;

/// Read one block, retrying on a dropped reply. `label` names it for the error.
fn read_block_retry<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    addr: &[u8],
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut last = String::new();
    for _ in 0..READ_TRIES {
        match dev.read_block(addr) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                last = e.to_string();
                sleep(BANK_PACE);
            }
        }
    }
    Err(format!("reading {label}: {last}"))
}

/// Read the current edit buffer's name (public; `None` if the read fails).
#[must_use]
pub fn current_name<D: ElevenDevice + ?Sized>(dev: &mut D) -> Option<String> {
    read_name(dev).ok()
}

/// Select a Factory slot and read its name — for a read-only preset browser. Note
/// this loads the slot into the edit buffer (so it also auditions it).
#[must_use]
pub fn factory_name<D: ElevenDevice + ?Sized>(dev: &mut D, slot: u8) -> Option<String> {
    select_settle(dev, FACTORY_BANK, slot).ok()?;
    read_name(dev).ok()
}

/// Read the current edit buffer's name (block `0x05`).
fn read_name<D: ElevenDevice + ?Sized>(dev: &mut D) -> Result<String, String> {
    read_block_retry(dev, &[NAME_BLOCK], "name block").map(|b| block_name(&b))
}

/// Read the current edit buffer's full packed patch image (aggregate block `0x01`).
fn read_aggregate<D: ElevenDevice + ?Sized>(dev: &mut D) -> Result<Vec<u8>, String> {
    read_block_retry(dev, &[AGGREGATE_BLOCK], "aggregate block")
}

/// The current sound's live **amp parameter table**: its block id and records
/// (value / index / target). Addressing an amp parameter needs this live read
/// because the physical `index` is reassigned on every reload; the stable handle
/// is [`ParamRecord::target`]. The table has been block `0x21` on every model seen,
/// so try that first, else fall back to the densest parameter-table block.
///
/// # Errors
/// If no parameter-table block can be read from the unit.
pub fn amp_param_table<D: ElevenDevice + ?Sized>(
    dev: &mut D,
) -> Result<(u8, Vec<ParamRecord>), String> {
    if let Ok(bytes) = read_block_retry(dev, &[0x21], "amp table")
        && let Some(recs) = (BlockData { id: 0x21, bytes }).param_records()
    {
        return Ok((0x21, recs));
    }
    let mut best: Option<(u8, Vec<ParamRecord>)> = None;
    for id in 0x1E..=0x3F_u8 {
        if let Ok(bytes) = dev.read_block(&[id])
            && let Some(recs) = (BlockData { id, bytes }).param_records()
            && best
                .as_ref()
                .is_none_or(|(_, prev)| recs.len() > prev.len())
        {
            best = Some((id, recs));
        }
    }
    best.ok_or_else(|| "no amp parameter table found on the unit".to_owned())
}

/// Resolve the amp parameter with stable `target` to its live record + block.
///
/// # Errors
/// If the table cannot be read, or no parameter has that target.
pub fn get_amp_param<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    target: u8,
) -> Result<(u8, ParamRecord), String> {
    let (block, recs) = amp_param_table(dev)?;
    recs.into_iter()
        .find(|r| r.target == target)
        .map(|r| (block, r))
        .ok_or_else(|| format!("no amp parameter with target {target:#04X} (see `scan amp`)"))
}

/// Write `value` to the amp parameter with stable `target`: read the live table to
/// resolve its current index, write there, then re-read and return the record.
/// This targets the edit buffer; a store is needed to persist it.
///
/// # Errors
/// If the table cannot be read, no parameter has that target, or the write fails.
pub fn set_amp_param<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    target: u8,
    value: u8,
) -> Result<(u8, ParamRecord), String> {
    let (_block, recs) = amp_param_table(dev)?;
    let index = recs
        .iter()
        .find(|r| r.target == target)
        .map(|r| r.index)
        .ok_or_else(|| format!("no amp parameter with target {target:#04X} (see `scan amp`)"))?;
    // The editor addresses a parameter as `11 <tfx-block> <sub> <index>`; the amp is
    // .tfx block 0x49, sub 0x01 (from the USB capture's `11 49 01` reads).
    dev.write_param(&[0x49, 0x01, index], value)
        .map_err(|e| format!("writing amp param: {e}"))?;
    sleep(BANK_PACE);
    get_amp_param(dev, target)
}

/// Verify a native full-buffer copy: the target's packed image (`0x01`) must match
/// the source's, non-empty. Reported as the single aggregate "block".
fn verify_aggregate(source: &[u8], after: &[u8]) -> VerifyReport {
    let mut report = VerifyReport {
        written: 1,
        ..Default::default()
    };
    if !source.is_empty() && source == after {
        report.matched = 1;
    } else {
        report.mismatched.push(AGGREGATE_BLOCK);
    }
    report
}

/// Resolve a control `name` to its MIDI CC (via the parameter catalog, using the
/// optional amp / effect context) and send it: `value` on `channel`. Returns the
/// resolved `(cc, kind)`. This is the native remote-control path.
///
/// With `fx` set, `slot` selects the chain slot (defaulting to the effect's first
/// slot if `None`), since an effect's CC differs per slot.
///
/// # Errors
/// If the amp / effect / control name cannot be resolved, or the send fails.
pub fn send_named_cc<D: ElevenDevice + ?Sized>(
    dev: &mut D,
    name: &str,
    value: u8,
    amp: Option<&str>,
    fx: Option<&str>,
    slot: Option<Slot>,
    channel: u8,
) -> Result<(u8, Kind), String> {
    let fx_ctx = match fx {
        Some(f) => {
            let e = param::effect(f).ok_or_else(|| format!("no effect named {f:?}; see `list`"))?;
            let s = slot
                .or_else(|| e.slots.first().copied())
                .ok_or_else(|| format!("effect {f:?} has no slots"))?;
            Some((f, s))
        }
        None => None,
    };
    let (cc, kind) = param::resolve_cc(name, amp, fx_ctx).ok_or_else(|| {
        format!("could not resolve {name:?}; give --amp/--fx context, or see `list`")
    })?;
    dev.send_cc(channel, cc, value)
        .map_err(|e| format!("sending CC {cc}: {e}"))?;
    Ok((cc, kind))
}

/// Extract a printable-ASCII patch name from a directory (`0x04`) block payload.
///
/// The read reply is the NUL-terminated name from byte 0 — the device strips the
/// `<hi> <lo>` slot address that the *store* path writes, so nothing is skipped
/// here (hardware-verified: `04 00 00` reads back `42 69 67 20 42 6C 75 65 00` =
/// "Big Blue"). This matches the `0x05` current-name decode.
fn block_name(payload: &[u8]) -> String {
    payload
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|&&b| (0x20..0x7f).contains(&b))
        .map(|&b| b as char)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::block_name;

    #[test]
    fn block_name_decodes_from_byte_zero_no_header() {
        // Hardware-observed `04 00 00` read reply for User slot 0: the clean,
        // NUL-terminated name from byte 0 (no `<hi> <lo>` header). Regression
        // guard against the old 2-byte skip that chopped "Big Blue" to "g Blue".
        let payload = [0x42, 0x69, 0x67, 0x20, 0x42, 0x6C, 0x75, 0x65, 0x00];
        assert_eq!(block_name(&payload), "Big Blue");
        // Trailing padding/garbage after the NUL is ignored.
        let padded = [0x41, 0x78, 0x65, 0x00, 0xFF, 0x01];
        assert_eq!(block_name(&padded), "Axe");
    }
}
