//! Capturing and restoring the GX-700 patch buffer as a JSON [`Patch`].
//!
//! A [`Patch`] is a serde-(de)serializable snapshot of parameter values, keyed
//! by [`crate::Param::key`]. File and format handling lives in the caller (e.g. the
//! CLI); this module only turns device state into a [`Patch`] and back.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::backend::Transport;
use crate::device::Gx700;
use crate::error::{Error, Result};
use crate::param::{self, Kind, Value};

/// Schema version written into every patch, for forward compatibility.
pub const PATCH_VERSION: u32 = 1;

/// Number of characters in a GX-700 patch name.
pub const NAME_LEN: usize = 12;

/// Decode a GX-700 patch name: up to [`NAME_LEN`] 7-bit character codes (ASCII
/// over the printable range), with trailing padding trimmed.
#[must_use]
pub fn decode_name(bytes: &[u8]) -> String {
    let raw: String = bytes
        .iter()
        .take(NAME_LEN)
        .map(|&c| {
            if (0x20..0x7f).contains(&c) {
                char::from(c)
            } else {
                ' '
            }
        })
        .collect();
    raw.trim_end().to_owned()
}

/// Encode a patch name into [`NAME_LEN`] space-padded 7-bit character bytes.
#[must_use]
pub fn encode_name(name: &str) -> [u8; NAME_LEN] {
    let mut out = [0x20u8; NAME_LEN];
    for (slot, ch) in out.iter_mut().zip(name.chars().take(NAME_LEN)) {
        let code = u32::from(ch);
        if (0x20..0x7f).contains(&code) {
            *slot = u8::try_from(code).unwrap_or(0x20);
        }
    }
    out
}

/// The 4-byte base address of patch memory `slot`: user patches `1..=100`
/// (area `00`), preset patches `101..=200` (area `01`). `None` if out of range.
#[must_use]
pub fn patch_base(slot: u16) -> Option<[u8; 4]> {
    let (area, index) = match slot {
        1..=100 => (0x00u8, slot - 1),
        101..=200 => (0x01u8, slot - 101),
        _ => return None,
    };
    Some([area, u8::try_from(index).unwrap_or(0), 0x00, 0x00])
}

/// The header of a stored patch: name, output level, and effect-chain order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchHeader {
    /// The patch name (trailing padding trimmed).
    pub name: String,
    /// Patch output level, raw `0..=100`.
    pub output_level: u8,
    /// The 13 effect-type bytes giving the block order in the signal chain.
    pub chain: Vec<u8>,
}

/// Base address of the temporary buffer (the current sound), bulk access.
const TEMP_BULK_BASE: [u8; 4] = [0x04, 0x00, 0x00, 0x00];
/// Sound Change Request address: a DT1 `00` here applies a bulk temp-buffer write.
const SCR_ADDR: [u8; 4] = [0x04, 0x7F, 0x7F, 0x7F];

/// Format `bytes` as space-separated uppercase hex.
fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse space-separated hex bytes.
fn from_hex(text: &str) -> Result<Vec<u8>> {
    text.split_whitespace()
        .map(|tok| {
            u8::from_str_radix(tok, 16).map_err(|_| Error::Patch(format!("bad hex {tok:?}")))
        })
        .collect()
}

/// A lossless whole-patch snapshot: every sub-block's raw bytes, exactly as the
/// device sends them. Unlike [`Patch`] (cataloged parameters only), this captures
/// everything -- name, chain order, control assigns, modulation, multi-byte
/// values -- so any patch round-trips faithfully.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawPatch {
    /// Schema version ([`PATCH_VERSION`]).
    pub version: u32,
    /// The decoded patch name (from the Level/Chain block), for readability.
    pub name: String,
    /// Sub-block offset (`0..=13`) to its raw bytes as space-separated hex.
    pub blocks: BTreeMap<u8, String>,
}

impl RawPatch {
    /// Output level and chain-order bytes, decoded from the Level/Chain block.
    fn header(&self) -> (u8, Vec<u8>) {
        self.blocks
            .get(&0x00)
            .and_then(|hex| from_hex(hex).ok())
            .map_or((0, Vec::new()), |b| {
                (
                    b.first().copied().unwrap_or(0),
                    b.get(1..14).unwrap_or(&[]).to_vec(),
                )
            })
    }

