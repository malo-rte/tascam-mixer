//! A captured patch as raw `SysEx` block payloads — the unit of MIDI library
//! backup/restore.
//!
//! Unlike a `.tfx` [`crate::Patch`] (a typed, on-disk patch file), a [`PatchBackup`] is
//! a *device-faithful* snapshot: the bytes the unit itself returned for each block
//! read (`01 <block>` → `12 <block> <payload>`). Restoring writes those payloads
//! straight back into the edit buffer (`00 <block> <payload>`) and runs the store
//! sequence, so it reproduces the patch without needing to decode per-parameter
//! value scaling. This is the backup format that round-trips through hardware
//! byte-for-byte; the `.tfx` form is for interchange with the Windows editor.
//!
//! The read-only aggregate block (`0x01`) and the directory/commit blocks
//! (`0x02`..`0x04`, used by the store sequence itself) are *not* patch content and
//! are excluded from a backup — see [`BlockData::is_restorable`].

use serde::{Deserialize, Serialize};

/// The aggregate full-patch blob block. It is *read-only* (writing it does
/// nothing), so it is captured for reference but never written back.
pub const AGGREGATE_BLOCK: u8 = 0x01;

/// The directory/commit blocks driven by the store sequence (`0x02` commit,
/// `0x03` begin, `0x04` directory entry). These are not per-patch content.
pub const DIRECTORY_BLOCKS: [u8; 3] = [0x02, 0x03, 0x04];

/// Device-managed/derived blocks that are not patch content: not restored and not
/// verified. `0x34` is a one-byte value that ticks on every store (a store /
/// revision counter), so it never round-trips and must be ignored.
pub const VOLATILE_BLOCKS: [u8; 1] = [0x34];

/// Flat (non-parameter-table) blocks that are known-safe *patch content* to write
/// back on restore: the name (`0x05`) and the FX-chain routing (`0x08`).
///
/// SAFETY: every other flat block a capture turns up is *device/system* state, not
/// patch content — e.g. `0x22` is the unit's device-info register, and the
/// single-byte `0x30..0x3f` blocks are model selectors / system flags. Blindly
/// whole-block-writing those put a unit into **Retail Demo Mode** (dimmed display,
/// blocked Reset Memory) during testing. So restore writes a flat block *only* if
/// it is on this allowlist; everything else is skipped. Parameter values still
/// restore fully via the per-parameter path ([`BlockData::param_records`]).
pub const SAFE_FLAT_BLOCKS: [u8; 2] = [0x05, 0x08];

/// What restore should do with a captured block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAction {
    /// Not written (read-only aggregate, directory/commit, volatile, or any flat
    /// block not on [`SAFE_FLAT_BLOCKS`] — i.e. device/system state).
    Skip,
    /// Written whole (`00 <block> <payload>`) — a safe flat patch-content block.
    WholeBlock,
    /// Replayed per parameter, keyed by `target` onto the live index.
    PerParam,
}

/// One record of a parameter-table block.
///
/// The `target` is the **stable logical parameter id**; the `index` is the
/// *physical* slot (the byte2 of a `11 <block> <index>` wire address) which the
/// unit **reassigns** when it rebuilds the table (so the same parameter appears at
/// a different `index` after a reload). Restore therefore keys values by `target`
/// and writes them to the *live* `index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamRecord {
    /// The current `0..=127` value.
    pub value: u8,
    /// The physical index in the live table (unstable — reassigned on reload).
    pub index: u8,
    /// The stable logical parameter id (the indirection target).
    pub target: u8,
}

/// One captured block: its 1-byte id and the raw payload the unit returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockData {
    /// The block id (the 1-byte address in `01 <block>` / `00 <block> …`).
    pub id: u8,
    /// The block payload bytes (everything after the echoed id in the reply).
    pub bytes: Vec<u8>,
}

impl BlockData {
    /// Whether this block should be *written back* on restore. Excludes the
    /// read-only aggregate, the store sequence's own directory/commit blocks, and
    /// device-managed volatile blocks (see [`VOLATILE_BLOCKS`]).
    #[must_use]
    pub fn is_restorable(&self) -> bool {
        self.id != AGGREGATE_BLOCK
            && !DIRECTORY_BLOCKS.contains(&self.id)
            && !VOLATILE_BLOCKS.contains(&self.id)
    }

    /// If this block is a *parameter table* — a leading `<count>` byte then `count`
    /// 3-byte `[value][index][target]` records — return the parsed [`ParamRecord`]s.
    ///
    /// These blocks (e.g. the amp table `0x21`) are an indexed structure the unit
    /// builds, not flat data: a whole-block write does not take, and the physical
    /// `index` is reassigned on reload. On restore they are replayed *per
    /// parameter*, keyed by the stable [`ParamRecord::target`] onto the live index.
    #[must_use]
    pub fn param_records(&self) -> Option<Vec<ParamRecord>> {
        let count = usize::from(*self.bytes.first()?);
        let records = self.bytes.get(1..)?;
        if count == 0 || records.len() != count * 3 {
            return None;
        }
        Some(
            records
                .chunks_exact(3)
                .filter_map(|r| {
                    Some(ParamRecord {
                        value: *r.first()?,
                        index: *r.get(1)?,
                        target: *r.get(2)?,
                    })
                })
                .collect(),
        )
    }

    /// Whether this block is a parameter table (see [`Self::param_records`]).
    #[must_use]
    pub fn is_param_table(&self) -> bool {
        self.param_records().is_some()
    }

