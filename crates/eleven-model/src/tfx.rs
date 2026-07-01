//! Parser for the Eleven Rack binary **patch file** format (`.tfx`).
//!
//! A patch file is a header, the patch name (word-swapped ASCII from offset 64), then
//! from offset 92 a sequence of *blocks*. Each block is a header word
//! `[size_lo size_hi 0x20 block_id]` followed by `size / 8` entries, each a 4-byte
//! `FourCC` tag (stored little-endian, so its ASCII reads byte-swapped) and a 32-bit
//! value. Global-block values are integers; effect-block values are 32-bit floats.
//!
//! This layout was reverse-engineered and validated across the full 1,243-patch ERUG
//! corpus; see `docs/eleven-rack-patch-format.adoc`. The parser preserves the raw
//! 32-bit value of each parameter losslessly; interpreting it (int vs. float) is the
//! caller's job, since it depends on the block.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Offset where the patch name begins (word-swapped ASCII).
const NAME_OFFSET: usize = 64;
/// Offset where the block table begins.
const BODY_OFFSET: usize = 92;
/// The constant marker byte (3rd) in a block header word.
const BLOCK_MARKER: u8 = 0x20;

/// A parsed Eleven Rack patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patch {
    /// The patch name.
    pub name: String,
    /// The signal-chain blocks, in file order.
    pub blocks: Vec<Block>,
}

/// One block of a patch (a signal-chain slot).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// The block id (signal-chain slot: `0x41` global, `0x49` amp, …).
    pub id: u8,
    /// The block's parameters, in file order.
    pub params: Vec<Param>,
}

/// One keyed parameter: a 4-character tag and its raw 32-bit value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    /// The `FourCC` tag (e.g. `"Tmpo"`, `"Driv"`), de-byte-swapped to logical order.
    pub tag: String,
    /// The raw 32-bit value (the little-endian word). For the global block this is
    /// an integer; for effect blocks a 32-bit float. Kept raw so no information is
    /// lost; interpretation is block-specific.
    pub value: u32,
}

impl Param {
    /// Reinterpret the raw value as a 32-bit float (effect-block parameters).
    #[must_use]
    pub fn as_f32(&self) -> f32 {
        f32::from_bits(self.value)
    }
}

/// True if `word` (4 bytes, stored little-endian) is a `FourCC` tag: its logical
/// (byte-swapped) form starts with a letter and is all printable ASCII.
fn is_tag(word: &[u8]) -> bool {
    matches!(word, [b0, b1, b2, b3]
        if b3.is_ascii_alphabetic() && [b0, b1, b2, b3].iter().all(|&&c| (0x20..0x7f).contains(&c)))
}

/// Decode a 4-byte little-endian tag word to its logical `FourCC` string.
fn tag_string(word: &[u8]) -> String {
    word.iter().rev().map(|&b| char::from(b)).collect()
}

/// Parse a `.tfx` patch file.
///
/// # Errors
/// [`Error::Tfx`] if the data is too short to contain the header and block table.
pub fn parse(data: &[u8]) -> Result<Patch> {
    if data.len() < BODY_OFFSET {
        return Err(Error::Tfx(format!(
            "file too short: {} bytes (need at least {BODY_OFFSET})",
            data.len()
        )));
    }
    Ok(Patch {
        name: parse_name(data),
        blocks: parse_blocks(data),
    })
}

/// Read the patch name (word-swapped ASCII from [`NAME_OFFSET`] up to the first
/// zero word or the block table).
fn parse_name(data: &[u8]) -> String {
    let mut bytes = Vec::new();
    let mut off = NAME_OFFSET;
    while off + 4 <= BODY_OFFSET {
        let Some(word) = data.get(off..off + 4) else {
            break;
        };
        if word == [0, 0, 0, 0] {
            break;
        }
        bytes.extend(word.iter().rev().copied());
        off += 4;
    }
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(bytes.get(..end).unwrap_or(&bytes))
        .trim()
        .to_owned()
}

