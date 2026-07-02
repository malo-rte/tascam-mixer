//! Implementations of the `rackctl-eleven` subcommands.
//!
//! Mirrors `rackctl-gx700`'s split: the CLI definition and dispatch live in
//! `main.rs`, and these are thin adapters that print. The device-touching work
//! (capture / save / load / copy / bank backup / scenes / named CC) lives in
//! `rackctl-eleven-lib`'s `manage` module, so a GUI shares one implementation.
//! Parameter-level commands (`get`/`set`/`scan`) run on the mock or hardware; the
//! patch/slot commands need a connected unit (`--port`).

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};

#[cfg(feature = "alsa")]
use rackctl_eleven::RawMidi;
use rackctl_eleven::{Eleven, MockTransport, RawValue, Transport};

// ---- device opening ----

/// The `--midi-log` path, set once at startup and read when opening hardware.
static MIDI_LOG: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Record the `--midi-log FILE` path. Called once from `main` before dispatch;
/// any subsequent hardware connection logs its byte-level MIDI I/O to it.
pub fn set_midi_log(path: Option<PathBuf>) {
    let _ = MIDI_LOG.set(path);
}

/// Open a real unit at `port`, enabling the byte-level MIDI log when `--midi-log`
/// was given.
#[cfg(feature = "alsa")]
fn open_raw(port: &str) -> Result<RawMidi> {
    let mut dev = RawMidi::open(port)?;
    if let Some(path) = MIDI_LOG.get().and_then(Option::as_deref) {
        dev.enable_midi_log(path)?;
    }
    Ok(dev)
}

/// Open the device for parameter-level commands: the mock (`--mock`) or the
/// hardware port (`--port`).
fn open_device(mock: bool, port: Option<&str>) -> Result<Eleven<Box<dyn Transport>>> {
    if mock {
        return Ok(Eleven::new(Box::new(MockTransport::new())));
    }
    #[cfg(feature = "alsa")]
    {
        let port = port.context("no --port given (run `ports`, or use --mock)")?;
        Ok(Eleven::new(Box::new(open_raw(port)?)))
    }
    #[cfg(not(feature = "alsa"))]
    {
        let _ = port;
        anyhow::bail!("built without the `alsa` feature; re-run with --mock")
    }
}

/// Open a real unit for hardware-only commands (no mock equivalent).
#[cfg(feature = "alsa")]
fn open_rawmidi(port: Option<&str>) -> Result<RawMidi> {
    let port = port.context("this command needs --port (a connected unit)")?;
    open_raw(port)
}

// ---- parameter commands (mock or hardware) ----

/// Read one parameter and print its raw bytes and decoded word.
pub fn get(mock: bool, port: Option<&str>, addr: &str) -> Result<()> {
    let bytes = parse_addr(addr)?;
    // A single byte is a stable amp-parameter `target`: resolve its live index from
    // the unit and read the value. A 3-byte address is a raw read.
    if let [target] = bytes.as_slice() {
        return get_amp(port, *target);
    }
    let mut dev = open_device(mock, port)?;
    let raw = dev.read_raw(&bytes)?;
    let word = raw.decode();
    println!(
        "{} -> {}  (word {word:#x} = {word})",
        addr.trim(),
        hex(raw.as_bytes())
    );
    Ok(())
}

/// Write a knob value (`b0`) at an address, then read it back to verify.
pub fn set(mock: bool, port: Option<&str>, addr: &str, value: &str) -> Result<()> {
    let bytes = parse_addr(addr)?;
    let b0 = parse_byte(value)?;
    // A single byte is a stable amp-parameter `target` (see `get`); 3 bytes is raw.
    if let [target] = bytes.as_slice() {
        return set_amp(port, *target, b0);
    }
    let mut dev = open_device(mock, port)?;
    // Knob-parameter value form: b0 in the low byte, with the 0x10 type tag.
    let want = RawValue::from_bytes([b0, 0, 0, 0, 0x10]);
    dev.write_raw(&bytes, &want)?;
    let got = dev.read_raw(&bytes)?;
    let ok = got.as_bytes().first() == Some(&b0);
    println!(
        "set {} = {b0:#04X} -> read back {}  [{}]",
        addr.trim(),
        hex(got.as_bytes()),
        if ok { "verified" } else { "MISMATCH" }
    );
    Ok(())
}

