//! A *scene*: a named snapshot of the device's whole user patch bank.
//!
//! Where a [`RawPatch`] is one sound, a [`Scene`] is the entire editable state of
//! the device -- all 100 user patches -- captured under a name so it can be backed
//! up and restored as a unit (e.g. a per-gig or per-project set).
//!
//! Preset patches (factory slots 101..=200) are deliberately *not* part of a
//! scene: the device does not accept writes to the factory area, so they could
//! never be restored. Archive those read-only via the CLI's `backup --preset`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::patch::RawPatch;

/// Schema version stamped into serialized [`Scene`] files.
pub const SCENE_VERSION: u32 = 1;

/// The number of user patch slots a full scene covers (device slots 1..=100).
pub const USER_PATCH_COUNT: u16 = 100;

/// A named snapshot of the 100 user patches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scene {
    /// Schema version ([`SCENE_VERSION`]).
    pub version: u32,
    /// Human-readable scene name.
    pub name: String,
    /// User patch slot (`1..=100`) to its captured patch. A scene may be partial
    /// if a read failed mid-capture, so this is not assumed full on load.
    pub patches: BTreeMap<u16, RawPatch>,
}

impl Scene {
    /// An empty scene named `name`, stamped with the current schema version.
    #[must_use]
    pub fn new(name: String) -> Self {
        Self {
            version: SCENE_VERSION,
            name,
            patches: BTreeMap::new(),
        }
    }
}
