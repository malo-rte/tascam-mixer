//! `rackctl-eleven` — command-line control for the Avid/Digidesign Eleven Rack.
//!
//! Mirrors the `rackctl-gx700` CLI structure: global `--mock`/`--port`, a
//! `commands` module for the implementations, and a `completions` command.
//! Parameter-level commands (`get`/`set`/`scan`) run on the mock or hardware; the
//! patch/slot commands need a connected unit (`--port`).
//!
//! `list`/`info` browse the parameter catalog (offline, no device). The remaining
//! GX-700 commands without an Eleven Rack equivalent yet are `load`/`copy`, which
//! await library restore.
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
    /// Move a named parameter over MIDI CC (the native remote-control path).
    Cc {
        /// Control name, e.g. `"dist bypass"`, `presence`, `rate`.
        name: String,
        /// Value 0-127, or `on`/`off` for a switch.
        value: String,
        /// Amp model, to disambiguate an amp parameter (e.g. `--amp "Tweed Bass"`).
        #[arg(long)]
        amp: Option<String>,
        /// Effect name, to disambiguate an effect parameter (e.g. `--fx "Parametric EQ"`).
        #[arg(long)]
        fx: Option<String>,
        /// Chain slot for an effect parameter: `mod`, `fx1`, or `fx2` (default: its first).
        #[arg(long)]
        slot: Option<String>,
        /// MIDI channel (1-16).
        #[arg(long, default_value_t = 1)]
        channel: u8,
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
    /// Capture the current sound (or a slot) to the on-disk backup library.
    Capture {
        /// Name to save the backup under.
        name: String,
        /// Capture User slot N instead of the current sound.
        #[arg(long)]
        slot: Option<u8>,
    },
    /// Restore a saved backup into a User slot (writes the edit buffer, then stores).
    Restore {
        /// Name of the saved backup (see `backups`).
        name: String,
        /// Target User slot number (0-based).
        #[arg(long)]
        slot: u8,
    },
    /// List the saved patch backups.
    Backups,
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
    /// List the parameter catalog (amp models and effects); optionally filtered.
    List {
        /// Only show amps/effects whose name contains this text.
        filter: Option<String>,
    },
    /// Show one parameter's CC/index, wire address and value semantics.
    Info {
        /// Parameter name, e.g. `presence` or `decay`.
        name: String,
    },
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
        Command::Cc {
            name,
            value,
            amp,
            fx,
            slot,
            channel,
        } => commands::cc(
            port,
            &name,
            &value,
            amp.as_deref(),
            fx.as_deref(),
            slot.as_deref(),
            channel,
        ),
        Command::Patches { count } => commands::patches(port, count),
        Command::Dump { slot } => commands::dump(port, slot),
        Command::Save { name, slot } => commands::save(port, &name, slot),
        Command::Store { slot, name } => commands::store(port, slot, &name),
        Command::Capture { name, slot } => commands::capture(port, &name, slot),
        Command::Restore { name, slot } => commands::restore(port, &name, slot),
        Command::Backups => {
            commands::backups();
            Ok(())
        }
        Command::Rename { slot, name } => commands::rename(port, slot, &name),
        Command::Import { file, name } => commands::import(&file, name.as_deref()),
        Command::Rigs => {
            commands::rigs();
            Ok(())
        }
        Command::List { filter } => {
            commands::list(filter.as_deref());
            Ok(())
        }
        Command::Info { name } => commands::info(&name),
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