/// Read the live amp parameter table (`get_amp_param` resolves the target's index).
#[cfg(feature = "alsa")]
fn get_amp(port: Option<&str>, target: u8) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let (block, rec) =
        rackctl_eleven_lib::manage::get_amp_param(&mut dev, target).map_err(anyhow::Error::msg)?;
    println!(
        "amp param target {:#04X} = {}  (block {block:#04X}, live index {:#04X})",
        rec.target, rec.value, rec.index
    );
    Ok(())
}

/// Write an amp parameter by stable target, resolving its live index first.
#[cfg(feature = "alsa")]
fn set_amp(port: Option<&str>, target: u8, value: u8) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let (block, rec) = rackctl_eleven_lib::manage::set_amp_param(&mut dev, target, value)
        .map_err(anyhow::Error::msg)?;
    let ok = rec.value == value;
    println!(
        "set amp param target {:#04X} = {value} -> read back {}  (block {block:#04X} index {:#04X})  [{}]",
        target,
        rec.value,
        rec.index,
        if ok { "verified" } else { "MISMATCH" }
    );
    if !ok {
        println!(
            "note: the unit did not apply the change. Host-initiated per-parameter sets \
             are unresolved (see eleven-rack-sysex-protocol.adoc) — use `cc <name> \
             <value>` for live amp control."
        );
    }
    Ok(())
}

#[cfg(not(feature = "alsa"))]
fn get_amp(_port: Option<&str>, _target: u8) -> Result<()> {
    anyhow::bail!("amp-parameter addressing needs the `alsa` feature and a connected unit")
}

#[cfg(not(feature = "alsa"))]
fn set_amp(_port: Option<&str>, _target: u8, _value: u8) -> Result<()> {
    anyhow::bail!("amp-parameter addressing needs the `alsa` feature and a connected unit")
}

/// Scan `<prefix> from`..`<prefix> to`, printing each address that answered. The
/// special prefix `amp` instead dumps the live amp parameter table.
pub fn scan(mock: bool, port: Option<&str>, prefix: &str, from: &str, to: &str) -> Result<()> {
    if prefix.eq_ignore_ascii_case("amp") {
        return scan_amp(port);
    }
    let base = parse_addr(prefix)?;
    let from = parse_byte(from)?;
    let to = parse_byte(to)?;
    let addrs: Vec<Vec<u8>> = (from..=to)
        .map(|b| {
            let mut a = base.clone();
            a.push(b);
            a
        })
        .collect();
    let mut dev = open_device(mock, port)?;
    let answers = dev.scan(&addrs)?;
    println!("{} of {} addresses answered", answers.len(), addrs.len());
    for (addr, value) in answers {
        println!(
            "{}  {}  (word {:#x})",
            hex(&addr),
            hex(value.as_bytes()),
            value.decode()
        );
    }
    Ok(())
}

/// Dump the current sound's live amp parameter table (target / value / live index).
#[cfg(feature = "alsa")]
fn scan_amp(port: Option<&str>) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let (block, recs) =
        rackctl_eleven_lib::manage::amp_param_table(&mut dev).map_err(anyhow::Error::msg)?;
    println!(
        "amp parameter table: block {block:#04X}, {} params (address a param by its target)",
        recs.len()
    );
    println!("  target  value  live-index");
    for r in &recs {
        println!(
            "  {:#04X}      {:>3}    {:#04X}",
            r.target, r.value, r.index
        );
    }
    Ok(())
}

#[cfg(not(feature = "alsa"))]
fn scan_amp(_port: Option<&str>) -> Result<()> {
    anyhow::bail!("`scan amp` needs the `alsa` feature and a connected unit")
}

// ---- disk commands (no device) ----

/// Parse a `.tfx` patch file and save it to the on-disk patch library — or, with
/// `json`, print the converted patch as JSON to stdout instead.
pub fn import(tfx: &str, name: Option<&str>, json: bool) -> Result<()> {
    let patch =
        rackctl_eleven_lib::import_tfx(std::path::Path::new(tfx)).map_err(anyhow::Error::msg)?;
    if json {
        let text = rackctl_eleven_lib::patch_to_json(&patch).map_err(anyhow::Error::msg)?;
        println!("{text}");
        return Ok(());
    }
    let save_as = name.unwrap_or(&patch.name);
    let file = rackctl_eleven_lib::save_patch(save_as, &patch).map_err(anyhow::Error::msg)?;
    let active = [
        patch.volume_pedal.bypass,
        patch.wah.bypass,
        patch.distortion.bypass,
        patch.eq.bypass,
        patch.modulation.bypass,
        patch.modulation2.bypass,
        patch.fx_loop.bypass,
        patch.delay.bypass,
        patch.reverb.bypass,
    ]
    .iter()
    .filter(|bypassed| !**bypassed)
    .count();
    println!(
        "imported {:?} ({active} effects active) -> {}",
        patch.name,
        file.display()
    );
    Ok(())
}

