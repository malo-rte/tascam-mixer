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
    about = "Control the Tascam US-16x08 DSP mixer",
    long_about = "\
tascamctl reads and writes the US-16x08's on-board DSP mixer -- faders, EQ, \
compressor, mutes, and output routing -- over ALSA. It drives the control \
surface only; it does not stream or record audio.

Controls are addressed by a short key (run `list` to see them all) and, for \
per-channel and per-output controls, a 0-based index given with --channel. \
Values read and write in display units, such as dB, Hz, ms, and pan percent.",
    after_help = EXAMPLES,
    propagate_version = true
)]
struct Cli {
    /// Use an in-memory mock device instead of real hardware.
    #[arg(long, global = true)]
    mock: bool,

    #[command(subcommand)]
    command: Command,
}

/// Examples shown at the foot of `tascamctl --help`.
const EXAMPLES: &str = "\
Examples:
  tascamctl list                         List every control key
  tascamctl info eq-low-volume           Explain one control
  tascamctl get master-volume            Read a value
  tascamctl set mute on -c 3             Mute channel 3
  tascamctl set master-volume -6dB       Set the master to -6 dB
  tascamctl set line-volume +2 -c 0      Nudge channel 0 up 2 dB
  tascamctl monitor                      Watch the meters live
  tascamctl save mix.json                Back up the whole mixer
  tascamctl default --save               Remember the current mix as the default";

#[derive(Subcommand)]
enum Command {
    /// List every control with its key, scope, kind, and ALSA name.
    List,
    /// Explain the signal flow and how the 8 outputs are routed.
    Topology,
    /// Show one control's scope, range, and any enum values.
    Info {
        /// Control key (see `list`).
        control: String,
    },
    /// Read a control's current value.
    Get {
        /// Control key (see `list`).
        control: String,
        /// Channel or output index (0-based).
        #[arg(short, long, default_value_t = 0)]
        channel: u32,
    },
    /// Write a value to a control
    ///
    /// VALUE is one of:
    ///   number, on/off, label   an absolute value, in the control's own units
    ///   +N or -N                a relative step (integer controls only)
    ///   toggle                  flip a boolean control
    ///
    /// A bare leading `-` is read as a relative step; for a negative absolute
    /// value, add a unit suffix (for example `-6dB`).
    #[command(after_help = SET_EXAMPLES, verbatim_doc_comment)]
    Set {
        /// Control key (see `list`).
        control: String,
        /// The value to write (absolute, relative +N/-N, or toggle).
        #[arg(allow_hyphen_values = true)]
        value: String,
        /// Channel or output index (0-based).
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
        /// Poll interval, in milliseconds.
        #[arg(long, default_value_t = 100)]
        interval: u64,
        /// Print raw linear samples instead of dB-scaled values.
        #[arg(long)]
        raw: bool,
    },
    /// Print control changes as they happen, until interrupted.
    Watch {
        /// Poll interval, in milliseconds.
        #[arg(long, default_value_t = 500)]
        interval: u64,
    },
    /// Save the mixer, or one channel strip, to a JSON file.
    ///
    /// Without --channel, saves the whole mixer (master, routing, and all 16
    /// strips). With --channel, saves only that one channel's strip, which can
    /// later be loaded onto any channel.
    Save {
        /// Output file path.
        file: String,
        /// Save only this channel's strip instead of the whole mixer.
        #[arg(short, long)]
        channel: Option<u32>,
    },
    /// Load a mixer or strip preset from a JSON file.
    ///
    /// A whole-mixer preset is loaded as-is. A strip preset must be given a
    /// target channel with --channel.
    Load {
        /// Input file path.
        file: String,
        /// Target channel for a strip preset.
        #[arg(short, long)]
        channel: Option<u32>,
    },
    /// Load the shared default mixer preset (or save it with --save).
    ///
    /// The default preset lives in the configuration directory and is shared
    /// with the GUI's "Save default" and "Load default" buttons. With --save,
    /// capture the current mixer as the default instead of loading it.
    Default {
        /// Save the current mixer state as the default instead of loading it.
        #[arg(long)]
        save: bool,
    },
}

/// Examples shown at the foot of `tascamctl set --help`.
const SET_EXAMPLES: &str = "\
Examples:
  tascamctl set mute on -c 3             Mute channel 3
  tascamctl set master-volume -6dB       Absolute -6 dB
  tascamctl set line-volume +2 -c 0      Relative +2 dB on channel 0
  tascamctl set comp-enable toggle -c 0  Flip the compressor on channel 0";

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
        Command::Default { save } => commands::default_preset(dev, save),
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
