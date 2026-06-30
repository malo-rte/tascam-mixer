//! Command handlers, generic over the [`Transport`] so the same logic drives
//! the mock and the real ALSA rawmidi device.

use std::fs;
use std::thread::sleep;

use anyhow::{Context, Result, anyhow, bail};
use rackctl_gx700::typed::Patch as TypedPatch;
use rackctl_gx700::{Block, Gx700, Kind, Param, RawPatch, Transport, param};
use rackctl_gx700_lib::manage::{BANK_READ_PACE, slot_label};
use rackctl_gx700_lib::{library, manage};

use crate::value::{format_value, parse_value};

/// Load a saved patch by name from the library (any on-disk format) as a
/// [`RawPatch`]. Backend-free.
fn load_raw(name: &str) -> Result<RawPatch> {
    Ok(library::load_patch(name)
        .map_err(anyhow::Error::msg)?
        .to_raw())
}

/// Print the full parameter catalog. Backend-independent.
pub(crate) fn list() {
    println!("{:<22} {:<18} {:<24} PATCH OFFSET", "KEY", "BLOCK", "KIND");
    for &p in param::ALL {
        println!(
            "{:<22} {:<18} {:<24} {}",
            p.key(),
            p.block_label(),
            kind_str(p),
            hex4(p.patch_offset()),
        );
    }
}