/// List patches saved in the on-disk library.
pub fn imports() {
    for name in rackctl_eleven_lib::list_patches() {
        println!("{name}");
    }
}

// ---- parameter catalog (offline; no device) ----

/// A short label for a value kind.
fn kind_label(kind: rackctl_eleven::param::Kind) -> String {
    use rackctl_eleven::param::Kind;
    match kind {
        Kind::Knob => "knob 0-127".to_string(),
        Kind::Switch { off, on } => format!("switch <64={off} >=64={on}"),
        Kind::Stepped(steps) => format!("stepped ({} positions)", steps.len()),
        _ => "?".to_string(),
    }
}

/// Print one amp/global `param::Param` as a catalog row: name, MIDI CC and kind.
fn print_param(p: &rackctl_eleven::param::Param) {
    println!("  {:<22} CC {:>3}   {}", p.name, p.cc, kind_label(p.kind));
}

/// Print one effect `param::FxParam` row: name, per-slot CCs, and kind.
fn print_fx_param(fx: &rackctl_eleven::param::Effect, p: &rackctl_eleven::param::FxParam) {
    let ccs: Vec<String> = fx
        .slots
        .iter()
        .zip(p.cc)
        .map(|(s, c)| {
            if fx.slots.len() == 1 {
                format!("CC {c}")
            } else {
                format!("{}:{c}", s.label())
            }
        })
        .collect();
    println!(
        "  {:<16} {:<26} {}",
        p.name,
        ccs.join(" "),
        kind_label(p.kind)
    );
}

/// List the parameter catalog: amp models and effects (User Guide Ch.11). With an
/// argument, list only the matching amp/effect.
pub fn list(filter: Option<&str>) {
    use rackctl_eleven::param;
    let matches = |name: &str| {
        filter.is_none_or(|f| name.to_ascii_lowercase().contains(&f.to_ascii_lowercase()))
    };

    if filter.is_none() {
        println!("General/Frequently Used Controls");
        for p in param::GENERAL {
            print_param(p);
        }
        println!("\nAmplifier (applies to all amps)");
        for p in param::AMP_GLOBAL {
            print_param(p);
        }
        println!("\nCabinets:    {}", param::CABS.join(", "));
        println!("Microphones: {}", param::MICS.join(", "));
        println!("Mic position: {}", param::MIC_POSITION.join(" / "));
    }

    for amp in param::AMPS {
        if matches(amp.name) {
            println!("\nAmp: {}", amp.name);
            for p in amp.params {
                print_param(p);
            }
        }
    }
    for fx in param::EFFECTS {
        if matches(fx.name) {
            let slots: Vec<&str> = fx.slots.iter().map(|s| s.label()).collect();
            println!("\nEffect: {}   (slots: {})", fx.name, slots.join("/"));
            for p in fx.params {
                print_fx_param(fx, p);
            }
        }
    }
}

