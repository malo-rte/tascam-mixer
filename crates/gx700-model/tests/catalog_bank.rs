//! Cross-check the parameter catalog against a real 100-patch bank capture
//! (`tests/fixtures/bank/`): every catalogued parameter's byte must exist and fall
//! within its declared range across *all* the fixture patches. This guards the
//! offsets and ranges in `param.rs` against the actual device — a wrong offset or
//! too-narrow range shows up as an out-of-range byte in real data.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::similar_names
)]

use std::path::PathBuf;

use rackctl_gx700_model::{Kind, RawPatch, param};

/// Load every `U*.json` fixture as a `RawPatch`.
fn bank() -> Vec<(String, RawPatch)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bank");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read fixtures/bank") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .expect("file stem")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).expect("read patch json");
        let patch: RawPatch = serde_json::from_str(&text).expect("parse patch json");
        out.push((name, patch));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The raw bytes of one sub-block (by base id), parsed from the stored hex.
fn block_bytes(patch: &RawPatch, base: u8) -> Vec<u8> {
    patch.blocks.get(&base).map_or_else(Vec::new, |hex| {
        hex.split_whitespace()
            .map(|tok| u8::from_str_radix(tok, 16).expect("valid hex byte"))
            .collect()
    })
}

#[test]
fn every_catalogued_param_is_in_range_across_the_bank() {
    let patches = bank();
    assert!(
        patches.len() >= 100,
        "expected the full 100-patch bank, found {}",
        patches.len()
    );

    let mut failures: Vec<String> = Vec::new();
    for (name, patch) in &patches {
        for &p in param::ALL {
            let bytes = block_bytes(patch, p.block().base());
            let off = usize::from(p.offset());
            let Some(slice) = bytes.get(off..off + p.width()) else {
                failures.push(format!(
                    "{name}: {} reads past block {} (len {})",
                    p.key(),
                    p.block().label(),
                    bytes.len()
                ));
                continue;
            };
            let raw = p.encoding().decode(slice);
            let ok = match p.kind() {
                Kind::Bool => raw == 0 || raw == 1,
                Kind::Int { min, max, .. } => (min..=max).contains(&raw),
                Kind::Enum { values, .. } => usize::try_from(raw).is_ok_and(|v| v < values.len()),
                _ => true,
            };
            if !ok {
                failures.push(format!(
                    "{name}: {} = {raw} out of range for {:?}",
                    p.key(),
                    p.kind()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} catalog/bank mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
