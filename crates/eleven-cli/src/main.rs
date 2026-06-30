//! `rackctl-eleven` — command-line control for the Avid/Digidesign Eleven Rack.
//!
//! Mirrors the `rackctl-gx700` CLI structure: global `--mock`/`--port`, a
//! `commands` module for the implementations, and a `completions` command.
//! Parameter-level commands (`get`/`set`/`scan`) run on the mock or hardware; the
//! patch/slot commands need a connected unit (`--port`).
//!
//! Some GX-700 commands have no Eleven Rack equivalent yet: `list`/`info` await
//! the named parameter catalog, and `load`/`copy` await library restore.
#![forbid(unsafe_code)]

mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

/// Control the Avid/Digidesign Eleven Rack over MIDI System Exclusive.
#[derive(Parser)]
#[command(name = "rackctl-eleven", version, about)]
struct Cli {
    /// Use an in-memory mock device instead of real hardware.
    #[arg(long, global = true)]
    mock: bool,

    /// ALSA rawmidi port (`hw:CARD,DEV`) of the "Eleven Rack Rig" port; see `ports`.
    #[arg(long, global = true)]
    port: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the available ALSA rawmidi ports.
    Ports,
    /// Read a parameter's value at a hex address, e.g. `get "11 21 0D"`.
    Get {
        /// Address bytes in hex, space- or comma-separated.
        addr: String,
    },
    /// Write a knob value byte at a hex address, then read it back to verify.
    Set {
        /// Address bytes in hex.
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
    /// Select a User patch slot (Program Change).
    Select {
        /// User slot number (0-based).
        slot: u8,
    },
    /// List the unit's patch names from the on-device directory.
    Patches {
        /// How many slots to read.
        #[arg(long, default_value_t = 128)]
        count: u8,
    },
    /// Show the current patch (or a slot): its name and packed size.
    Dump {
        /// Show User slot N instead of the current sound.
        #[arg(long)]
        slot: Option<u8>,
    },
    /// Save the current sound (or a slot) to a disk file (`<name>.erpatch`).
    Save {
        /// File name to save under (`.erpatch` is appended).
        name: String,
        /// Save User slot N instead of the current sound.
        #[arg(long)]
        slot: Option<u8>,
    },
    /// Store the current edit buffer to a User slot, with a name (persists).
    Store {
        /// User slot number (0-based).
        slot: u8,
        /// Name for the stored rig.
        name: String,
    },
    /// Rename a User slot, preserving its patch data.
    Rename {
        /// User slot number (0-based).
        slot: u8,
        /// New name.
        name: String,
    },
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
    /// Back up the unit's whole patch library to a directory.
    Backup {
        /// Output directory (created if missing).
        #[arg(long, default_value = "eleven-backup")]
        out: String,
        /// Number of patches to read per bank (User, then Factory).
        #[arg(long, default_value_t = 100)]
        count: u8,
    },
    /// Stream the unit's change reports as you turn knobs.
    Monitor,
    /// Probe and print the unit's identity.
    Identity,
    /// Generate a shell completion script.
    Completions {
        /// Shell to generate the completion script for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mock = cli.mock;
    let port = cli.port.as_deref();
    match cli.command {
        Command::Ports => commands::ports(),
        Command::Get { addr } => commands::get(mock, port, &addr),
        Command::Set { addr, value } => commands::set(mock, port, &addr, &value),
        Command::Scan { prefix, from, to } => commands::scan(mock, port, &prefix, &from, &to),
        Command::Select { slot } => commands::select(port, slot),
        Command::Patches { count } => commands::patches(port, count),
        Command::Dump { slot } => commands::dump(port, slot),
        Command::Save { name, slot } => commands::save(port, &name, slot),
        Command::Store { slot, name } => commands::store(port, slot, &name),
        Command::Rename { slot, name } => commands::rename(port, slot, &name),
        Command::Import { file, name } => commands::import(&file, name.as_deref()),
        Command::Rigs => {
            commands::rigs();
            Ok(())
        }
        Command::Backup { out, count } => commands::backup(port, &out, count),
        Command::Monitor => commands::monitor(port),
        Command::Identity => commands::identity(port),
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "rackctl-eleven", &mut std::io::stdout());
            Ok(())
        }
    }
}