/// Format a 4-byte address as space-separated hex.
fn hex4(addr: [u8; 4]) -> String {
    addr.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Print detailed metadata for one parameter. Backend-independent.
pub(crate) fn info(key: &str) -> Result<()> {
    let p = resolve(key)?;
    println!("{}  ({})", p.key(), p.block_label());
    println!("  patch offset: {}", hex4(p.patch_offset()));
    println!("  live address: {}", hex4(p.address()));
    match p.kind() {
        Kind::Bool => println!("  kind:  bool (on/off/true/false/1/0/yes/no)"),
        Kind::Int { min, max, default } => {
            println!("  kind:  int (raw device units)");
            println!("  range: {min}..={max} (default {default})");
        }
        Kind::Enum { values, default } => {
            println!("  kind:  enum (default {default})");
            let listed: Vec<String> = values
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{i}={v}"))
                .collect();
            println!("  values: {}", listed.join("  "));
        }
        _ => println!("  kind:  ?"),
    }
    Ok(())
}

/// Read and print one parameter's value.
pub(crate) fn get<T: Transport>(dev: &mut Gx700<T>, key: &str) -> Result<()> {
    let p = resolve(key)?;
    let value = dev.get(p)?;
    println!("{}", format_value(p, value));
    Ok(())
}

/// Parse and write one parameter's value. Silent on success.
pub(crate) fn set<T: Transport>(dev: &mut Gx700<T>, key: &str, raw_value: &str) -> Result<()> {
    let p = resolve(key)?;
    let value = parse_value(p, raw_value)?;
    dev.set(p, value)?;
    Ok(())
}

/// Print a patch in readable form: the current sound, or device slot `patch`.
pub(crate) fn dump_device<T: Transport>(
    dev: &mut Gx700<T>,
    patch: Option<u16>,
    json: bool,
) -> Result<()> {
    print_patch(&read_from_device(dev, patch)?, json)
}

/// Print a saved patch file in readable form. Backend-free.
pub(crate) fn dump_file(name: &str, json: bool) -> Result<()> {
    print_patch(&load_raw(name)?, json)
}

/// Edit a saved patch file in place: set one parameter by its catalog key, via
/// the typed model (load → set(key) → save byte-exact). Backend-free.
pub(crate) fn edit_file(name: &str, key: &str, raw_value: &str) -> Result<()> {
    let p = resolve(key)?;
    let value = parse_value(p, raw_value)?;
    let mut typed = library::load_patch(name).map_err(anyhow::Error::msg)?;
    typed.set(key, value)?;
    let path = library::save_patch(name, &typed).map_err(anyhow::Error::msg)?;
    eprintln!(
        "set {key} = {} in {}",
        format_value(p, value),
        path.display()
    );
    Ok(())
}

/// Print a patch as the human-readable decode, or (with `json`) the typed model
/// serialised to block-grouped JSON.
fn print_patch(raw: &RawPatch, json: bool) -> Result<()> {
    if json {
        let typed = rackctl_gx700::typed::Patch::from_raw(raw);
        println!(
            "{}",
            serde_json::to_string_pretty(&typed).context("serialising the typed patch")?
        );
    } else {
        print!("{}", raw.describe());
    }
    Ok(())
}

/// Save a whole patch (the current sound, or device slot `patch`) to the patch
/// library as the typed, enveloped JSON form.
pub(crate) fn save<T: Transport>(dev: &mut Gx700<T>, name: &str, slot: Option<u16>) -> Result<()> {
    let raw = read_from_device(dev, slot)?;
    let path =
        library::save_patch(name, &TypedPatch::from_raw(&raw)).map_err(anyhow::Error::msg)?;
    eprintln!("saved {:?} to {}", raw.name, path.display());
    Ok(())
}

/// Copy a stored patch from one slot to another on the device. The source `from`
/// may be any patch (user 1..=100 or preset 101..=200); the destination `to` must
/// be a user slot (1..=100), which it overwrites.
pub(crate) fn copy<T: Transport>(dev: &mut Gx700<T>, from: u16, to: u16) -> Result<()> {
    let raw = manage::copy_slot(dev, from, to).map_err(anyhow::Error::msg)?;
    eprintln!(
        "copied {} {:?} to {}",
        slot_label(from),
        raw.name,
        slot_label(to)
    );
    Ok(())
}

/// Save every patch in a bank to the patch library: the 100 user patches, or (with
/// `preset`) the 100 preset patches, each as `U001`/`P001`.. in the typed enveloped
/// form — the same library `load`, `dump --file`, and `patches --disk` read.
pub(crate) fn backup<T: Transport>(dev: &mut Gx700<T>, preset: bool) -> Result<()> {
    let count = manage::backup_bank(dev, preset, |slot, name| {
        println!("{}  {name:<12}", slot_label(slot));
    })
    .map_err(anyhow::Error::msg)?;
    eprintln!("backed up {count} patches");
    Ok(())
}

/// Load a saved whole-patch file onto the device: the current sound, or (with
/// `to_patch`) a user patch memory slot (which it overwrites).
pub(crate) fn load<T: Transport>(
    dev: &mut Gx700<T>,
    name: &str,
    to_patch: Option<u16>,
    json: bool,
) -> Result<()> {
    let raw = if json {
        // `name` is a path to a (typed/enveloped/legacy) patch file, not a library
        // name; read it directly and parse any supported on-disk form.
        let text = fs::read_to_string(name).with_context(|| format!("reading {name}"))?;
        rackctl_gx700_lib::parse_patch(&text)
            .map_err(anyhow::Error::msg)?
            .to_raw()
    } else {
        load_raw(name)?
    };
    let blocks = match to_patch {
        Some(slot) => {
            let n = dev.write_patch(slot, &raw)?;
            manage::verify_stored(dev, slot, &raw).map_err(anyhow::Error::msg)?;
            n
        }
        None => dev.write_current_patch(&raw)?,
    };
    let dest = to_patch.map_or_else(
        || "the current sound".to_owned(),
        |slot| format!("patch memory {slot}"),
    );
    eprintln!("loaded {:?} ({blocks} sub-blocks) into {dest}", raw.name);
    Ok(())
}

/// Preview a stored patch by writing it into the active sound (current sound),
/// without storing it. Non-destructive to memory, and works in any mode -- unlike
/// Program Change (`select`), it functions even while the unit is in BULK LOAD
/// mode, since it edits the temporary buffer rather than recalling a memory.
pub(crate) fn preview<T: Transport>(dev: &mut Gx700<T>, slot: u16) -> Result<()> {
    let raw = dev
        .read_patch(slot)
        .with_context(|| format!("reading patch {slot}"))?;
    let blocks = dev.write_current_patch(&raw)?;
    eprintln!(
        "previewing {} {:?} in the current sound ({blocks} sub-blocks) -- not stored",
        slot_label(slot),
        raw.name
    );
    Ok(())
}

/// Show or reorder the signal chain of a saved patch (on disk, no device). With
/// `set`, reorders the blocks and saves the patch; then load it with
/// `load --to-patch` (in BULK LOAD mode) to apply it on the unit.
pub(crate) fn chain(name: &str, set: Option<&[String]>) -> Result<()> {
    let mut raw = load_raw(name)?;
    if let Some(tokens) = set {
        let mut order = Vec::with_capacity(tokens.len());
        for t in tokens {
            order.push(chain_block_id(t).ok_or_else(|| {
                anyhow!(
                    "unknown block {t:?}; use the 13 tokens: \
                     comp wah dist preamp loop eq speaker ns mod delay chorus tremolo reverb"
                )
            })?);
        }
        raw.set_chain(&order).context("setting the signal chain")?;
        let path =
            library::save_patch(name, &TypedPatch::from_raw(&raw)).map_err(anyhow::Error::msg)?;
        eprintln!("updated the chain of {name:?}; saved to {}", path.display());
    }
    print_chain(&raw);
    Ok(())
}

/// Print a patch's signal chain as a numbered list of block labels and tokens.
fn print_chain(raw: &RawPatch) {
    let chain = raw.chain();
    if chain.is_empty() {
        eprintln!("(no Level/Chain block in this patch)");
        return;
    }
    for (i, &b) in chain.iter().enumerate() {
        let label =
            Block::from_base(b).map_or_else(|| format!("?{b:#04X}"), |blk| blk.label().to_owned());
        println!("{:>2}. {label} [{}]", i + 1, chain_token(b));
    }
}

/// Map a chain block token (CLI input) to its effect-type byte (`01`..`0D`).
fn chain_block_id(token: &str) -> Option<u8> {
    Some(match token.to_ascii_lowercase().as_str() {
        "comp" | "compressor" => 0x01,
        "wah" => 0x02,
        "dist" | "distortion" => 0x03,
        "preamp" => 0x04,
        "loop" => 0x05,
        "eq" | "equalizer" => 0x06,
        "speaker" | "spsim" | "sp" => 0x07,
        "ns" | "noise" => 0x08,
        "mod" | "modulation" => 0x09,
        "delay" => 0x0A,
        "chorus" => 0x0B,
        "tremolo" | "trem" | "pan" => 0x0C,
        "reverb" => 0x0D,
        _ => return None,
    })
}

/// The canonical short token for a chain block id, for display.
fn chain_token(id: u8) -> &'static str {
    match id {
        0x01 => "comp",
        0x02 => "wah",
        0x03 => "dist",
        0x04 => "preamp",
        0x05 => "loop",
        0x06 => "eq",
        0x07 => "speaker",
        0x08 => "ns",
        0x09 => "mod",
        0x0A => "delay",
        0x0B => "chorus",
        0x0C => "tremolo",
        0x0D => "reverb",
        _ => "?",
    }
}