    /// How restore should write this block. Parameter tables replay per parameter;
    /// only the [`SAFE_FLAT_BLOCKS`] flat blocks are written whole; everything else
    /// (system/device state) is skipped. See [`SAFE_FLAT_BLOCKS`] for why.
    #[must_use]
    pub fn restore_action(&self) -> RestoreAction {
        if !self.is_restorable() {
            RestoreAction::Skip
        } else if self.is_param_table() {
            RestoreAction::PerParam
        } else if SAFE_FLAT_BLOCKS.contains(&self.id) {
            RestoreAction::WholeBlock
        } else {
            RestoreAction::Skip
        }
    }

    /// The `(target, value)` map of a parameter table, sorted by target — the
    /// reload-stable form for comparing two captures of the same block.
    #[must_use]
    pub fn param_values_by_target(&self) -> Option<Vec<(u8, u8)>> {
        let mut v: Vec<(u8, u8)> = self
            .param_records()?
            .into_iter()
            .map(|r| (r.target, r.value))
            .collect();
        v.sort_unstable();
        Some(v)
    }
}

/// A device-faithful snapshot of one patch: its name and captured block payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchBackup {
    /// The patch name (from the `0x05` name block at capture time).
    pub name: String,
    /// Captured blocks, in capture order.
    pub blocks: Vec<BlockData>,
}

impl PatchBackup {
    /// Build a backup from a name and `(block id, payload)` pairs.
    #[must_use]
    pub fn new(name: impl Into<String>, blocks: Vec<BlockData>) -> Self {
        Self {
            name: name.into(),
            blocks,
        }
    }

    /// The blocks that should be written back on restore (excludes the read-only
    /// aggregate and the directory/commit blocks).
    pub fn restorable(&self) -> impl Iterator<Item = &BlockData> {
        self.blocks.iter().filter(|b| b.is_restorable())
    }

    /// The captured payload of a given block, if present.
    #[must_use]
    pub fn block(&self, id: u8) -> Option<&[u8]> {
        self.blocks
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.bytes.as_slice())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn sample() -> PatchBackup {
        PatchBackup::new(
            "Test Patch",
            vec![
                BlockData {
                    id: 0x01,
                    bytes: vec![1, 2, 3],
                }, // aggregate (read-only)
                BlockData {
                    id: 0x05,
                    bytes: b"Test Patch".to_vec(),
                },
                BlockData {
                    id: 0x21,
                    bytes: vec![0x14, 0x6d, 0x07, 0x00],
                },
                BlockData {
                    id: 0x03,
                    bytes: vec![0, 0],
                }, // directory/commit
            ],
        )
    }

    #[test]
    fn restorable_excludes_aggregate_and_directory_blocks() {
        let ids: Vec<u8> = sample().restorable().map(|b| b.id).collect();
        assert_eq!(ids, vec![0x05, 0x21]);
    }

    #[test]
    fn param_records_parse_and_compare_by_target() {
        // count=2, records [value index target]
        let b = BlockData {
            id: 0x21,
            bytes: vec![2, 55, 0x56, 0x0b, 43, 0x4c, 0x02],
        };
        assert_eq!(
            b.param_records(),
            Some(vec![
                ParamRecord {
                    value: 55,
                    index: 0x56,
                    target: 0x0b
                },
                ParamRecord {
                    value: 43,
                    index: 0x4c,
                    target: 0x02
                },
            ])
        );
        // The same parameters at *different* physical indices compare equal by target.
        let reindexed = BlockData {
            id: 0x21,
            bytes: vec![2, 43, 0x1f, 0x02, 55, 0x1e, 0x0b],
        };
        assert_eq!(
            b.param_values_by_target(),
            reindexed.param_values_by_target()
        );
        // a flat block (name) and a bad-length block are not param tables
        assert!(
            !BlockData {
                id: 0x05,
                bytes: b"Name".to_vec()
            }
            .is_param_table()
        );
        assert!(
            !BlockData {
                id: 0x34,
                bytes: vec![127]
            }
            .is_param_table()
        );
    }

    #[test]
    fn volatile_block_is_not_restorable() {
        assert!(
            !BlockData {
                id: 0x34,
                bytes: vec![127]
            }
            .is_restorable()
        );
    }

    #[test]
    fn restore_action_only_writes_known_patch_content() {
        let action = |id: u8, bytes: Vec<u8>| BlockData { id, bytes }.restore_action();
        // amp param-table -> per-parameter
        assert_eq!(
            action(0x21, vec![1, 55, 0x56, 0x0b]),
            RestoreAction::PerParam
        );
        // name + fx-chain -> whole-block (safe flat)
        assert_eq!(action(0x05, b"Name".to_vec()), RestoreAction::WholeBlock);
        assert_eq!(action(0x08, vec![4, 0, 0x7f]), RestoreAction::WholeBlock);
        // system/device flat blocks -> skipped (the bug that caused demo mode)
        assert_eq!(action(0x22, vec![0x41]), RestoreAction::Skip); // device info
        assert_eq!(action(0x07, vec![0x1d, 0x6e]), RestoreAction::Skip); // config
        assert_eq!(action(0x30, vec![0x00]), RestoreAction::Skip); // model selector
        assert_eq!(action(0x34, vec![0x7f]), RestoreAction::Skip); // store counter
        assert_eq!(action(0x01, vec![1, 2, 3]), RestoreAction::Skip); // aggregate
    }

    #[test]
    fn block_lookup_by_id() {
        let b = sample();
        assert_eq!(b.block(0x05), Some(&b"Test Patch"[..]));
        assert_eq!(b.block(0x99), None);
    }

    #[test]
    fn json_round_trips() {
        let b = sample();
        let json = serde_json::to_string(&b).unwrap();
        let back: PatchBackup = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }
}
