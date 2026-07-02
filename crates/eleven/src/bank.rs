//! The **patch-bank device interface** — a small trait over the management-level
//! operations (Program Change, capture / restore, store, block reads, MIDI CC), so
//! the `rackctl-eleven-lib` `manage` layer and a GUI drive either real hardware
//! ([`RawMidi`](crate::RawMidi)) or the in-memory [`MockEleven`] with no change.
//!
//! This mirrors the GX-700 stack, where the management layer is generic over the
//! device so a `--mock` frontend runs the whole app with no unit attached.

use std::collections::BTreeMap;

use crate::{BlockData, PatchBackup, Result};

/// Block id of the whole packed patch (the aggregate).
const AGGREGATE_BLOCK: u8 = 0x01;
/// Block id of the current sound's name.
const NAME_BLOCK: u8 = 0x05;
/// Block id of the on-device directory (`04 <hi> <lo>` → a slot's name).
const DIRECTORY_BLOCK: u8 = 0x04;
/// Address of a slot's whole packed patch, read directly with no Program Change:
/// `01 00 <slot>` → the same packed image as reading [`AGGREGATE_BLOCK`] of the
/// buffer after selecting that slot. This is how the official editor backs up the
/// whole bank in seconds (see the `eleven-save-*` USB captures).
const SLOT_PATCH_BLOCK: u8 = 0x00;

/// The management-level device operations, over real hardware or a mock. All
/// methods are object-safe so a frontend can hold a `Box<dyn ElevenDevice + Send>`.
pub trait ElevenDevice {
    /// Select a patch: Bank Select (`bank`) then Program Change (`slot`).
    ///
    /// # Errors
    /// If the transport write fails.
    fn select_rig(&mut self, bank: u8, slot: u8) -> Result<()>;

    /// Capture the current edit buffer as a [`PatchBackup`].
    ///
    /// # Errors
    /// If a device read fails.
    fn capture_patch(&mut self) -> Result<PatchBackup>;

    /// Restore a [`PatchBackup`] into User `slot` (edit-buffer write + store).
    ///
    /// # Errors
    /// If a device operation fails.
    fn restore_patch(&mut self, slot: u16, patch: &PatchBackup) -> Result<()>;

    /// Persist the current edit buffer to User `slot`, naming it `name`.
    ///
    /// # Errors
    /// If the store sequence fails.
    fn store(&mut self, slot: u16, name: &str) -> Result<()>;

    /// Read a block: `01 <addr>` → the payload after the echoed address.
    ///
    /// # Errors
    /// If no reply arrives.
    fn read_block(&mut self, addr: &[u8]) -> Result<Vec<u8>>;

    /// Set a live parameter (edit buffer only). `addr` is the bytes after the
    /// `0x11` parameter prefix.
    ///
    /// # Errors
    /// If the transport write fails.
    fn write_param(&mut self, addr: &[u8], value: u8) -> Result<()>;

    /// Send a MIDI Control Change (`Bn <cc> <value>`).
    ///
    /// # Errors
    /// If the transport write fails.
    fn send_cc(&mut self, channel: u8, cc: u8, value: u8) -> Result<()>;
}

/// An in-memory Eleven Rack: a User + Factory bank and an edit buffer, seeded with
/// a plausible bank so a GUI (or a test) exercises the whole management layer with
/// no hardware. `select_rig` loads a slot into the buffer; `capture` returns the
/// buffer; `store`/`restore_patch` persist it back; `read_block` serves the
/// directory / name / aggregate blocks; parameter writes and CC are accepted no-ops.
#[derive(Debug, Clone)]
pub struct MockEleven {
    user: BTreeMap<u8, PatchBackup>,
    factory: BTreeMap<u8, PatchBackup>,
    buffer: PatchBackup,
}

impl Default for MockEleven {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEleven {
    /// A mock seeded with the eight factory-preset names in slots 0–7 (matching a
    /// real unit); the remaining User slots are empty.
    #[must_use]
    pub fn new() -> Self {
        const NAMES: [&str; 8] = [
            "Big Blue",
            "Chorus Clean",
            "Bolder Axe",
            "Browntown",
            "Basie Phase",
            "Jangly Stereo",
            "Sandstage",
            "Austin Rotary",
        ];
        let mut user = BTreeMap::new();
        let mut factory = BTreeMap::new();
        for (i, name) in NAMES.iter().enumerate() {
            let slot = u8::try_from(i).unwrap_or(0);
            user.insert(slot, mock_patch(name, slot));
            factory.insert(slot, mock_patch(name, slot));
        }
        let buffer = user
            .get(&0)
            .cloned()
            .unwrap_or_else(|| mock_patch("Empty", 0));
        Self {
            user,
            factory,
            buffer,
        }
    }

