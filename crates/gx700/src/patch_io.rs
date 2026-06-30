//! Reading and writing patches over the device: the [`Gx700`] methods that capture
//! and restore the patch buffer and patch memory. The patch *representation* (the
//! [`RawPatch`]/[`Patch`] types and their helpers) lives in `rackctl-gx700-model`;
//! this layers the device I/O on top.

use std::collections::BTreeMap;

use rackctl_gx700_model::param;
use rackctl_gx700_model::patch::{
    PATCH_VERSION, Patch, PatchHeader, RawPatch, decode_name, from_hex, from_scalar, patch_base,
    to_hex, to_scalar,
};

use crate::backend::Transport;
use crate::device::Gx700;
use crate::error::{Error, Result};

/// Base address of the temporary buffer (the current sound), bulk access.
const TEMP_BULK_BASE: [u8; 4] = [0x04, 0x00, 0x00, 0x00];
/// Sound Change Request address: a DT1 `00` here applies a bulk temp-buffer write.
const SCR_ADDR: [u8; 4] = [0x04, 0x7F, 0x7F, 0x7F];

impl<T: Transport> Gx700<T> {
    /// Read every cataloged parameter into a [`Patch`].
    ///
    /// # Errors
    /// Propagates transport read errors.
    pub fn capture_patch(&mut self) -> Result<Patch> {
        let mut params = BTreeMap::new();
        for &p in param::ALL {
            let value = self.get(p)?;
            params.insert(p.key().to_owned(), to_scalar(p, value));
        }
        Ok(Patch {
            version: PATCH_VERSION,
            params,
        })
    }

    /// Read the header (name, output level, chain order) of stored patch memory
    /// `slot` (`1..=100` user, `101..=200` preset).
    ///
    /// One RQ1 to the patch base returns its Level/Chain block, whose first 26
    /// bytes are the output level, the 13 chain bytes, and the 12-char name.
    ///
    /// # Errors
    /// [`Error::Patch`] if `slot` is out of range; transport errors otherwise.
    pub fn read_patch_header(&mut self, slot: u16) -> Result<PatchHeader> {
        let base = patch_base(slot)
            .ok_or_else(|| Error::Patch(format!("patch slot {slot} out of range (1..=200)")))?;
        let data = self.transport_mut().request(&base, 26)?;
        Ok(PatchHeader {
            output_level: data.first().copied().unwrap_or(0),
            chain: data.get(1..14).unwrap_or(&[]).to_vec(),
            name: decode_name(data.get(14..26).unwrap_or(&[])),
        })
    }

    /// Read a whole patch losslessly from `base` (all sub-blocks) into a
    /// [`RawPatch`].
    ///
    /// # Errors
    /// Transport errors, or [`Error::Timeout`] if no reply arrives.
    pub fn read_raw_patch(&mut self, base: [u8; 4]) -> Result<RawPatch> {
        // The device dumps the whole patch in response to a patch-base RQ1 with a
        // valid (small) size; an over-large size is rejected entirely.
        let streamed = self.transport_mut().request_blocks(&base, 0x20)?;
        let mut blocks = BTreeMap::new();
        for (addr, data) in streamed {
            if let Some(&sub) = addr.get(2) {
                blocks.insert(sub, to_hex(&data));
            }
        }
        let name = blocks
            .get(&0x00)
            .and_then(|hex| from_hex(hex).ok())
            .map(|b| decode_name(b.get(14..26).unwrap_or(&[])))
            .unwrap_or_default();
        Ok(RawPatch {
            version: PATCH_VERSION,
            name,
            blocks,
        })
    }

    /// Read the current sound (the temporary buffer) as a [`RawPatch`].
    ///
    /// # Errors
    /// As [`Self::read_raw_patch`].
    pub fn read_current_patch(&mut self) -> Result<RawPatch> {
        self.read_raw_patch(TEMP_BULK_BASE)
    }

    /// Read stored patch memory `slot` (`1..=200`) as a [`RawPatch`].
    ///
    /// # Errors
    /// [`Error::Patch`] if `slot` is out of range; otherwise as
    /// [`Self::read_raw_patch`].
    pub fn read_patch(&mut self, slot: u16) -> Result<RawPatch> {
        let base = patch_base(slot)
            .ok_or_else(|| Error::Patch(format!("patch slot {slot} out of range (1..=200)")))?;
        self.read_raw_patch(base)
    }

