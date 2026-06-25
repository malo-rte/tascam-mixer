//! `rackctl-gx700` — command-line control for the BOSS GX-700 FX processor.

mod commands;
mod config;
mod value;

use std::process::ExitCode;

use anyhow::Result;
use clap::builder::styling::{AnsiColor, Styles};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use rackctl_gx700::{Gx700, MockTransport, Transport};

#[cfg(feature = "alsa")]
use rackctl_gx700::RawMidi;

#[derive(Parser)]
#[command(
    name = "rackctl-gx700",
    version,
    about = "Control the BOSS GX-700 guitar effects processor over MIDI",
    long_about = "\
rackctl-gx700 reads and writes the BOSS GX-700's effect-block parameters over \
MIDI System Exclusive, and selects patch memories with Program Change. It edits \
the control surface; it does not stream audio.

Parameters are addressed by a short key (run `list` to see them all). Values are \
raw 7-bit device units for now; display-unit conversion is deferred to Stage 2.",
    after_help = EXAMPLES,
    propagate_version = true,
    styles = HELP_STYLES
)]
struct Cli {
    /// Use an in-memory mock device instead of real hardware.
    #[arg(long, global = true)]
    mock: bool,

    /// ALSA rawmidi port (`hw:CARD,DEV`); see the `ports` command.
    #[arg(long, global = true)]
    port: Option<String>,

    #[command(subcommand)]
    command: Command,
}

/// Colour scheme for help and error output. clap auto-disables colour when the
/// output is not a terminal (piped or redirected), so scripts see plain text.
const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().bold())
    .valid(AnsiColor::Green.on_default().bold())
    .invalid(AnsiColor::Yellow.on_default());

/// Examples shown at the foot of `rackctl-gx700 --help`.
const EXAMPLES: &str = "\
Examples:
  rackctl-gx700 ports                          List ALSA rawmidi ports
  rackctl-gx700 list                           List every parameter key
  rackctl-gx700 info preamp-gain               Explain one parameter
  rackctl-gx700 --port hw:1,0 get preamp-gain  Read a value
  rackctl-gx700 --port hw:1,0 set dist-enable on  Turn a block on
  rackctl-gx700 --port hw:1,0 dump             Show the current sound (readable)
  rackctl-gx700 --port hw:1,0 dump --patch 3   Show device patch memory 3
  rackctl-gx700 --port hw:1,0 save \"My Tone\"    Save the current sound to disk
  rackctl-gx700 --port hw:1,0 load \"My Tone\"    Load a saved patch (live)
  rackctl-gx700 --port hw:1,0 patches          List device user-patch names
  rackctl-gx700 patches --disk                 List patches saved on disk
  rackctl-gx700 --port hw:1,0 select 7         Select patch memory 7";