/// Show one parameter in detail: which model/effect it belongs to, its MIDI CC,
/// and full value semantics. (The CC is the remote-control number, not a `SysEx`
/// address — the wire address is model/slot-specific; see the protocol doc.)
pub fn info(name: &str) -> Result<()> {
    use rackctl_eleven::param;
    let mut found = false;
    let needle = name.to_ascii_lowercase();
    let hit = |n: &str| n.to_ascii_lowercase() == needle;

    for amp in param::AMPS {
        for p in amp.params {
            if hit(p.name) {
                found = true;
                println!("{} / {}  (MIDI CC {})", amp.name, p.name, p.cc);
                describe_kind(p.kind);
            }
        }
    }
    for fx in param::EFFECTS {
        for p in fx.params {
            if hit(p.name) {
                found = true;
                let ccs = if fx.slots.len() == 1 {
                    p.cc.first().map(u8::to_string).unwrap_or_default()
                } else {
                    fx.slots
                        .iter()
                        .zip(p.cc)
                        .map(|(s, c)| format!("{}={c}", s.label()))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                println!("{} / {}  (MIDI CC {ccs})", fx.name, p.name);
                describe_kind(p.kind);
            }
        }
    }
    if !found {
        anyhow::bail!("no parameter named {name:?}; try `list` to see the catalog");
    }
    Ok(())
}

/// Print the value semantics of a `param::Kind` for `info`.
fn describe_kind(kind: rackctl_eleven::param::Kind) {
    use rackctl_eleven::param::Kind;
    match kind {
        Kind::Knob => println!("  knob, raw 0-127"),
        Kind::Switch { off, on } => println!("  switch: 0-63 = {off}, 64-127 = {on}"),
        Kind::Stepped(steps) => {
            println!("  stepped:");
            for s in steps {
                println!("    {:>3}-{:<3}  {}", s.lo, s.hi, s.label);
            }
        }
        _ => println!("  (unknown kind)"),
    }
}

// ---- hardware-only commands ----

/// Select a patch (Program Change), from the User or Factory bank.
#[cfg(feature = "alsa")]
pub fn select(port: Option<&str>, slot: u8, factory: bool) -> Result<()> {
    use rackctl_eleven_lib::manage::{FACTORY_BANK, USER_BANK};
    let bank = if factory { FACTORY_BANK } else { USER_BANK };
    open_rawmidi(port)?.select_rig(bank, slot)?;
    let label = if factory { "Factory" } else { "User" };
    println!("selected {label} slot {slot}");
    Ok(())
}

/// Parse a chain-slot name (`mod`/`fx1`/`fx2`) into a `param::Slot`.
#[cfg(feature = "alsa")]
fn parse_slot(s: &str) -> Result<rackctl_eleven::param::Slot> {
    use rackctl_eleven::param::Slot;
    match s.to_ascii_lowercase().as_str() {
        "mod" => Ok(Slot::Mod),
        "fx1" => Ok(Slot::Fx1),
        "fx2" => Ok(Slot::Fx2),
        other => anyhow::bail!("unknown slot {other:?} (expected mod, fx1, or fx2)"),
    }
}

/// Parse a CC value: a number `0..=127`, or `on`/`off` for a switch (127 / 0).
#[cfg(feature = "alsa")]
fn parse_cc_value(s: &str) -> Result<u8> {
    match s.to_ascii_lowercase().as_str() {
        "on" => Ok(127),
        "off" => Ok(0),
        _ => {
            let v: u16 = s.parse().with_context(|| format!("bad value {s:?}"))?;
            if v > 127 {
                anyhow::bail!("value {v} out of range (0-127, or on/off)");
            }
            Ok(u8::try_from(v).unwrap_or(0))
        }
    }
}

/// Move a named parameter over MIDI CC (the native remote-control path). Resolves
/// the CC from the catalog, using `--amp`/`--fx`/`--slot` to disambiguate.
#[cfg(feature = "alsa")]
pub fn cc(
    port: Option<&str>,
    name: &str,
    value: &str,
    amp: Option<&str>,
    fx: Option<&str>,
    slot: Option<&str>,
    channel: u8,
) -> Result<()> {
    let v = parse_cc_value(value)?;
    let slot = slot.map(parse_slot).transpose()?;
    let mut dev = open_rawmidi(port)?;
    let (cc_num, kind) =
        rackctl_eleven_lib::manage::send_named_cc(&mut dev, name, v, amp, fx, slot, channel)
            .map_err(anyhow::Error::msg)?;
    println!(
        "sent CC {cc_num} = {v} (ch {channel}) for {name:?}  [{}]",
        kind_summary(kind)
    );
    Ok(())
}

/// A one-word summary of a value kind, for the `cc` confirmation line.
#[cfg(feature = "alsa")]
fn kind_summary(kind: rackctl_eleven::param::Kind) -> &'static str {
    use rackctl_eleven::param::Kind;
    match kind {
        Kind::Knob => "knob",
        Kind::Switch { .. } => "switch",
        Kind::Stepped(_) => "stepped",
        _ => "?",
    }
}

/// List the on-device bank's patch names from the directory (block `0x04`).
#[cfg(feature = "alsa")]
pub fn patches(port: Option<&str>, count: u8, factory: bool) -> Result<()> {
    use rackctl_eleven_lib::manage::{self, FACTORY_BANK, USER_BANK};
    let bank = if factory { FACTORY_BANK } else { USER_BANK };
    let mut dev = open_rawmidi(port)?;
    for (slot, name) in
        manage::patch_directory(&mut dev, bank, count).map_err(anyhow::Error::msg)?
    {
        println!("{slot:3}  {name}");
    }
    Ok(())
}