/// Resolve a block token (the chain tokens: `comp`, `reverb`, …) to its [`Block`].
fn parse_block_token(token: &str) -> Result<Block> {
    chain_block_id(token)
        .and_then(Block::from_base)
        .ok_or_else(|| {
            anyhow!(
                "unknown block {token:?}; use one of: \
                 comp wah dist preamp loop eq speaker ns mod delay chorus tremolo reverb"
            )
        })
}

/// Save one effect block from a saved patch to that block type's preset library.
/// Backend-free.
pub(crate) fn block_save(patch: &str, block: &str, name: &str) -> Result<()> {
    let blk = parse_block_token(block)?;
    let typed = library::load_patch(patch).map_err(anyhow::Error::msg)?;
    let data = rackctl_gx700::typed::BlockData::from_patch(&typed, blk)
        .ok_or_else(|| anyhow!("patch {patch:?} has no {block} block"))?;
    let file = library::save_block(blk, name, &data).map_err(anyhow::Error::msg)?;
    eprintln!("saved {block} preset {name:?} to {}", file.display());
    Ok(())
}

/// Load a block preset into a saved patch, overwriting that block. Backend-free.
pub(crate) fn block_load(patch: &str, block: &str, name: &str) -> Result<()> {
    let blk = parse_block_token(block)?;
    let mut typed = library::load_patch(patch).map_err(anyhow::Error::msg)?;
    let data = library::load_block(blk, name).map_err(anyhow::Error::msg)?;
    data.apply_to(&mut typed);
    let file = library::save_patch(patch, &typed).map_err(anyhow::Error::msg)?;
    eprintln!("applied {block} preset {name:?} to {}", file.display());
    Ok(())
}

/// List saved presets for a block type. Backend-free.
pub(crate) fn block_list(block: &str) -> Result<()> {
    let blk = parse_block_token(block)?;
    let names = library::list_blocks(blk);
    if names.is_empty() {
        eprintln!("no saved {block} presets");
    }
    for name in names {
        println!("{name}");
    }
    Ok(())
}

/// Read the current sound, or device patch memory slot `patch`, as a [`RawPatch`].
fn read_from_device<T: Transport>(dev: &mut Gx700<T>, patch: Option<u16>) -> Result<RawPatch> {
    match patch {
        Some(slot) => dev.read_patch(slot).map_err(Into::into),
        None => dev.read_current_patch().map_err(Into::into),
    }
}