#[derive(Subcommand)]
enum Command {
    /// List the ALSA rawmidi ports available on the system.
    Ports,
    /// List every parameter with its key, block, kind, and address.
    List,
    /// Show one parameter's block, kind, range, and any enum values.
    Info {
        /// Parameter key (see `list`).
        param: String,
    },
    /// Read a parameter's current value.
    Get {
        /// Parameter key (see `list`).
        param: String,
    },
    /// Write a value to a parameter.
    Set {
        /// Parameter key (see `list`).
        param: String,
        /// The value to write (number, on/off, or enum index/label).
        value: String,
    },
    /// Print a patch in readable form: the current sound, a device slot, or a
    /// saved file.
    Dump {
        /// Read device patch memory slot N (1-200) instead of the current sound.
        #[arg(long)]
        patch: Option<u16>,
        /// Read a saved patch by name (no device needed) instead.
        #[arg(long)]
        file: Option<String>,
    },
    /// Save a whole patch to disk: the current sound, or device slot N.
    Save {
        /// Name to save under, in the gx700 patches directory.
        name: String,
        /// Save device patch memory slot N (1-200) instead of the current sound.
        #[arg(long)]
        patch: Option<u16>,
    },
    /// Load a saved whole-patch file onto the device.
    Load {
        /// Saved patch name to load.
        name: String,
        /// Write to USER patch memory slot N (1-100) instead of the current
        /// sound. DESTRUCTIVE: overwrites that stored patch.
        #[arg(long)]
        to_patch: Option<u16>,
    },
    /// Select a patch memory by Program Change.
    Select {
        /// Patch program number (0-127).
        n: u8,
    },
    /// List patch-memory slots and their names (on the device, or saved on disk).
    Patches {
        /// List the 100 preset patches instead of the 100 user patches.
        #[arg(long)]
        preset: bool,
        /// List patches saved on disk instead of on the device.
        #[arg(long)]
        disk: bool,
    },
    /// Print incoming `SysEx` messages as hex (a reverse-engineering aid).
    Recv,
    /// Print every incoming MIDI message, decoded (a link monitor / debugger).
    Monitor,
    /// Print a shell completion script for rackctl-gx700 to standard output.
    ///
    /// Redirect it to where your shell looks for completions, for example:
    ///   rackctl-gx700 completions bash | sudo tee /usr/share/bash-completion/completions/rackctl-gx700
    ///   rackctl-gx700 completions fish > ~/.config/fish/completions/rackctl-gx700.fish
    #[command(verbatim_doc_comment)]
    Completions {
        /// Shell to generate the completion script for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

/// Write the completion script for `shell` to standard output. Backend-free.
fn print_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
}

/// List ALSA rawmidi ports. Available only with the `alsa` feature.
fn list_ports() -> Result<()> {
    #[cfg(feature = "alsa")]
    {
        let ports = RawMidi::ports()?;
        if ports.is_empty() {
            eprintln!("no rawmidi ports found");
        }
        for port in ports {
            println!("{port}");
        }
        Ok(())
    }
    #[cfg(not(feature = "alsa"))]
    {
        anyhow::bail!("built without ALSA support; rebuild with the `alsa` feature")
    }
}

/// Print incoming `SysEx` as hex, until interrupted. Hardware-only.
#[cfg(feature = "alsa")]
fn recv(port: Option<&str>) -> Result<()> {
    let port = port.ok_or_else(|| anyhow::anyhow!("recv needs --port (see `ports`)"))?;
    let mut listener = RawMidi::open(port)?;
    listener.watch_sysex()?;
    Ok(())
}

#[cfg(not(feature = "alsa"))]
fn recv(_port: Option<&str>) -> Result<()> {
    anyhow::bail!("built without ALSA support; rebuild with the `alsa` feature")
}

/// Print decoded incoming MIDI, until interrupted. Hardware-only.
#[cfg(feature = "alsa")]
fn monitor(port: Option<&str>) -> Result<()> {
    let port = port.ok_or_else(|| anyhow::anyhow!("monitor needs --port (see `ports`)"))?;
    let mut listener = RawMidi::open(port)?;
    listener.watch_midi()?;
    Ok(())
}

#[cfg(not(feature = "alsa"))]
fn monitor(_port: Option<&str>) -> Result<()> {
    anyhow::bail!("built without ALSA support; rebuild with the `alsa` feature")
}

/// The selected backend, resolved once at startup.
enum Device {
    Mock(Gx700<MockTransport>),
    #[cfg(feature = "alsa")]
    Alsa(Gx700<RawMidi>),
}

fn open_device(mock: bool, port: Option<&str>) -> Result<Device> {
    if mock {
        return Ok(Device::Mock(Gx700::new(MockTransport::new())));
    }
    #[cfg(feature = "alsa")]
    {
        let port =
            port.ok_or_else(|| anyhow::anyhow!("no --port given (run `ports`, or use --mock)"))?;
        Ok(Device::Alsa(Gx700::open(port)?))
    }
    #[cfg(not(feature = "alsa"))]
    {
        let _ = port;
        anyhow::bail!("built without ALSA support; re-run with --mock")
    }
}

fn run_command<T: Transport>(dev: &mut Gx700<T>, command: Command) -> Result<()> {
    match command {
        Command::Get { param } => commands::get(dev, &param),
        Command::Set { param, value } => commands::set(dev, &param, &value),
        Command::Dump { patch, .. } => commands::dump_device(dev, patch),
        Command::Save { name, patch } => commands::save(dev, &name, patch),
        Command::Load { name, to_patch } => commands::load(dev, &name, to_patch),
        Command::Select { n } => commands::select(dev, n),
        Command::Patches { preset, .. } => commands::patches(dev, preset),
        // The backend-free and hardware-only commands are handled before a
        // device is opened; they never reach here.
        Command::Ports
        | Command::List
        | Command::Info { .. }
        | Command::Recv
        | Command::Monitor
        | Command::Completions { .. } => Ok(()),
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Commands that need no device (or open their own listener).
    match &cli.command {
        Command::Ports => return list_ports(),
        Command::List => {
            commands::list();
            return Ok(());
        }
        Command::Info { param } => return commands::info(param),
        Command::Recv => return recv(cli.port.as_deref()),
        Command::Monitor => return monitor(cli.port.as_deref()),
        Command::Completions { shell } => {
            print_completions(*shell);
            return Ok(());
        }
        // Disk-only operations need no device.
        Command::Patches { disk: true, .. } => {
            commands::patches_disk();
            return Ok(());
        }
        Command::Dump {
            file: Some(name), ..
        } => return commands::dump_file(name),
        _ => {}
    }

    match open_device(cli.mock, cli.port.as_deref())? {
        Device::Mock(mut dev) => run_command(&mut dev, cli.command),
        #[cfg(feature = "alsa")]
        Device::Alsa(mut dev) => run_command(&mut dev, cli.command),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
