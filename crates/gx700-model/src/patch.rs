//! The raw patch representation and the patch-global helpers.
//!
//! [`RawPatch`] is the lossless, byte-exact form — every sub-block's raw bytes
//! exactly as the device stores them — with the patch-global accessors (name,
//! output level, signal chain) that operate on the Level/Chain block. [`Patch`] is
//! the older cataloged-parameter snapshot. The device I/O that reads and writes
//! these lives in the protocol crate (`rackctl-gx700`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

/// Format `bytes` as space-separated uppercase hex (the form stored in
/// [`RawPatch::blocks`]).
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse space-separated hex bytes (as stored in [`RawPatch::blocks`]).
///
/// # Errors
/// [`Error::Patch`] on a non-hex token.
pub fn from_hex(text: &str) -> Result<Vec<u8>> {
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
    /// catalog. Uncataloged bytes (multi-byte values) are not shown.
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

        // Control assigns (also in the Level/Chain block): each routes a Source
        // controller to a Target parameter, swept between Min and Max while the
        // Source is within Action lo..hi. Targets are named via MI Table 1.2.
        if let Some(hex) = self.blocks.get(&param::Block::LevelChain.base())
            && let Ok(bytes) = from_hex(hex)
        {
            let _ = writeln!(out, "  [control assigns]");
            for n in 1..=4 {
                let raw = |suffix: &str| -> i32 {
                    param::Param::from_key(&format!("assign{n}-{suffix}"))
                        .map_or(0, |p| p.decode(&bytes))
                };
                let target = raw("target");
                if target == 0 {
                    let _ = writeln!(out, "    assign {n}: (off)");
                    continue;
                }
                let mode = if raw("mode") == 1 { "toggle" } else { "normal" };
                let _ = writeln!(
                    out,
                    "    assign {n}: {} [{mode}]  min {} max {}  source {} ({}..{})",
                    param::assign_target_name(target),
                    raw("min"),
                    raw("max"),
                    raw("source"),
                    raw("act-lo"),
                    raw("act-hi"),
                );
            }
        }

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

    /// The signal-chain order: the 13 effect-type bytes from the Level/Chain
    /// block (offsets `1..=13`), each a block id `01`=Compressor .. `0D`=Reverb,
    /// giving the order of the effect blocks in the signal path. Empty if the
    /// Level/Chain block is missing or too short.
    #[must_use]
    pub fn chain(&self) -> Vec<u8> {
        self.header().1
    }

    /// Reorder the signal chain. `order` must be a permutation of the 13 block
    /// ids `1..=13` (`01`=Compressor .. `0D`=Reverb) -- every block exactly once,
    /// none repeated or omitted. Rewrites the Level/Chain block in place.
    ///
    /// # Errors
    /// [`Error::Patch`] if `order` is not such a permutation, or the Level/Chain
    /// block is missing or too short to hold the chain.
    pub fn set_chain(&mut self, order: &[u8]) -> Result<()> {
        let mut sorted = order.to_vec();
        sorted.sort_unstable();
        if sorted != (1u8..=13).collect::<Vec<u8>>() {
            return Err(Error::Patch(
                "chain must be a permutation of the 13 block ids 01..0D \
                 (each exactly once, none repeated or omitted)"
                    .to_owned(),
            ));
        }
        let hex = self
            .blocks
            .get(&0x00)
            .ok_or_else(|| Error::Patch("patch has no Level/Chain block".to_owned()))?;
        let mut bytes = from_hex(hex)?;
        let Some(slot) = bytes.get_mut(1..14) else {
            return Err(Error::Patch(format!(
                "Level/Chain block too short ({} bytes) to hold the chain",
                bytes.len()
            )));
        };
        slot.copy_from_slice(order);
        self.blocks.insert(0x00, to_hex(&bytes));
        Ok(())
    }

    /// The patch output level (Level/Chain offset `0`, raw `0..=100`). `0` if the
    /// Level/Chain block is missing.
    #[must_use]
    pub fn output_level(&self) -> u8 {
        self.header().0
    }

    /// Set the patch output level (Level/Chain offset `0`, raw `0..=100`),
    /// rewriting the Level/Chain block in place.
    ///
    /// # Errors
    /// [`Error::Patch`] if the Level/Chain block is missing or empty.
    pub fn set_output_level(&mut self, level: u8) -> Result<()> {
        let hex = self
            .blocks
            .get(&0x00)
            .ok_or_else(|| Error::Patch("patch has no Level/Chain block".to_owned()))?;
        let mut bytes = from_hex(hex)?;
        let Some(first) = bytes.first_mut() else {
            return Err(Error::Patch("Level/Chain block is empty".to_owned()));
        };
        *first = level;
        self.blocks.insert(0x00, to_hex(&bytes));
        Ok(())
    }

    /// Set the patch name (Level/Chain offsets `0E..=19`, 12 characters), encoding
    /// it to the device character set (truncated/padded to [`NAME_LEN`]) and
    /// updating the cached [`Self::name`].
    ///
    /// # Errors
    /// [`Error::Patch`] if the Level/Chain block is missing or too short to hold
    /// the name.
    pub fn set_name(&mut self, name: &str) -> Result<()> {
        let hex = self
            .blocks
            .get(&0x00)
            .ok_or_else(|| Error::Patch("patch has no Level/Chain block".to_owned()))?;
        let mut bytes = from_hex(hex)?;
        let encoded = encode_name(name);
        let Some(slot) = bytes.get_mut(14..14 + NAME_LEN) else {
            return Err(Error::Patch(format!(
                "Level/Chain block too short ({} bytes) to hold the name",
                bytes.len()
            )));
        };
        slot.copy_from_slice(&encoded);
        self.blocks.insert(0x00, to_hex(&bytes));
        self.name = decode_name(&encoded);
        Ok(())
    }

    /// Reset this patch to an empty/initialized state: every effect block
    /// bypassed (each block's enable flag, the first byte of the block, cleared),
    /// the signal chain in default order, output level `0`, and the name
    /// `"Empty"`. The blocks keep their remaining bytes, so the result stays a
    /// valid patch the device accepts; only the enables, chain, level, and name
    /// change.
    ///
    /// # Errors
    /// [`Error::Patch`] if the Level/Chain block is missing or too short to hold
    /// the chain, level, or name.
    pub fn initialize(&mut self) -> Result<()> {
        // Bypass every effect block (bases 0x01..=0x0D); the enable flag is the
        // first byte of each block.
        for base in 1u8..=13 {
            let Some(hex) = self.blocks.get(&base) else {
                continue;
            };
            let mut bytes = from_hex(hex)?;
            if let Some(first) = bytes.first_mut() {
                *first = 0;
            }
            self.blocks.insert(base, to_hex(&bytes));
        }
        self.set_chain(&(1u8..=13).collect::<Vec<u8>>())?;
        self.set_output_level(0)?;
        self.set_name("Empty")?;
        Ok(())
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

/// A single parameter value as stored in a [`Patch`]. Enums are kept as their
/// label for readability; integers and booleans use their native JSON types.
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

/// A saved snapshot of a GX-700 patch as cataloged parameter values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    /// Schema version ([`PATCH_VERSION`]).
    pub version: u32,
    /// Parameter values, keyed by [`crate::Param::key`].
    pub params: BTreeMap<String, Scalar>,
}

/// Convert a typed [`Value`] to its [`Scalar`] storage form (enums as labels).
#[must_use]
pub fn to_scalar(param: param::Param, value: Value) -> Scalar {
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

/// Convert a stored [`Scalar`] back to a typed [`Value`] for `param`.
///
/// # Errors
/// [`Error::Patch`] if the scalar's shape doesn't fit the parameter's kind.
pub fn from_scalar(param: param::Param, scalar: &Scalar) -> Result<Value> {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

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
    fn describe_shows_named_control_assigns() {
        let mut t = crate::typed::Patch::init();
        t.set("assign1-target", Value::Int(22)).unwrap(); // MI 1.2: Distortion: Drive
        t.set("assign1-min", Value::Int(10)).unwrap();
        t.set("assign1-max", Value::Int(90)).unwrap();
        let desc = t.to_raw().describe();
        assert!(desc.contains("[control assigns]"), "{desc}");
        assert!(desc.contains("assign 1: Distortion: Drive"), "{desc}");
        assert!(desc.contains("min 10 max 90"), "{desc}");
        assert!(desc.contains("assign 2: (off)"), "{desc}"); // unset reads as off
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
    fn chain_reads_and_reorders() {
        let mut blocks = BTreeMap::new();
        blocks.insert(
            0x00u8,
            "32 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 4A 41 5A 5A 20 54 4F 4E 45 20 20 20"
                .to_owned(),
        );
        let mut patch = RawPatch {
            version: PATCH_VERSION,
            name: "JAZZ TONE".to_owned(),
            blocks,
        };
        assert_eq!(patch.chain(), (1u8..=13).collect::<Vec<u8>>());

        // Valid reorder: swap Modulation (9) and Delay (10).
        let order = vec![1, 2, 3, 4, 5, 6, 7, 8, 10, 9, 11, 12, 13];
        patch.set_chain(&order).unwrap();
        assert_eq!(patch.chain(), order);
        // Output level and name bytes are untouched.
        assert!(patch.describe().contains("JAZZ TONE"));

        // Rejected: duplicate/omission and wrong length.
        assert!(
            patch
                .set_chain(&[1, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13])
                .is_err()
        );
        assert!(patch.set_chain(&[1, 2, 3]).is_err());
    }

    #[test]
    fn output_level_reads_and_sets() {
        let mut blocks = BTreeMap::new();
        blocks.insert(
            0x00u8,
            "32 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 4A 41 5A 5A 20 54 4F 4E 45 20 20 20"
                .to_owned(),
        );
        let mut patch = RawPatch {
            version: PATCH_VERSION,
            name: "JAZZ TONE".to_owned(),
            blocks,
        };
        assert_eq!(patch.output_level(), 0x32); // 50
        patch.set_output_level(80).unwrap();
        assert_eq!(patch.output_level(), 80);
        // The chain bytes are untouched by a level change.
        assert_eq!(patch.chain(), (1u8..=13).collect::<Vec<u8>>());
    }

    #[test]
    fn set_name_updates_block_and_field() {
        let mut blocks = BTreeMap::new();
        blocks.insert(
            0x00u8,
            "32 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 4A 41 5A 5A 20 54 4F 4E 45 20 20 20"
                .to_owned(),
        );
        let mut patch = RawPatch {
            version: PATCH_VERSION,
            name: "JAZZ TONE".to_owned(),
            blocks,
        };
        patch.set_name("CRUNCH").unwrap();
        assert_eq!(patch.name, "CRUNCH");
        assert!(patch.describe().contains("CRUNCH")); // landed in the block bytes
        // Over-long names truncate to 12 characters.
        patch.set_name("THIS NAME IS TOO LONG").unwrap();
        assert_eq!(patch.name, "THIS NAME IS");
        // Level and chain are untouched by a name change.
        assert_eq!(patch.output_level(), 0x32);
        assert_eq!(patch.chain(), (1u8..=13).collect::<Vec<u8>>());
    }

    #[test]
    fn initialize_blanks_the_patch() {
        let mut blocks = BTreeMap::new();
        // Level/Chain: level 0x32, chain reordered (10 before 9), name "JAZZ TONE".
        blocks.insert(
            0x00u8,
            "32 01 02 03 04 05 06 07 08 0A 09 0B 0C 0D 4A 41 5A 5A 20 54 4F 4E 45 20 20 20"
                .to_owned(),
        );
        // Two effect blocks with their enable byte (offset 0) set on.
        blocks.insert(0x01u8, "01 10 20".to_owned()); // Compressor enabled
        blocks.insert(0x04u8, "01 7F 40 0A".to_owned()); // Preamp enabled
        let mut patch = RawPatch {
            version: PATCH_VERSION,
            name: "JAZZ TONE".to_owned(),
            blocks,
        };

        patch.initialize().unwrap();

        assert_eq!(patch.name, "Empty");
        assert_eq!(patch.output_level(), 0);
        // Chain reset to the default order.
        assert_eq!(patch.chain(), (1u8..=13).collect::<Vec<u8>>());
        // Every effect block's enable byte is cleared; the rest is kept.
        let block = |base: &u8| from_hex(patch.blocks.get(base).unwrap()).unwrap();
        assert_eq!(block(&0x01), vec![0x00, 0x10, 0x20]);
        assert_eq!(block(&0x04), vec![0x00, 0x7F, 0x40, 0x0A]);
    }
}