/// Show the current patch (or a slot): its name and the size of its packed data.
#[cfg(feature = "alsa")]
pub fn dump(port: Option<&str>, slot: Option<u8>) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    if let Some(s) = slot {
        dev.select_rig(0, s)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let name = trailing_name(&dev.read_block(&[0x05])?);
    let blob = dev.read_block(&[0x01])?;
    let where_ = slot.map_or_else(|| "current sound".to_owned(), |s| format!("User slot {s}"));
    println!(
        "{where_}: {name:?}  ({} bytes of packed patch data)",
        blob.len()
    );
    Ok(())
}

/// Save the current sound (or a slot) to the library as `name`.
#[cfg(feature = "alsa")]
pub fn save(port: Option<&str>, name: &str, slot: Option<u8>) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let patch = rackctl_eleven_lib::manage::capture_to_library(&mut dev, name, slot)
        .map_err(anyhow::Error::msg)?;
    println!(
        "saved {:?} ({} blocks) to the library as {name:?}",
        patch.name,
        patch.blocks.len()
    );
    Ok(())
}

/// Load a saved patch from the library onto User `slot`, verifying.
#[cfg(feature = "alsa")]
pub fn load(port: Option<&str>, name: &str, slot: u8) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let (patch, report) = rackctl_eleven_lib::manage::restore_from_library(&mut dev, name, slot)
        .map_err(anyhow::Error::msg)?;
    println!("loaded {:?} onto User slot {slot}: {report}", patch.name);
    if !report.ok() {
        anyhow::bail!("load verify failed: {report}");
    }
    Ok(())
}

/// Copy a patch from one slot to a User slot (e.g. a Factory preset), verifying.
#[cfg(feature = "alsa")]
pub fn copy(port: Option<&str>, from: u8, to: u8, factory: bool) -> Result<()> {
    use rackctl_eleven_lib::manage::{self, FACTORY_BANK, USER_BANK};
    let bank = if factory { FACTORY_BANK } else { USER_BANK };
    let src = if factory { "Factory" } else { "User" };
    let mut dev = open_rawmidi(port)?;
    let report = manage::copy_slot(&mut dev, bank, from, to).map_err(anyhow::Error::msg)?;
    println!("copied {src} slot {from} -> User slot {to}: {report}");
    if !report.ok() {
        anyhow::bail!("copy verify failed: {report}");
    }
    Ok(())
}

/// Back up the whole User bank to the library (one saved patch per slot).
#[cfg(feature = "alsa")]
pub fn backup(port: Option<&str>, count: u8) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let n = rackctl_eleven_lib::manage::backup_bank(&mut dev, count, |slot, name| {
        println!("U{slot:03}: {name:?}");
    })
    .map_err(anyhow::Error::msg)?;
    println!("backed up {n} User patches to the library");
    Ok(())
}

/// List the saved patches in the library.
pub fn library() {
    for name in rackctl_eleven_lib::list_backups() {
        println!("{name}");
    }
}

/// Store the current edit buffer to a User slot, with a name.
#[cfg(feature = "alsa")]
pub fn store(port: Option<&str>, slot: u8, name: &str) -> Result<()> {
    open_rawmidi(port)?.store(u16::from(slot), name)?;
    println!("stored the current edit buffer to User slot {slot} as {name:?}");
    Ok(())
}

/// Rename a User slot, preserving its patch data (select it, then store it back).
#[cfg(feature = "alsa")]
pub fn rename(port: Option<&str>, slot: u8, name: &str) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    dev.select_rig(0, slot)?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    dev.store(u16::from(slot), name)?;
    println!("renamed User slot {slot} to {name:?}");
    Ok(())
}

/// Capture the whole User bank into a named scene.
#[cfg(feature = "alsa")]
pub fn scene_save(port: Option<&str>, name: &str, count: u8) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let scene = rackctl_eleven_lib::manage::capture_scene(&mut dev, name, count, |slot, n| {
        println!("U{slot:03}: {n:?}");
    })
    .map_err(anyhow::Error::msg)?;
    rackctl_eleven_lib::save_scene(&scene).map_err(anyhow::Error::msg)?;
    println!("saved scene {name:?} ({} patches)", scene.patches.len());
    Ok(())
}

