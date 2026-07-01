//! `rackctl-eleven` — command-line control for the Avid/Digidesign Eleven Rack.
//!
//! Mirrors the `rackctl-gx700` CLI: global `--mock`/`--port`, a thin `commands`
//! module that dispatches to the shared `rackctl-eleven-lib` management layer (so a
//! GUI reuses the same implementations), and matching terminology — `save` /
//! `load` (a captured sound to/from the library), `backup` (the whole User bank),
//! `scene save/restore/list` (a whole-bank snapshot), `copy` (slot to slot),
//! `patches` (the on-device bank). Parameter-level `get`/`set`/`scan` run on the
//! mock or hardware; `list`/`info` browse the catalog offline.
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
    /// Select a patch (Program Change); Factory bank with `--factory`.
    Select {
        /// Slot number (0-based).
        slot: u8,
        /// Select from the Factory bank instead of User.
        #[arg(long)]
        factory: bool,
    },
    /// Move a named parameter over MIDI CC (the native remote-control path).
    Cc {
        /// Control name (kebab-case), e.g. `dist-bypass`, `presence`, `rate`.
        name: String,
        /// Value 0-127, or `on`/`off` for a switch.
        value: String,
        /// Amp model, to disambiguate an amp parameter (e.g. `--amp tweed-bass`).
        #[arg(long)]
        amp: Option<String>,
        /// Effect name, to disambiguate an effect parameter (e.g. `--fx parametric-eq`).
        #[arg(long)]
        fx: Option<String>,
        /// Chain slot for an effect parameter: `mod`, `fx1`, or `fx2` (default: its first).
        #[arg(long)]
        slot: Option<String>,
        /// MIDI channel (1-16).
        #[arg(long, default_value_t = 1)]
        channel: u8,
    },
    /// List the on-device bank's patch names (User, or Factory with `--factory`).
    Patches {
        /// How many slots to read.
        #[arg(long, default_value_t = 128)]
        count: u8,
        /// List the Factory bank instead of User.
        #[arg(long)]
        factory: bool,
    },
    /// Show the current sound (or a slot): its name and packed size.
    Dump {
        /// Show User slot N instead of the current sound.
        #[arg(long)]
        slot: Option<u8>,
    },
    /// Save the current sound (or a slot) to the library as `name`.
    Save {
        /// Name to save under, in the library.
        name: String,
        /// Save User slot N instead of the current sound.
        #[arg(long)]
        slot: Option<u8>,
    },
    /// Load a saved patch from the library onto a User slot (verified).
    Load {
        /// Saved patch name (see `library`).
        name: String,
        /// Target User slot number (0-based).
        #[arg(long)]
        slot: u8,
    },
    /// Copy a patch from one slot to a User slot (e.g. a Factory preset).
    Copy {
        /// Source slot number (0-based).
        from: u8,
        /// Destination User slot number (0-based).
        #[arg(long)]
        to: u8,
        /// Take the source from the Factory bank.
        #[arg(long)]
        factory: bool,
    },
    /// Back up the whole User bank to the library (one saved patch per slot).
    Backup {
        /// Number of User slots to read.
        #[arg(long, default_value_t = 128)]
        count: u8,
    },
    /// List the saved patches in the library.
    Library,
    /// Store the current edit buffer to a User slot, with a name (persists).
    Store {
        /// User slot number (0-based).
        slot: u8,
        /// Name for the stored patch.
        name: String,
    },
    /// Rename a User slot, preserving its patch data.
    Rename {
        /// User slot number (0-based).
        slot: u8,
        /// New name.
        name: String,
    },
    /// Whole-bank scenes: capture / restore / list the User bank as one file.
    Scene {
        #[command(subcommand)]
        cmd: SceneCommand,
    },
    /// Import a `.tfx` patch file into the on-disk patch library.
    Import {
        /// Path to the `.tfx` file.
        file: String,
        /// Save under this name (default: the patch's own name).
        #[arg(long)]
        name: Option<String>,
    },
    /// List the `.tfx` patches saved in the on-disk library.
    Imports,
    /// List the parameter catalog (amp models and effects); optionally filtered.
    List {
        /// Only show amps/effects whose name contains this text.
        filter: Option<String>,
    },
    /// Show one parameter's MIDI CC(s) and value semantics.
    Info {
        /// Parameter name, e.g. `presence` or `decay`.
        name: String,
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

/// `scene` subcommands (a whole User bank as one library file).
#[derive(Subcommand)]
enum SceneCommand {
    /// Capture the whole User bank into a named scene.
    Save {
        /// Name to save the scene under.
        name: String,
        /// Number of User slots to read.
        #[arg(long, default_value_t = 128)]
        count: u8,
    },
    /// Restore a saved scene to the device (overwrites each captured slot).
    Restore {
        /// Scene name to restore.
        name: String,
    },
    /// List the saved scenes.
    List,
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
        Command::Select { slot, factory } => commands::select(port, slot, factory),
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
        Command::Patches { count, factory } => commands::patches(port, count, factory),
        Command::Dump { slot } => commands::dump(port, slot),
        Command::Save { name, slot } => commands::save(port, &name, slot),
        Command::Load { name, slot } => commands::load(port, &name, slot),
        Command::Copy { from, to, factory } => commands::copy(port, from, to, factory),
        Command::Backup { count } => commands::backup(port, count),
        Command::Library => {
            commands::library();
            Ok(())
        }
        Command::Store { slot, name } => commands::store(port, slot, &name),
        Command::Rename { slot, name } => commands::rename(port, slot, &name),
        Command::Scene { cmd } => match cmd {
            SceneCommand::Save { name, count } => commands::scene_save(port, &name, count),
            SceneCommand::Restore { name } => commands::scene_restore(port, &name),
            SceneCommand::List => {
                commands::scene_list();
                Ok(())
            }
        },
        Command::Import { file, name } => commands::import(&file, name.as_deref()),
        Command::Imports => {
            commands::imports();
            Ok(())
        }
        Command::List { filter } => {
            commands::list(filter.as_deref());
            Ok(())
        }
        Command::Info { name } => commands::info(&name),
        Command::Monitor => commands::monitor(port),
        Command::Identity => commands::identity(port),
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "rackctl-eleven", &mut std::io::stdout());
            Ok(())
        }
    }
}
