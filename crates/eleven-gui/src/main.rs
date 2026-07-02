//! `rackctl-eleven-gui` — graphical patch librarian for the Avid/Digidesign
//! Eleven Rack (mirrors the GX-700 GUI's librarian / presets / library / scene
//! structure; the Eleven can't do live per-parameter edits, so the editor tab is a
//! read-only inspector plus MIDI-CC quick-controls).

mod app;
mod config;
mod device;
mod loader;
mod writer;

use anyhow::Result;
use clap::Parser;

/// Graphical patch librarian for the Avid/Digidesign Eleven Rack.
#[derive(Parser)]
#[command(name = "rackctl-eleven-gui", version)]
struct Cli {
    /// Use an in-memory mock device instead of real hardware.
    #[arg(long)]
    mock: bool,
    /// ALSA rawmidi port of the Eleven Rack (`hw:CARD,DEV`); see `rackctl-eleven ports`.
    #[arg(long)]
    port: Option<String>,
    /// Start without connecting to the unit — browse the library and compose scenes
    /// offline. Use the Connect button (top bar) to go online later.
    #[arg(long)]
    offline: bool,
    /// Log every MIDI byte sent/received to this file (for diagnosing device I/O).
    #[arg(long, value_name = "FILE")]
    midi_log: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (mock, offline) = (cli.mock, cli.offline);
    // `--port` overrides the saved port; otherwise reuse the last-used one.
    let port = cli.port.or_else(|| config::load().port);
    // Lets the app (re)open the device on demand (Retry / Connect button).
    let reopen_port = port.clone();
    let midi_log = cli.midi_log;
    let mut reopen: app::Reopen =
        Box::new(move || device::open(mock, reopen_port.as_deref(), midi_log.as_deref()));
    // Open now if we can; otherwise start disconnected and let the user Retry.
    // `--offline` skips the connect attempt entirely.
    let (dev, connected) = if offline {
        (device::placeholder(), false)
    } else {
        match reopen() {
            Ok(dev) => (dev, true),
            Err(_) => (device::placeholder(), false),
        }
    };

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("rackctl-eleven-gui")
        .with_inner_size([760.0, 820.0]);
    if let Some([w, h]) = config::load().window {
        viewport = viewport.with_inner_size([w, h]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Eleven Rack Patch Librarian",
        options,
        Box::new(move |cc| {
            rackctl_ui::install_fonts(&cc.egui_ctx);
            let app = app::App::new(dev, connected, reopen, offline, port);
            cc.egui_ctx.set_zoom_factor(app.zoom());
            cc.egui_ctx.style_mut(|style| {
                style.spacing.slider_width = 160.0;
                // Reserve a gutter for scrollbars instead of floating them over the
                // content, so a list's scrollbar never clips trailing names.
                style.spacing.scroll.floating = false;
            });
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))?;
    Ok(())
}
