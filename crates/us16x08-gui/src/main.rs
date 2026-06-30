//! `rackctl-us16x08-gui` — graphical mixer for the Tascam US-16x08.

mod app;
mod bridge;
mod channel;
mod config;
mod curves;
mod output;
mod poller;
mod preset_tab;
mod routing;

use anyhow::Result;
use clap::Parser;
use rackctl_us16x08::{Backend, MockBackend, Us16x08};

#[cfg(feature = "alsa")]
use rackctl_us16x08::AlsaBackend;

/// Graphical mixer for the Tascam US-16x08 DSP mixer.
#[derive(Parser)]
#[command(name = "rackctl-us16x08-gui", version)]
struct Cli {
    /// Use an in-memory mock device instead of real hardware.
    #[arg(long)]
    mock: bool,
}

/// Open the device as a boxed, `Send` backend (so it can be shared with the
/// poller thread): the in-memory mock, or real hardware.
fn open_device(mock: bool) -> Result<poller::Device> {
    if mock {
        return Ok(Us16x08::new(Box::new(MockBackend::new())));
    }
    #[cfg(feature = "alsa")]
    {
        Ok(Us16x08::new(Box::new(AlsaBackend::open()?)))
    }
    #[cfg(not(feature = "alsa"))]
    {
        anyhow::bail!("built without ALSA support; re-run with --mock")
    }
}

fn main() -> Result<()> {
    let mock = Cli::parse().mock;
    // Lets the app reopen the device after a USB replug.
    let reopen: app::Reopen = Box::new(move || open_device(mock));
    // Open the card now if we can. If it is absent, start disconnected with a
    // placeholder backend (never read) and let the app connect when the card
    // appears, applying the default preset then.
    let (device, connected) = match reopen() {
        Ok(device) => (device, true),
        Err(_) => (
            Us16x08::new(Box::new(MockBackend::new()) as Box<dyn Backend + Send>),
            false,
        ),
    };

    // Restore the saved window size before creating the window; an absent size
    // falls back to eframe's default. The app id is the Wayland app_id (the
    // window "class" in Hyprland/sway), so compositor rules can target it.
    let mut viewport = eframe::egui::ViewportBuilder::default().with_app_id("rackctl-us16x08-gui");
    if let Some([w, h]) = config::load().window {
        viewport = viewport.with_inner_size([w, h]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Tascam US-16x08 Mixer",
        options,
        Box::new(move |cc| {
            // The shared Rackctl UI font (JetBrains Mono Nerd Font), so both GUIs
            // share one typeface and the action-button icon glyphs.
            rackctl_ui::install_fonts(&cc.egui_ctx);
            let app = app::App::new(device, mock, connected, reopen);
            // Apply the saved zoom; Ctrl +/- adjusts from here and Save default
            // remembers it.
            cc.egui_ctx.set_zoom_factor(app.zoom());
            cc.egui_ctx.style_mut(|style| {
                // Uniform slider length so the editor's value boxes line up.
                style.spacing.slider_width = 120.0;
                // Reserve a gutter for scrollbars instead of floating them over the
                // content, so the bank surface's scrollbar never overlaps the
                // rightmost channel.
                style.spacing.scroll.floating = false;
            });
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))?;
    Ok(())
}