    /// A human-readable, multi-line decode of this patch using the parameter
    /// catalog. Uncataloged bytes (assigns, multi-byte values) are not shown.
    #[must_use]
    pub fn describe(&self) -> String {
        use std::fmt::Write as _;
        let (output_level, chain) = self.header();
        let chain_labels: Vec<&str> = chain
            .iter()
            .filter_map(|&c| param::Block::from_base(c).map(param::Block::label))
            .collect();

        let level = param::Param::from_key("output-level").map_or_else(
            || output_level.to_string(),
            |p| crate::units::display(p, Value::Int(i32::from(output_level))),
        );
        let mut out = String::new();
        let _ = writeln!(out, "Patch: {:?}", self.name);
        let _ = writeln!(out, "  output level: {level}");
        let _ = writeln!(out, "  chain: {}", chain_labels.join(" > "));

        let mut current: Option<&str> = None;
        for &p in param::ALL {
            if p.block() == param::Block::LevelChain {
                continue; // name / level / chain are shown in the header
            }
            if current != Some(p.block_label()) {
                let _ = writeln!(out, "  [{}]", p.block_label());
                current = Some(p.block_label());
            }
            let raw = self
                .blocks
                .get(&p.block().base())
                .and_then(|hex| from_hex(hex).ok())
                .and_then(|b| b.get(usize::from(p.offset())).copied());
            let value = raw.map_or_else(|| "-".to_owned(), |v| format_raw(p, v));
            let _ = writeln!(out, "    {:<22} {value}", p.key());
        }
        out
    }
}

/// Format a raw device byte for one parameter in display units.
fn format_raw(p: param::Param, raw: u8) -> String {
    let value = match p.kind() {
        Kind::Bool => Value::Bool(raw != 0),
        Kind::Int { .. } => Value::Int(i32::from(raw)),
        Kind::Enum { .. } => Value::Enum(i32::from(raw)),
    };
    crate::units::display(p, value)
}

/// A single parameter value as stored in a patch. Enums are kept as their label
/// for readability; integers and booleans use their native JSON types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    /// A boolean switch value.
    Bool(bool),
    /// An integer value, in raw device units.
    Int(i64),
    /// An enum value, stored as its label.
    Text(String),
}

/// A saved snapshot of a GX-700 patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    /// Schema version ([`PATCH_VERSION`]).
    pub version: u32,
    /// Parameter values, keyed by [`crate::Param::key`].
    pub params: BTreeMap<String, Scalar>,
}

fn to_scalar(param: param::Param, value: Value) -> Scalar {
    match value {
        Value::Bool(b) => Scalar::Bool(b),
        Value::Int(i) => Scalar::Int(i64::from(i)),
        Value::Enum(i) => {
            if let Kind::Enum { values, .. } = param.kind()
                && let Some(label) = usize::try_from(i).ok().and_then(|n| values.get(n))
            {
                return Scalar::Text((*label).to_owned());
            }
            Scalar::Int(i64::from(i))
        }
    }
}

fn from_scalar(param: param::Param, scalar: &Scalar) -> Result<Value> {
    let key = param.key();
    match param.kind() {
        Kind::Bool => match scalar {
            Scalar::Bool(b) => Ok(Value::Bool(*b)),
            _ => Err(Error::Patch(format!("{key}: expected a boolean"))),
        },
        Kind::Int { .. } => match scalar {
            Scalar::Int(n) => i32::try_from(*n)
                .map(Value::Int)
                .map_err(|_| Error::Patch(format!("{key}: value {n} out of range"))),
            _ => Err(Error::Patch(format!("{key}: expected an integer"))),
        },
        Kind::Enum { values, .. } => match scalar {
            Scalar::Text(s) => values
                .iter()
                .position(|v| v.eq_ignore_ascii_case(s))
                .and_then(|i| i32::try_from(i).ok())
                .map(Value::Enum)
                .ok_or_else(|| Error::Patch(format!("{key}: unknown value {s:?}"))),
            Scalar::Int(n) => i32::try_from(*n)
                .map(Value::Enum)
                .map_err(|_| Error::Patch(format!("{key}: value {n} out of range"))),
            Scalar::Bool(_) => Err(Error::Patch(format!("{key}: expected an enum value"))),
        },
    }
}

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
    use crate::param::Param;

    fn dev() -> Gx700<MockTransport> {
        Gx700::new(MockTransport::new())
    }

    fn p(key: &str) -> Param {
        Param::from_key(key).unwrap()
    }

    #[test]
    fn name_decodes_and_round_trips() {
        let bytes = [
            0x4E, 0x20, 0x52, 0x4F, 0x44, 0x47, 0x45, 0x52, 0x53, 0x3F, 0x20, 0x20,
        ];
        assert_eq!(decode_name(&bytes), "N RODGERS?");
        assert_eq!(encode_name("N RODGERS?"), bytes);
        // Over-long names are truncated; short ones space-padded to 12.
        assert_eq!(decode_name(&encode_name("JAZZ TONE")), "JAZZ TONE");
        assert_eq!(encode_name("WAY TOO LONG A NAME").len(), NAME_LEN);
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
    fn patch_base_addresses() {
        assert_eq!(patch_base(1), Some([0x00, 0x00, 0x00, 0x00]));
        assert_eq!(patch_base(2), Some([0x00, 0x01, 0x00, 0x00]));
        assert_eq!(patch_base(100), Some([0x00, 0x63, 0x00, 0x00]));
        assert_eq!(patch_base(101), Some([0x01, 0x00, 0x00, 0x00]));
        assert_eq!(patch_base(200), Some([0x01, 0x63, 0x00, 0x00]));
        assert_eq!(patch_base(0), None);
        assert_eq!(patch_base(201), None);
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
