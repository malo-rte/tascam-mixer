//! Implementations of the `rackctl-eleven` subcommands.
//!
//! Mirrors `rackctl-gx700`'s split: the CLI definition and dispatch live in
//! `main.rs`, the work lives here. Parameter-level commands (`get`/`set`/`scan`)
//! run on the mock or real hardware; the patch/slot commands need a connected
//! unit (`--port`).

use anyhow::{Context, Result};

#[cfg(feature = "alsa")]
use rackctl_eleven::RawMidi;
use rackctl_eleven::{Eleven, MockTransport, RawValue, Transport};

// ---- device opening ----

/// Open the device for parameter-level commands: the mock (`--mock`) or the
/// hardware port (`--port`).
fn open_device(mock: bool, port: Option<&str>) -> Result<Eleven<Box<dyn Transport>>> {
    if mock {
        return Ok(Eleven::new(Box::new(MockTransport::new())));
    }
    #[cfg(feature = "alsa")]
    {
        let port = port.context("no --port given (run `ports`, or use --mock)")?;
        Ok(Eleven::new(Box::new(RawMidi::open(port)?)))
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
    Ok(RawMidi::open(port)?)
}

// ---- parameter commands (mock or hardware) ----

/// Read one parameter and print its raw bytes and decoded word.
pub fn get(mock: bool, port: Option<&str>, addr: &str) -> Result<()> {
    let bytes = parse_addr(addr)?;
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

/// Scan `<prefix> from`..`<prefix> to`, printing each address that answered.
pub fn scan(mock: bool, port: Option<&str>, prefix: &str, from: &str, to: &str) -> Result<()> {
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

// ---- disk commands (no device) ----

/// Parse a `.tfx` rig file and save it to the on-disk rig library.
pub fn import(file: &str, name: Option<&str>) -> Result<()> {
    let rig =
        rackctl_eleven_lib::import_tfx(std::path::Path::new(file)).map_err(anyhow::Error::msg)?;
    let save_as = name.unwrap_or(&rig.name);
    let path = rackctl_eleven_lib::save_rig(save_as, &rig).map_err(anyhow::Error::msg)?;
    println!(
        "imported {:?} ({} blocks) -> {}",
        rig.name,
        rig.blocks.len(),
        path.display()
    );
    Ok(())
}

/// List rigs saved in the on-disk library.
pub fn rigs() {
    for name in rackctl_eleven_lib::list_rigs() {
        println!("{name}");
    }
}

// ---- hardware-only commands ----

/// Select a User patch slot (Program Change).
#[cfg(feature = "alsa")]
pub fn select(port: Option<&str>, slot: u8) -> Result<()> {
    open_rawmidi(port)?.select_rig(0, slot)?;
    println!("selected User slot {slot}");
    Ok(())
}

/// List the unit's patch names from the on-device directory (block `0x04`).
#[cfg(feature = "alsa")]
pub fn patches(port: Option<&str>, count: u8) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    for slot in 0..count {
        let hi = (slot >> 7) & 0x7f;
        let lo = slot & 0x7f;
        match dev.read_block(&[0x04, hi, lo]) {
            Ok(payload) => println!("{slot:3}  {}", trailing_name(&payload)),
            Err(_) => println!("{slot:3}  (no reply)"),
        }
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

/// Save the current sound (or a slot) to a disk file (the raw packed patch).
#[cfg(feature = "alsa")]
pub fn save(port: Option<&str>, name: &str, slot: Option<u8>) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    if let Some(s) = slot {
        dev.select_rig(0, s)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let blob = dev.read_block(&[0x01])?;
    let file = std::path::PathBuf::from(format!("{}.erpatch", sanitize(name)));
    std::fs::write(&file, &blob).with_context(|| format!("write {}", file.display()))?;
    println!("saved {} bytes -> {}", blob.len(), file.display());
    Ok(())
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

/// Back up the unit's patch library: each patch's packed block (`0x01`) + name.
#[cfg(feature = "alsa")]
pub fn backup(port: Option<&str>, out: &str, count: u8) -> Result<()> {
    let mut dev = open_rawmidi(port)?;
    let dir = std::path::Path::new(out);
    std::fs::create_dir_all(dir).with_context(|| format!("create {out}"))?;
    let mut total = 0u32;
    for (bank, label) in [(0u8, "user"), (1u8, "factory")] {
        let mut first: Option<String> = None;
        for pc in 0..count {
            dev.select_rig(bank, pc)?;
            std::thread::sleep(std::time::Duration::from_millis(60));
            let blob = read_block_retry(&mut dev, &[0x01])?;
            let name = trailing_name(&read_block_retry(&mut dev, &[0x05])?);
            if pc > 0 && first.as_deref() == Some(name.as_str()) {
                println!("{label}: wrapped at {pc} ({pc} patches)");
                break;
            }
            first.get_or_insert_with(|| name.clone());
            let file = dir.join(format!("{label}-{pc:03}-{}.erpatch", sanitize(&name)));
            std::fs::write(&file, &blob).with_context(|| format!("write {}", file.display()))?;
            println!("{label} {pc:3}: {name:?} ({} bytes)", blob.len());
            total += 1;
        }
    }
    println!("backed up {total} patches to {out}");
    Ok(())
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

/// Read a block, retrying a few times — the unit occasionally misses a reply.
#[cfg(feature = "alsa")]
fn read_block_retry(dev: &mut RawMidi, addr: &[u8]) -> Result<Vec<u8>> {
    let mut last = None;
    for _ in 0..3 {
        match dev.read_block(addr) {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
    }
    Err(last.map_or_else(|| anyhow::anyhow!("read failed"), Into::into))
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
    select(port: Option<&str>, slot: u8);
    patches(port: Option<&str>, count: u8);
    dump(port: Option<&str>, slot: Option<u8>);
    save(port: Option<&str>, name: &str, slot: Option<u8>);
    store(port: Option<&str>, slot: u8, name: &str);
    rename(port: Option<&str>, slot: u8, name: &str);
    backup(port: Option<&str>, out: &str, count: u8);
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

/// The trailing run of printable ASCII in `payload` (a rig name, after any flag
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

/// Make a name safe for a filename (keep alphanumerics, space, dash, underscore).
#[cfg(feature = "alsa")]
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}