/// Restore a saved scene to the device.
#[cfg(feature = "alsa")]
pub fn scene_restore(port: Option<&str>, name: &str) -> Result<()> {
    let scene = rackctl_eleven_lib::load_scene(name).map_err(anyhow::Error::msg)?;
    let mut dev = open_rawmidi(port)?;
    let report = rackctl_eleven_lib::manage::restore_scene(&mut dev, &scene, |slot, n| {
        println!("U{slot:03}: {n:?}");
    })
    .map_err(anyhow::Error::msg)?;
    println!("restored scene {name:?}: {report}");
    if !report.ok() {
        anyhow::bail!("scene restore verify failed: {report}");
    }
    Ok(())
}

/// List the saved scenes.
pub fn scene_list() {
    for name in rackctl_eleven_lib::list_scenes() {
        println!("{name}");
    }
}

/// Stream the unit's change reports until interrupted.
#[cfg(feature = "alsa")]
pub fn monitor(port: Option<&str>) -> Result<()> {
    let port = port.context("monitor needs --port (a connected unit)")?;
    let mut dev = RawMidi::open(port)?;
    eprintln!("listening on {port}; turn a knob (Ctrl-C to stop)");
    dev.monitor()?;
    Ok(())
}

/// Probe and print the unit's identity.
#[cfg(feature = "alsa")]
pub fn identity(port: Option<&str>) -> Result<()> {
    let id = open_rawmidi(port)?.identity()?;
    println!(
        "device id {:#04x}  manufacturer {:#04x}  family {:#06x}  model {:#06x}  version {:?}",
        id.device_id, id.manufacturer, id.family, id.model, id.version
    );
    Ok(())
}

/// List the available ALSA rawmidi ports.
#[cfg(feature = "alsa")]
pub fn ports() -> Result<()> {
    for p in RawMidi::ports()? {
        println!("{p}");
    }
    Ok(())
}

// ---- `alsa`-less stubs ----

#[cfg(not(feature = "alsa"))]
macro_rules! no_alsa {
    ($($name:ident($($arg:ident : $ty:ty),*));* $(;)?) => {$(
        pub fn $name($(_: $ty),*) -> Result<()> {
            anyhow::bail!("built without the `alsa` feature; this command needs hardware")
        }
    )*};
}
#[cfg(not(feature = "alsa"))]
no_alsa! {
    select(port: Option<&str>, slot: u8, factory: bool);
    cc(port: Option<&str>, name: &str, value: &str, amp: Option<&str>, fx: Option<&str>, slot: Option<&str>, channel: u8);
    patches(port: Option<&str>, count: u8, factory: bool);
    dump(port: Option<&str>, slot: Option<u8>);
    save(port: Option<&str>, name: &str, slot: Option<u8>);
    load(port: Option<&str>, name: &str, slot: u8);
    copy(port: Option<&str>, from: u8, to: u8, factory: bool);
    backup(port: Option<&str>, count: u8);
    store(port: Option<&str>, slot: u8, name: &str);
    rename(port: Option<&str>, slot: u8, name: &str);
    scene_save(port: Option<&str>, name: &str, count: u8);
    scene_restore(port: Option<&str>, name: &str);
    monitor(port: Option<&str>);
    identity(port: Option<&str>);
    ports();
}

// ---- helpers ----

/// Render bytes as space-separated uppercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a single hex byte (optional `0x`).
fn parse_byte(s: &str) -> Result<u8> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    u8::from_str_radix(h, 16).with_context(|| format!("invalid hex byte {s:?}"))
}

/// Parse `"11 21 0D"` (or comma-separated, with optional `0x`) into address bytes.
fn parse_addr(s: &str) -> Result<Vec<u8>> {
    let bytes: Result<Vec<u8>> = s
        .split([' ', ','])
        .filter(|t| !t.is_empty())
        .map(|t| {
            let h = t.strip_prefix("0x").unwrap_or(t);
            u8::from_str_radix(h, 16).with_context(|| format!("invalid hex byte {t:?}"))
        })
        .collect();
    let bytes = bytes?;
    if bytes.is_empty() {
        anyhow::bail!("empty address");
    }
    Ok(bytes)
}

/// The trailing run of printable ASCII in `payload` (a patch name, after any flag
/// byte and before the NUL terminator).
#[cfg(feature = "alsa")]
fn trailing_name(payload: &[u8]) -> String {
    let mut run = Vec::new();
    for &b in payload.iter().rev() {
        if (0x20..0x7f).contains(&b) {
            run.push(b);
        } else if !run.is_empty() {
            break;
        }
    }
    run.reverse();
    String::from_utf8_lossy(&run).into_owned()
}
