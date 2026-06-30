//! Device-touching management operations: read or write whole banks and scenes,
//! and copy a slot. These drive the device via [`Gx700`] and are generic over the
//! [`Transport`], so the CLI (and any future tool) shares one implementation.
//!
//! Each long operation takes a `progress` callback, invoked per slot with the slot
//! number and patch name, so a caller can stream progress without this module
//! knowing how it's displayed.

use std::thread::sleep;
use std::time::Duration;

use rackctl_gx700::typed::Patch as TypedPatch;
use rackctl_gx700::{Gx700, RawPatch, Transport};

use crate::format::Scene;
use crate::library;

/// Pause between patch reads/writes over a whole bank, to avoid overrunning the
/// USB-MIDI interface (the same pacing the CLI's bank listing uses).
pub const BANK_READ_PACE: Duration = Duration::from_millis(40);

/// A patch slot as its user/preset label: `1..=100` -> `U001`.., `101..=200` -> `P001`..
#[must_use]
pub fn slot_label(slot: u16) -> String {
    if slot > 100 {
        format!("P{:03}", slot - 100)
    } else {
        format!("U{slot:03}")
    }
}

/// After writing a patch to a device slot, read it back and confirm the device
/// actually stored it.
///
/// The GX-700 accepts patch-memory writes *only while it is in BULK LOAD mode*;
/// outside it the write is silently ignored, so without this check a failed store
/// looks like success. The read-back turns that into a clear, actionable error.
///
/// # Errors
/// A message (including the BULK LOAD guidance) if the slot is unchanged after the
/// write, or the read-back fails.
pub fn verify_stored<T: Transport>(
    dev: &mut Gx700<T>,
    slot: u16,
    expected: &RawPatch,
) -> Result<(), String> {
    let got = dev
        .read_patch(slot)
        .map_err(|e| format!("reading back patch {slot} to verify the store: {e}"))?;
    if got.blocks != expected.blocks {
        return Err(format!(
            "the GX-700 did not store the patch to {} -- the slot is unchanged after the write. \
             The unit accepts patch-memory writes only in BULK LOAD mode: on the GX-700, press \
             TUNER/UTILITY and select \"MIDI BULK LOAD\" (the display shows \"Waiting...\"), then \
             re-run this command. See the user manual.",
            slot_label(slot)
        ));
    }
    Ok(())
}

/// Copy a stored patch from `from` (any slot `1..=200`) to user slot `to`
/// (`1..=100`), which it overwrites. Returns the copied patch.
///
/// # Errors
/// If a slot is out of range, the destination is a read-only preset, the read or
/// write fails, or the read-back verify (BULK LOAD) fails.
pub fn copy_slot<T: Transport>(dev: &mut Gx700<T>, from: u16, to: u16) -> Result<RawPatch, String> {
    if !(1..=200).contains(&from) {
        return Err(format!("source patch {from} out of range (1..=200)"));
    }
    if !(1..=100).contains(&to) {
        return Err(format!(
            "destination patch {to} must be a user slot (1..=100); presets are read-only"
        ));
    }
    let raw = dev
        .read_patch(from)
        .map_err(|e| format!("reading patch {from}: {e}"))?;
    dev.write_patch(to, &raw)
        .map_err(|e| format!("writing patch {to}: {e}"))?;
    verify_stored(dev, to, &raw)?;
    Ok(raw)
}

/// Back up a whole bank to the patch library: the 100 user patches, or (with
/// `preset`) the 100 preset patches, each saved as `U001`/`P001`.. in the
/// enveloped typed format. Returns the number saved.
///
/// # Errors
/// If a read or a library save fails.
pub fn backup_bank<T: Transport>(
    dev: &mut Gx700<T>,
    preset: bool,
    mut progress: impl FnMut(u16, &str),
) -> Result<u32, String> {
    let (slots, tag): (_, char) = if preset {
        (101u16..=200, 'P')
    } else {
        (1u16..=100, 'U')
    };
    let mut count = 0;
    for slot in slots {
        let raw = dev
            .read_patch(slot)
            .map_err(|e| format!("reading patch {slot}: {e}"))?;
        let n = if preset { slot - 100 } else { slot };
        let name = format!("{tag}{n:03}");
        progress(slot, &raw.name);
        library::save_patch(&name, &TypedPatch::from_raw(&raw))?;
        count += 1;
        sleep(BANK_READ_PACE);
    }
    Ok(count)
}

/// Capture all 100 user patches into a [`Scene`] named `name` (not yet saved).
///
/// # Errors
/// If a read fails.
pub fn capture_scene<T: Transport>(
    dev: &mut Gx700<T>,
    name: &str,
    mut progress: impl FnMut(u16, &str),
) -> Result<Scene, String> {
    let mut scene = Scene::new(name);
    for slot in 1u16..=100 {
        let raw = dev
            .read_patch(slot)
            .map_err(|e| format!("reading patch {slot}: {e}"))?;
        progress(slot, &raw.name);
        scene.patches.insert(slot, TypedPatch::from_raw(&raw));
        sleep(BANK_READ_PACE);
    }
    Ok(scene)
}

/// Restore a [`Scene`] to the device, overwriting each captured user slot. The
/// first write is read-back verified to fail fast if the unit isn't in BULK LOAD
/// mode. Returns the number of patches written.
///
/// # Errors
/// If the first write didn't take (BULK LOAD), or any write fails.
pub fn restore_scene<T: Transport>(
    dev: &mut Gx700<T>,
    scene: &Scene,
    mut progress: impl FnMut(u16, &str),
) -> Result<usize, String> {
    let mut count = 0;
    for (&slot, typed) in &scene.patches {
        let raw = typed.to_raw();
        dev.write_patch(slot, &raw)
            .map_err(|e| format!("writing patch {slot}: {e}"))?;
        if count == 0 {
            // Fail fast: confirm the very first patch actually stored before writing
            // the rest, rather than silently "restoring" nothing.
            verify_stored(dev, slot, &raw)
                .map_err(|e| format!("scene restore aborted after the first patch: {e}"))?;
        }
        progress(slot, &raw.name);
        count += 1;
        sleep(BANK_READ_PACE);
    }
    Ok(count)
}