    /// Write a [`RawPatch`]'s sub-blocks to `base`; if `scr`, follow with a Sound
    /// Change Request (needed to apply a bulk write to the temporary buffer).
    /// Returns the number of sub-blocks written.
    ///
    /// # Errors
    /// [`Error::Patch`] on malformed block hex; transport write errors otherwise.
    pub fn write_raw_patch(&mut self, base: [u8; 4], patch: &RawPatch, scr: bool) -> Result<usize> {
        let mut written = 0;
        for (&sub, hex) in &patch.blocks {
            let data = from_hex(hex)?;
            self.transport_mut()
                .send(&[base[0], base[1], sub, 0x00], &data)?;
            written += 1;
        }
        if scr {
            self.transport_mut().send(&SCR_ADDR, &[0x00])?;
        }
        Ok(written)
    }

    /// Load a [`RawPatch`] into the current sound (temporary buffer), applied
    /// immediately. Non-destructive: stored patch memories are untouched, and
    /// re-selecting a patch restores the saved sound.
    ///
    /// # Errors
    /// As [`Self::write_raw_patch`].
    pub fn write_current_patch(&mut self, patch: &RawPatch) -> Result<usize> {
        self.write_raw_patch(TEMP_BULK_BASE, patch, true)
    }

    /// Write a [`RawPatch`] to user patch memory `slot` (`1..=100`). **This
    /// overwrites the stored patch.** Preset slots (`101..=200`) are read-only.
    ///
    /// # Errors
    /// [`Error::Patch`] if `slot` is out of range or a preset; otherwise as
    /// [`Self::write_raw_patch`].
    pub fn write_patch(&mut self, slot: u16, patch: &RawPatch) -> Result<usize> {
        let base = patch_base(slot)
            .ok_or_else(|| Error::Patch(format!("patch slot {slot} out of range (1..=200)")))?;
        if base.first() != Some(&0x00) {
            return Err(Error::Patch(format!("patch {slot} is a read-only preset")));
        }
        self.write_raw_patch(base, patch, false)
    }

    /// Probe whether the unit is in front-panel BULK LOAD mode, by writing a marker
    /// name to patch `slot` and reading it back: a *memory* write persists only in
    /// BULK LOAD mode (it is silently ignored in normal Play mode). The slot's
    /// original patch is written back afterwards either way, so the slot is left
    /// unchanged. Returns `Ok(true)` when the marker stuck — i.e. the unit is in
    /// BULK LOAD mode.
    ///
    /// `slot` must be a user slot (`1..=100`); a preset is read-only and can't be
    /// probed. Pick a slot whose brief, self-reverting rename is acceptable.
    ///
    /// # Errors
    /// [`Error::Patch`] if `slot` is out of range/read back malformed; transport
    /// read/write errors otherwise (e.g. the unit not answering at all).
    pub fn probe_bulk_load(&mut self, slot: u16) -> Result<bool> {
        let original = self.read_patch(slot)?;
        let mut probe = original.clone();
        // A marker distinct from the slot's current name, so a stored write is
        // unambiguous. Restored immediately below.
        let marker = if original.name.trim() == "RACKCTL?" {
            "RACKCTL!"
        } else {
            "RACKCTL?"
        };
        probe.set_name(marker)?;
        self.write_patch(slot, &probe)?;
        let stuck = self.read_patch(slot)?.name.trim() == marker;
        // Restore the original (a no-op in Play mode, where nothing changed anyway).
        self.write_patch(slot, &original)?;
        Ok(stuck)
    }