    fn bank(&self, bank: u8) -> &BTreeMap<u8, PatchBackup> {
        if bank == 1 { &self.factory } else { &self.user }
    }
}

/// Build a deterministic mock patch: a name block and an aggregate block whose
/// bytes embed the name and a per-slot fill (so distinct patches copy/verify).
fn mock_patch(name: &str, seed: u8) -> PatchBackup {
    let mut agg: Vec<u8> = name.bytes().collect();
    agg.push(0);
    while agg.len() < 48 {
        let n = u8::try_from(agg.len() & 0x7f).unwrap_or(0);
        agg.push(seed.wrapping_add(n));
    }
    let mut nm: Vec<u8> = name.bytes().collect();
    nm.push(0);
    PatchBackup::new(
        name,
        vec![
            BlockData {
                id: NAME_BLOCK,
                bytes: nm,
            },
            BlockData {
                id: AGGREGATE_BLOCK,
                bytes: agg,
            },
        ],
    )
}

impl ElevenDevice for MockEleven {
    fn select_rig(&mut self, bank: u8, slot: u8) -> Result<()> {
        if let Some(p) = self.bank(bank).get(&slot) {
            self.buffer = p.clone();
        }
        Ok(())
    }

    fn capture_patch(&mut self) -> Result<PatchBackup> {
        Ok(self.buffer.clone())
    }

    fn restore_patch(&mut self, slot: u16, patch: &PatchBackup) -> Result<()> {
        self.buffer = patch.clone();
        if let Ok(s) = u8::try_from(slot) {
            self.user.insert(s, patch.clone());
        }
        Ok(())
    }

    fn store(&mut self, slot: u16, name: &str) -> Result<()> {
        let mut p = self.buffer.clone();
        name.clone_into(&mut p.name);
        let mut nm: Vec<u8> = name.bytes().collect();
        nm.push(0);
        if let Some(b) = p.blocks.iter_mut().find(|b| b.id == NAME_BLOCK) {
            b.bytes = nm;
        }
        self.buffer = p.clone();
        if let Ok(s) = u8::try_from(slot) {
            self.user.insert(s, p);
        }
        Ok(())
    }

    fn read_block(&mut self, addr: &[u8]) -> Result<Vec<u8>> {
        match addr {
            [DIRECTORY_BLOCK, hi, lo] => {
                let idx = (u16::from(*hi) << 7) | u16::from(*lo);
                let name = u8::try_from(idx)
                    .ok()
                    .and_then(|s| self.user.get(&s))
                    .map_or("", |p| p.name.as_str());
                let mut v: Vec<u8> = name.bytes().collect();
                v.push(0);
                Ok(v)
            }
            // Direct slot read (`00 <slot>`): the slot's whole packed patch, no PC.
            [SLOT_PATCH_BLOCK, slot] => Ok(self
                .user
                .get(slot)
                .and_then(|p| p.block(AGGREGATE_BLOCK))
                .map(<[u8]>::to_vec)
                .unwrap_or_default()),
            [id] => Ok(self
                .buffer
                .block(*id)
                .map(<[u8]>::to_vec)
                .unwrap_or_default()),
            _ => Ok(Vec::new()),
        }
    }

    fn write_param(&mut self, _addr: &[u8], _value: u8) -> Result<()> {
        Ok(())
    }

    fn send_cc(&mut self, _channel: u8, _cc: u8, _value: u8) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn mock_bank_selects_captures_and_stores() {
        let mut m = MockEleven::new();
        // Directory read returns the seeded names.
        assert_eq!(m.read_block(&[0x04, 0, 0]).unwrap(), b"Big Blue\0");
        assert_eq!(m.read_block(&[0x04, 0, 2]).unwrap(), b"Bolder Axe\0");
        // Select loads the slot into the buffer; capture returns it.
        m.select_rig(0, 2).unwrap();
        assert_eq!(m.capture_patch().unwrap().name, "Bolder Axe");
        // Store persists a renamed buffer to a slot; the directory reflects it.
        m.store(10, "My Patch").unwrap();
        assert_eq!(m.read_block(&[0x04, 0, 10]).unwrap(), b"My Patch\0");
        // Direct slot read (`00 <slot>`) returns the slot's aggregate with no
        // select — the fast, Program-Change-free capture path.
        let want = mock_patch("Bolder Axe", 2).block(0x01).unwrap().to_vec();
        assert_eq!(m.read_block(&[0x00, 2]).unwrap(), want);
    }
}
