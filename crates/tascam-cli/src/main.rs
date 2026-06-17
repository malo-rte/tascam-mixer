//! `tascamctl` — command-line control for the Tascam US-16x08 DSP mixer.

mod commands;
mod value;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tascam_us16x08::{Backend, MockBackend, Us16x08};

#[cfg(feature = "alsa")]
use tascam_us16x08::AlsaBackend;

#[derive(Parser)]
#[command(
    name = "tascamctl",
    version,
    about = "Control the Tascam US-16x08 DSP mixer"
)]
struct Cli {
    /// Use an in-memory mock device instead of real hardware.
    #[arg(long, global = true)]
    mock: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List every control with its key, scope, kind, and ALSA name.
    List,
    /// Explain the card's signal flow and how the 8 outputs are routed.
    Topology,
    /// Show details for one control (scope, range, enum values).
    Info {
        /// Control key (see `list`).
        control: String,
    },
    /// Read a control's current value.
    Get {
        /// Control key (see `list`).
        control: String,
        /// Channel/output index (0-based).
        #[arg(short, long, default_value_t = 0)]
        channel: u32,
    },
    /// Write a control's value.
    Set {
        /// Control key (see `list`).
        control: String,
        /// Absolute (number, on/off, enum index/label), relative `+N`/`-N` for
        /// integer controls, or `toggle` for booleans.
        #[arg(allow_hyphen_values = true)]
        value: String,
        /// Channel/output index (0-based).
        #[arg(short, long, default_value_t = 0)]
        channel: u32,
    },
    /// Read the level meters once.
    Meters {
        /// Print raw linear samples instead of dB-scaled values.
        #[arg(long)]
        raw: bool,
    },
    /// Print the level meters continuously until interrupted.
    Monitor {
        /// Poll interval in milliseconds.
        #[arg(long, default_value_t = 100)]
        interval: u64,
        /// Print raw linear samples instead of dB-scaled values.
        #[arg(long)]
        raw: bool,
    },
    /// Print control changes as they happen, until interrupted.
    Watch {
        /// Poll interval in milliseconds.
        #[arg(long, default_value_t = 500)]
        interval: u64,
    },
    /// Save state to a JSON file: the whole mixer, or one strip with --channel.
    Save {
        /// Output file path.
        file: String,
        /// Save only this channel's strip instead of the whole mixer.
        #[arg(short, long)]
        channel: Option<u32>,
    },
    /// Restore state from a JSON file (apply a strip preset to --channel).
    Load {
        /// Input file path.
        file: String,
        /// Target channel for a strip preset.
        #[arg(short, long)]
        channel: Option<u32>,
    },
}

/// The selected backend, resolved once at startup.
enum Device {
    Mock(Us16x08<MockBackend>),
    #[cfg(feature = "alsa")]
    Alsa(Us16x08<AlsaBackend>),
}

fn open_device(mock: bool) -> Result<Device> {
    if mock {
        return Ok(Device::Mock(Us16x08::new(MockBackend::new())));
    }
    #[cfg(feature = "alsa")]
    {
        Ok(Device::Alsa(Us16x08::open()?))
    }
    #[cfg(not(feature = "alsa"))]
    {
        anyhow::bail!("built without ALSA support; re-run with --mock")
    }
}

fn run_command<B: Backend>(dev: &mut Us16x08<B>, command: Command) -> Result<()> {
    match command {
        Command::List => {
            commands::list();
            Ok(())
        }
        Command::Topology => {
            commands::topology();
            Ok(())
        }
        Command::Info { control } => commands::info(&control),
        Command::Get { control, channel } => commands::get(dev, &control, channel),
        Command::Set {
            control,
            value,
            channel,
        } => commands::set(dev, &control, &value, channel),
        Command::Meters { raw } => commands::meters(dev, raw),
        Command::Monitor { interval, raw } => commands::monitor(dev, interval, raw),
        Command::Watch { interval } => commands::watch(dev, interval),
        Command::Save { file, channel } => commands::save(dev, &file, channel),
        Command::Load { file, channel } => commands::load(dev, &file, channel),
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // `list` and `info` are backend-independent; don't open a device for them.
    match &cli.command {
        Command::List => {
            commands::list();
            return Ok(());
        }
        Command::Topology => {
            commands::topology();
            return Ok(());
        }
        Command::Info { control } => return commands::info(control),
        _ => {}
    }

    match open_device(cli.mock)? {
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