    /// Apply a [`Patch`], writing every parameter the patch holds that this
    /// build recognises. Keys it does not recognise are skipped; the count of
    /// applied parameters is returned.
    ///
    /// # Errors
    /// [`Error::Patch`] if a stored value does not fit its parameter; otherwise
    /// transport write errors.
    pub fn apply_patch(&mut self, patch: &Patch) -> Result<usize> {
        let mut applied = 0;
        for (key, scalar) in &patch.params {
            let Some(p) = param::Param::from_key(key) else {
                continue;
            };
            let value = from_scalar(p, scalar)?;
            self.set(p, value)?;
            applied += 1;
        }
        Ok(applied)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::backend::MockTransport;
    use rackctl_gx700_model::param::{Param, Value};
    use rackctl_gx700_model::patch::Scalar;

    fn dev() -> Gx700<MockTransport> {
        Gx700::new(MockTransport::new())
    }

    fn p(key: &str) -> Param {
        Param::from_key(key).unwrap()
    }

    #[test]
    fn raw_patch_round_trips_and_describes() {
        let mut d = dev();
        let mut blocks = BTreeMap::new();
        // Level/Chain: output 50, default chain, name "JAZZ TONE".
        blocks.insert(
            0x00u8,
            "32 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 4A 41 5A 5A 20 54 4F 4E 45 20 20 20"
                .to_owned(),
        );
        // Preamp: on, BG Lead, ...
        blocks.insert(0x04u8, "01 03 1F 57 3A 32 07 5F 00 01".to_owned());
        let patch = RawPatch {
            version: PATCH_VERSION,
            name: "JAZZ TONE".to_owned(),
            blocks,
        };

        d.write_current_patch(&patch).unwrap();
        let read = d.read_current_patch().unwrap();
        assert_eq!(read.name, "JAZZ TONE");
        assert_eq!(read.blocks.get(&0x04), patch.blocks.get(&0x04));

        let desc = read.describe();
        assert!(desc.contains("JAZZ TONE"), "{desc}");
        assert!(desc.contains("BG Lead"), "{desc}"); // preamp type byte 03
    }

    #[test]
    fn probe_bulk_load_detects_persisted_write_and_restores() {
        let mut d = dev();
        // Seed slot 1 with a valid patch so it has a Level/Chain block to rename.
        let mut blocks = BTreeMap::new();
        blocks.insert(
            0x00u8,
            "32 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 4A 41 5A 5A 20 54 4F 4E 45 20 20 20"
                .to_owned(),
        );
        let patch = RawPatch {
            version: PATCH_VERSION,
            name: "JAZZ TONE".to_owned(),
            blocks,
        };
        d.write_patch(1, &patch).unwrap();
        // The mock stores writes (as the hardware does in BULK LOAD), so the marker
        // sticks and the probe reports BULK LOAD.
        assert!(d.probe_bulk_load(1).unwrap());
        // …and the slot is left with its original name afterwards.
        assert_eq!(d.read_patch(1).unwrap().name, "JAZZ TONE");
    }

    #[test]
    fn patch_round_trips_through_a_fresh_device() {
        let mut a = dev();
        a.set(p("preamp-volume"), Value::Int(90)).unwrap();
        a.set(p("comp-enable"), Value::Bool(true)).unwrap();
        a.set(p("dist-type"), Value::Enum(2)).unwrap();

        let patch = a.capture_patch().unwrap();

        let mut b = dev();
        let applied = b.apply_patch(&patch).unwrap();
        assert_eq!(applied, param::ALL.len());
        assert_eq!(b.get(p("preamp-volume")).unwrap(), Value::Int(90));
        assert_eq!(b.get(p("comp-enable")).unwrap(), Value::Bool(true));
        assert_eq!(b.get(p("dist-type")).unwrap(), Value::Enum(2));
    }

    #[test]
    fn serde_json_round_trip() {
        let patch = dev().capture_patch().unwrap();
        let json = serde_json::to_string(&patch).unwrap();
        let back: Patch = serde_json::from_str(&json).unwrap();
        assert_eq!(patch, back);
    }

    #[test]
    fn unknown_keys_are_skipped() {
        let mut params = BTreeMap::new();
        params.insert("comp-enable".to_owned(), Scalar::Bool(true));
        params.insert("not-a-param".to_owned(), Scalar::Int(1));
        let patch = Patch {
            version: PATCH_VERSION,
            params,
        };
        let mut d = dev();
        assert_eq!(d.apply_patch(&patch).unwrap(), 1);
    }
}
