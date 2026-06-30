//! `rackctl-eleven` — command-line control for the Avid/Digidesign Eleven Rack.
//!
//! It reads parameters by raw address (`read`/`scan`), streams change reports
//! (`monitor`), probes the unit (`identity`/`ports`), and imports `.tfx` rig files
//! into the on-disk rig library (`import`/`rigs`). See `docs/eleven-rack-roadmap.adoc`.
#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[cfg(feature = "alsa")]
use rackctl_eleven::RawMidi;
use rackctl_eleven::{Eleven, MockTransport, Transport};

/// Control the Avid/Digidesign Eleven Rack over MIDI System Exclusive.
#[derive(Parser)]
#[command(name = "rackctl-eleven", version, about)]
struct Cli {
    /// ALSA rawmidi port (`hw:CARD,DEV`) of the "Eleven Rack Rig" port. Omit to
    /// use the in-memory mock (no hardware needed).
    #[arg(long, global = true)]
    port: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read the parameter at a hex address, e.g. `read "11 21 0D"`.
    Read {
        /// Address bytes in hex, space- or comma-separated.
        addr: String,
    },
    /// Write a value byte at a hex address, then read it back to verify.
    Set {
        /// Address bytes in hex, e.g. `"11 21 07"`.
        addr: String,
        /// New value byte `b0` in hex (0..7F for a knob).
        value: String,
    },
    /// Scan a block: read `<prefix> 00`..`<prefix> 7F` and print the answers.
    Scan {
        /// Leading address bytes in hex, e.g. `"11 21"`.
        prefix: String,
        /// First value of the trailing byte (hex).
        #[arg(long, default_value = "00")]
        from: String,
        /// Last value of the trailing byte (hex).
        #[arg(long, default_value = "7f")]
        to: String,
    },
    /// Stream the unit's change reports as you turn knobs (needs `--port`).
    Monitor,
    /// Probe and print the unit's identity (needs `--port`).
    Identity,
    /// Import a `.tfx` rig file into the on-disk rig library.
    Import {
        /// Path to the `.tfx` file.
        file: String,
        /// Save under this name (default: the rig's own name).
        #[arg(long)]
        name: Option<String>,
    },
    /// List the rigs saved in the on-disk library.
    Rigs,
    /// List the available ALSA rawmidi ports.
    Ports,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Read { addr } => read(cli.port.as_deref(), addr),
        Command::Set { addr, value } => set(cli.port.as_deref(), addr, value),
        Command::Scan { prefix, from, to } => scan(cli.port.as_deref(), prefix, from, to),
        Command::Monitor => monitor(cli.port.as_deref()),
        Command::Identity => identity(cli.port.as_deref()),
        Command::Import { file, name } => import(file, name.as_deref()),
        Command::Rigs => {
            rigs();
            Ok(())
        }
        Command::Ports => list_ports(),
    }
}

/// Parse a `.tfx` rig file and save it to the on-disk rig library.
fn import(file: &str, name: Option<&str>) -> Result<()> {
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
fn rigs() {
    for name in rackctl_eleven_lib::list_rigs() {
        println!("{name}");
    }
}

/// Read one parameter and print its raw bytes and decoded word.
fn read(port: Option<&str>, addr: &str) -> Result<()> {
    let bytes = parse_addr(addr)?;
    let mut dev = open_device(port)?;
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
fn set(port: Option<&str>, addr: &str, value: &str) -> Result<()> {
    let bytes = parse_addr(addr)?;
    let b0 = parse_byte(value)?;
    let mut dev = open_device(port)?;
    // Knob-parameter value form: b0 in the low byte, with the 0x10 type tag.
    let want = rackctl_eleven::RawValue::from_bytes([b0, 0, 0, 0, 0x10]);
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
fn scan(port: Option<&str>, prefix: &str, from: &str, to: &str) -> Result<()> {
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

    let mut dev = open_device(port)?;
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

/// Stream change reports until interrupted (hardware only).
#[cfg(feature = "alsa")]
fn monitor(port: Option<&str>) -> Result<()> {
    let port = port.context("monitor needs --port (a connected unit)")?;
    let mut dev = RawMidi::open(port)?;
    eprintln!("listening on {port}; turn a knob (Ctrl-C to stop)");
    dev.monitor()?;
    Ok(())
}

#[cfg(not(feature = "alsa"))]
fn monitor(_port: Option<&str>) -> Result<()> {
    anyhow::bail!("built without the `alsa` feature; cannot monitor hardware")
}

/// Probe and print the unit's identity (hardware only).
#[cfg(feature = "alsa")]
fn identity(port: Option<&str>) -> Result<()> {
    let port = port.context("identity needs --port (a connected unit)")?;
    let id = RawMidi::open(port)?.identity()?;
    println!(
        "device id {:#04x}  manufacturer {:#04x}  family {:#06x}  model {:#06x}  version {:?}",
        id.device_id, id.manufacturer, id.family, id.model, id.version
    );
    Ok(())
}

#[cfg(not(feature = "alsa"))]
fn identity(_port: Option<&str>) -> Result<()> {
    anyhow::bail!("built without the `alsa` feature; cannot probe hardware")
}

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

/// Open the device: the mock when `port` is `None`, else the hardware port.
fn open_device(port: Option<&str>) -> Result<Eleven<Box<dyn Transport>>> {
    match port {
        Some(p) => open_hardware(p),
        None => Ok(Eleven::new(Box::new(MockTransport::new()))),
    }
}

#[cfg(feature = "alsa")]
fn open_hardware(port: &str) -> Result<Eleven<Box<dyn Transport>>> {
    Ok(Eleven::new(Box::new(RawMidi::open(port)?)))
}

#[cfg(not(feature = "alsa"))]
fn open_hardware(_port: &str) -> Result<Eleven<Box<dyn Transport>>> {
    anyhow::bail!("built without the `alsa` feature; only the in-memory mock is available")
}

#[cfg(feature = "alsa")]
fn list_ports() -> Result<()> {
    for p in RawMidi::ports()? {
        println!("{p}");
    }
    Ok(())
}

#[cfg(not(feature = "alsa"))]
fn list_ports() -> Result<()> {
    anyhow::bail!("built without the `alsa` feature; no rawmidi ports available")
}