/// Save all 100 user patches as a single named scene (a whole-device snapshot) in
/// the typed enveloped form. The port lock makes this the device's sole accessor
/// for the run.
pub(crate) fn scene_save<T: Transport>(dev: &mut Gx700<T>, name: &str) -> Result<()> {
    let scene = manage::capture_scene(dev, name, |slot, patch_name| {
        println!("{}  {patch_name:<12}", slot_label(slot));
    })
    .map_err(anyhow::Error::msg)?;
    let path = library::save_scene(&scene).map_err(anyhow::Error::msg)?;
    eprintln!(
        "saved scene {name:?} ({} patches) to {}",
        scene.patches.len(),
        path.display()
    );
    Ok(())
}

/// Restore a named scene to the device, overwriting the user patch bank. This is
/// destructive, so `confirm` must be set (the CLI's `--yes`).
pub(crate) fn scene_restore<T: Transport>(
    dev: &mut Gx700<T>,
    name: &str,
    confirm: bool,
) -> Result<()> {
    let scene = library::load_scene(name).map_err(anyhow::Error::msg)?;
    if !confirm {
        bail!(
            "restoring scene {name:?} overwrites {} user patches on the device; \
             re-run with --yes to confirm",
            scene.patches.len()
        );
    }
    let count = manage::restore_scene(dev, &scene, |slot, patch_name| {
        println!("{}  {patch_name:<12}", slot_label(slot));
    })
    .map_err(anyhow::Error::msg)?;
    eprintln!("restored scene {name:?} ({count} patches) to the device");
    Ok(())
}

/// List scenes saved on disk. Backend-free.
pub(crate) fn scenes_list() {
    let names = library::list_scenes();
    if names.is_empty() {
        eprintln!("no saved scenes");
    }
    for name in names {
        println!("{name}");
    }
}

/// List patches saved on disk. Backend-free.
pub(crate) fn patches_disk() {
    let names = library::list_patches();
    if names.is_empty() {
        eprintln!("no saved patches");
    }
    for name in names {
        println!("{name}");
    }
}

/// Select a patch memory by Program Change.
pub(crate) fn select<T: Transport>(dev: &mut Gx700<T>, n: u8) -> Result<()> {
    dev.select_patch(n)?;
    Ok(())
}

/// List patch-memory slots with their names and output level: the 100 user
/// patches, or (with `preset`) the 100 preset patches.
pub(crate) fn patches<T: Transport>(dev: &mut Gx700<T>, preset: bool) -> Result<()> {
    let (slots, tag) = if preset {
        (101u16..=200, 'P')
    } else {
        (1u16..=100, 'U')
    };
    for slot in slots {
        let header = dev
            .read_patch_header(slot)
            .with_context(|| format!("reading patch {slot}"))?;
        sleep(BANK_READ_PACE); // ease off the US-16x08's MIDI input between dumps
        let n = if preset { slot - 100 } else { slot };
        let level = rackctl_gx700::Param::from_key("output-level").map_or_else(
            || header.output_level.to_string(),
            |p| {
                rackctl_gx700::units::display(
                    p,
                    rackctl_gx700::Value::Int(i32::from(header.output_level)),
                )
            },
        );
        println!("{tag}{n:03}  {:<12}  out {level}", header.name);
    }
    Ok(())
}

fn resolve(key: &str) -> Result<Param> {
    Param::from_key(key).with_context(|| format!("unknown parameter {key:?} (try `list`)"))
}

fn kind_str(p: Param) -> String {
    match p.kind() {
        Kind::Bool => "bool".to_owned(),
        Kind::Int { min, max, .. } => format!("int {min}..={max}"),
        Kind::Enum { values, .. } => format!("enum[{}]", values.len()),
        _ => "?".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use rackctl_gx700::MockTransport;

    fn dev() -> Gx700<MockTransport> {
        Gx700::new(MockTransport::new())
    }

    #[test]
    fn set_then_get_round_trips() {
        let mut d = dev();
        set(&mut d, "preamp-volume", "77").unwrap();
        // get() prints; assert via the device directly.
        let p = Param::from_key("preamp-volume").unwrap();
        assert_eq!(format_value(p, d.get(p).unwrap()), "77");
    }

    #[test]
    fn unknown_param_errors() {
        assert!(resolve("nonsuch").is_err());
    }
}