/// Walk the block table from [`BODY_OFFSET`].
fn parse_blocks(data: &[u8]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut off = BODY_OFFSET;
    while let Some(header) = data.get(off..off + 4) {
        // A `FourCC` where a block header is expected means the table has ended.
        if is_tag(header) {
            break;
        }
        let [size_lo, size_hi, marker, block_id] = header else {
            break;
        };
        if *marker != BLOCK_MARKER {
            off += 4;
            continue;
        }
        let size = usize::from(*size_hi) << 8 | usize::from(*size_lo);
        let id = *block_id;
        off += 4;
        let end = off.saturating_add(size).min(data.len());
        let mut params = Vec::new();
        while off + 8 <= end {
            let Some(tag_word) = data.get(off..off + 4) else {
                break;
            };
            if !is_tag(tag_word) {
                break;
            }
            let value = data
                .get(off + 4..off + 8)
                .and_then(|s| <[u8; 4]>::try_from(s).ok())
                .map_or(0, u32::from_le_bytes);
            params.push(Param {
                tag: tag_string(tag_word),
                value,
            });
            off += 8;
        }
        blocks.push(Block { id, params });
        off = end;
    }
    blocks
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::cast_possible_truncation
    )]
    use super::*;

    /// Build a synthetic `.tfx` byte image for the parser tests.
    fn build(name: &str, blocks: &[(u8, &[(&str, u32)])]) -> Vec<u8> {
        let mut d = vec![0u8; BODY_OFFSET];
        // Name at NAME_OFFSET, word-swapped per 4-byte group.
        let nb = name.as_bytes();
        assert!(
            nb.len() <= BODY_OFFSET - NAME_OFFSET - 4,
            "test name too long"
        );
        for (i, chunk) in nb.chunks(4).enumerate() {
            let mut w = [0u8; 4];
            w[..chunk.len()].copy_from_slice(chunk);
            let off = NAME_OFFSET + i * 4;
            d[off..off + 4].copy_from_slice(&[w[3], w[2], w[1], w[0]]);
        }
        // Blocks.
        for (id, params) in blocks {
            let size = params.len() * 8;
            d.push((size & 0xff) as u8);
            d.push(((size >> 8) & 0xff) as u8);
            d.push(BLOCK_MARKER);
            d.push(*id);
            for (tag, val) in *params {
                let t = tag.as_bytes();
                assert_eq!(t.len(), 4, "test tags must be 4 chars");
                d.extend_from_slice(&[t[3], t[2], t[1], t[0]]);
                d.extend_from_slice(&val.to_le_bytes());
            }
        }
        // word0 = big-endian total size.
        let total = d.len() as u32;
        d[0..4].copy_from_slice(&total.to_be_bytes());
        d
    }

    #[test]
    fn parses_name_blocks_and_values() {
        let img = build(
            "DC Modern",
            &[
                (0x41, &[("Tmpo", 500_000), ("PIGI", 125)]),
                (0x49, &[("sld1", 0x1234_5678)]),
            ],
        );
        let patch = parse(&img).unwrap();
        assert_eq!(patch.name, "DC Modern");
        assert_eq!(patch.blocks.len(), 2);

        let g = &patch.blocks[0];
        assert_eq!(g.id, 0x41);
        assert_eq!(
            g.params[0],
            Param {
                tag: "Tmpo".into(),
                value: 500_000
            }
        );
        assert_eq!(g.params[1].tag, "PIGI");
        assert_eq!(g.params[1].value, 125);

        let amp = &patch.blocks[1];
        assert_eq!(amp.id, 0x49);
        assert_eq!(amp.params[0].tag, "sld1");
        assert_eq!(amp.params[0].value, 0x1234_5678);
    }

    #[test]
    fn name_not_multiple_of_four_round_trips() {
        let img = build("A is A 800", &[(0x41, &[("RVol", 1)])]);
        assert_eq!(parse(&img).unwrap().name, "A is A 800");
    }

    #[test]
    fn float_values_reinterpret() {
        let bits = 0.5f32.to_bits();
        let img = build("x", &[(0x45, &[("Driv", bits)])]);
        let patch = parse(&img).unwrap();
        assert!((patch.blocks[0].params[0].as_f32() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn too_short_is_an_error() {
        assert!(matches!(parse(&[0u8; 10]), Err(Error::Tfx(_))));
    }
}
