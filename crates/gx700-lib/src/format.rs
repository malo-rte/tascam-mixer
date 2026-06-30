//! Saved-file formats and multi-format parsing.
//!
//! New saves use the rackctl-core envelope around the typed payload (the documented
//! format). Readers accept every shape a file has ever had, newest first, so a file
//! written by an older tool — or by the *other* frontend — still loads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use rackctl_gx700::RawPatch;
use rackctl_gx700::typed::{BlockData, Patch};

use crate::{DEVICE_ID, LIB_VERSION};

/// A whole-device **scene**: the user patch bank as `slot -> patch`. The map is
/// sparse, so a partial capture (a read failed mid-scene) survives rather than
/// silently filling the gap. `name` is the library filename, kept in the payload
/// too so the file is self-describing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scene {
    /// The scene's library name.
    #[serde(default)]
    pub name: String,
    /// Captured user patches by slot (`1..=100`).
    pub patches: BTreeMap<u16, Patch>,
}

impl Scene {
    /// An empty scene named `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            patches: BTreeMap::new(),
        }
    }
}

/// Parse a saved **patch** file, newest format first: a rackctl-core envelope around
/// the typed patch, a bare typed patch, or the CLI's legacy bare [`RawPatch`].
///
/// # Errors
/// If the text matches none of the known forms, or an envelope is from another
/// device / a newer format.
pub fn parse_patch(text: &str) -> Result<Patch, String> {
    if let Some(res) = rackctl_core::decode_item::<Patch>(DEVICE_ID, LIB_VERSION, text) {
        return res;
    }
    if let Ok(typed) = serde_json::from_str::<Patch>(text) {
        return Ok(typed);
    }
    serde_json::from_str::<RawPatch>(text)
        .map(|raw| Patch::from_raw(&raw))
        .map_err(|_| "unrecognised patch file".to_owned())
}

/// Parse a saved **scene** file across every format it has used: a rackctl-core
/// envelope (the canonical [`Scene`], or the GUI's earlier flat patch list), a bare
/// `Scene` or list, or the CLI's legacy raw-patch scene.
///
/// # Errors
/// If the text matches none of the known forms, or an envelope is from another
/// device / a newer format.
pub fn parse_scene(text: &str) -> Result<Scene, String> {
    if let Some(res) = rackctl_core::decode_payload(DEVICE_ID, LIB_VERSION, text) {
        return scene_from_value(res?);
    }
    if let Ok(scene) = serde_json::from_str::<Scene>(text) {
        return Ok(scene);
    }
    if let Ok(list) = serde_json::from_str::<Vec<Patch>>(text) {
        return Ok(scene_from_list(&list));
    }
    serde_json::from_str::<rackctl_gx700::Scene>(text)
        .map(|old| scene_from_legacy_raw(&old))
        .map_err(|_| "unrecognised scene file".to_owned())
}

/// A scene from an envelope payload value: the canonical struct, else the GUI's
/// earlier flat patch list (slots assigned `1..=N`).
fn scene_from_value(payload: serde_json::Value) -> Result<Scene, String> {
    if let Ok(scene) = serde_json::from_value::<Scene>(payload.clone()) {
        return Ok(scene);
    }
    serde_json::from_value::<Vec<Patch>>(payload)
        .map(|list| scene_from_list(&list))
        .map_err(|_| "unrecognised scene payload".to_owned())
}

/// A dense patch list becomes slots `1..=N`.
fn scene_from_list(list: &[Patch]) -> Scene {
    let patches = list
        .iter()
        .enumerate()
        .map(|(i, p)| (u16::try_from(i + 1).unwrap_or(0), p.clone()))
        .collect();
    Scene {
        name: String::new(),
        patches,
    }
}

/// The CLI's legacy raw-patch scene becomes typed.
fn scene_from_legacy_raw(old: &rackctl_gx700::Scene) -> Scene {
    let patches = old
        .patches
        .iter()
        .map(|(&slot, raw)| (slot, Patch::from_raw(raw)))
        .collect();
    Scene {
        name: old.name.clone(),
        patches,
    }
}

/// Parse a saved single-**block** preset file: a rackctl-core envelope around the
/// block data, or bare [`BlockData`].
///
/// # Errors
/// If the text matches neither form, or an envelope is from another device / a
/// newer format.
pub fn parse_block(text: &str) -> Result<BlockData, String> {
    if let Some(res) = rackctl_core::decode_item::<BlockData>(DEVICE_ID, LIB_VERSION, text) {
        return res;
    }
    serde_json::from_str::<BlockData>(text).map_err(|_| "unrecognised block file".to_owned())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn json_eq<T: Serialize>(a: &T, b: &T) -> bool {
        serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
    }

    #[test]
    fn patch_envelope_round_trips() {
        let p = Patch::init();
        let text = rackctl_core::encode_item(DEVICE_ID, LIB_VERSION, &p).unwrap();
        assert!(json_eq(&parse_patch(&text).unwrap(), &p));
    }

    #[test]
    fn patch_reads_legacy_bare_rawpatch() {
        // What the CLI used to write: a bare RawPatch.
        let raw = Patch::init().to_raw();
        let text = serde_json::to_string(&raw).unwrap();
        assert!(json_eq(
            &parse_patch(&text).unwrap(),
            &Patch::from_raw(&raw)
        ));
    }

    #[test]
    fn scene_reads_canonical_and_both_legacies() {
        // Canonical: enveloped Scene.
        let mut scene = Scene::new("gig");
        scene.patches.insert(1, Patch::init());
        let text = rackctl_core::encode_item(DEVICE_ID, LIB_VERSION, &scene).unwrap();
        assert_eq!(parse_scene(&text).unwrap().patches.len(), 1);

        // GUI legacy: a flat patch list (enveloped) -> slots 1..=N.
        let list = vec![Patch::init(), Patch::init()];
        let text = rackctl_core::encode_item(DEVICE_ID, LIB_VERSION, &list).unwrap();
        let got = parse_scene(&text).unwrap();
        assert_eq!(got.patches.len(), 2);
        assert!(got.patches.contains_key(&1) && got.patches.contains_key(&2));

        // CLI legacy: a bare raw-patch Scene.
        let mut old = rackctl_gx700::Scene::new("old".to_owned());
        old.patches.insert(7, Patch::init().to_raw());
        let text = serde_json::to_string(&old).unwrap();
        let got = parse_scene(&text).unwrap();
        assert_eq!(got.name, "old");
        assert!(got.patches.contains_key(&7));
    }

    #[test]
    fn other_device_envelope_is_rejected() {
        let text = rackctl_core::encode_item("us16x08", LIB_VERSION, &Patch::init()).unwrap();
        assert!(parse_patch(&text).is_err());
    }
}
