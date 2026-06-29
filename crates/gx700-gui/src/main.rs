//! `rackctl-gx700-gui` — graphical patch librarian and level balancer for the
//! BOSS GX-700.

mod app;
mod config;
mod device;
mod loader;
mod writer;

use anyhow::Result;
use clap::Parser;

/// Graphical patch librarian and level balancer for the BOSS GX-700.
#[derive(Parser)]
#[command(name = "rackctl-gx700-gui", version)]
struct Cli {
    /// Use an in-memory mock device instead of real hardware.
    #[arg(long)]
    mock: bool,
    /// ALSA rawmidi port of the GX-700 (`hw:CARD,DEV`); see `rackctl-gx700 ports`.
    #[arg(long)]
    port: Option<String>,
}

/// Install the `JetBrains Mono Nerd Font`: as the monospace text face, and as a fallback
/// on the proportional face so the Nerd Font icon glyphs (used on list buttons)
/// resolve everywhere. The font is vendored under `assets/fonts/` (SIL OFL 1.1).
fn install_fonts(ctx: &eframe::egui::Context) {
    use eframe::egui::{FontData, FontDefinitions, FontFamily};
    const NERD: &str = "jetbrains-mono-nerd";
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        NERD.to_owned(),
        FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMonoNerdFont-Regular.ttf"
        )),
    );
    // Monospace: JetBrains Mono first (the visible monospace face).
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.insert(0, NERD.to_owned());
    }
    // Proportional: keep the default UI face first, append the Nerd Font as a
    // fallback so icon code points still render in proportional text (buttons).
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.push(NERD.to_owned());
    }
    ctx.set_fonts(fonts);
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (mock, port) = (cli.mock, cli.port);
    // Lets the app (re)open the device on demand (Retry button).
    let reopen: app::Reopen = Box::new(move || device::open(mock, port.as_deref()));
    // Open now if we can; otherwise start disconnected with a never-read
    // placeholder and let the user Retry (e.g. after passing the right port).
    let (dev, connected) = match reopen() {
        Ok(dev) => (dev, true),
        Err(_) => (device::placeholder(), false),
    };

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("rackctl-gx700-gui")
        // A default wide enough that, at the default 1.5x zoom, the id + name +
        // level + buttons row fits without squeezing the name column. The UI gets
        // roughly width/zoom egui points, so 880/1.5 ≈ 585 points of usable width.
        .with_inner_size([880.0, 820.0]);
    if let Some([w, h]) = config::load().window {
        viewport = viewport.with_inner_size([w, h]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "BOSS GX-700 Patch Editor",
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            let app = app::App::new(dev, connected, reopen);
            cc.egui_ctx.set_zoom_factor(app.zoom());
            cc.egui_ctx
                .style_mut(|style| style.spacing.slider_width = 160.0);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))?;
    Ok(())
}
