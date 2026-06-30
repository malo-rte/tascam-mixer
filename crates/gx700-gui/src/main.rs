//! `rackctl-gx700-gui` — graphical patch librarian and level balancer for the
//! BOSS GX-700.

mod app;
mod config;
mod device;
mod loader;
mod prober;
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
    /// Start without connecting to the unit — edit scenes and the library offline.
    /// Use the Connect button (top bar) to go online later.
    #[arg(long)]
    offline: bool,
}

/// Install the `JetBrains Mono Nerd Font` (vendored under `assets/fonts/`, SIL OFL
/// 1.1). Two variants are used: the monospace face for monospace text (the slot
/// labels and the Loop diagram, which need cell alignment), and the *proportional*
/// (Propo) face as a fallback on the UI face — its icon glyphs carry their natural
/// advance width, so the list-button icons aren't clipped (the mono variant crams
/// wide icons into one cell and cuts them off on the right).
fn install_fonts(ctx: &eframe::egui::Context) {
    use eframe::egui::{FontData, FontDefinitions, FontFamily};
    const MONO: &str = "jetbrains-mono-nerd";
    const PROPO: &str = "jetbrains-mono-nerd-propo";
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        MONO.to_owned(),
        FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMonoNerdFont-Regular.ttf"
        )),
    );
    fonts.font_data.insert(
        PROPO.to_owned(),
        FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMonoNerdFontPropo-Regular.ttf"
        )),
    );
    // Monospace: JetBrains Mono first (the visible monospace face).
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.insert(0, MONO.to_owned());
    }
    // Proportional: JetBrains Mono (Propo) as the primary UI face — it also carries
    // the full-width icon glyphs. The default faces stay after it as fallback for
    // any code points it lacks.
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, PROPO.to_owned());
    }
    ctx.set_fonts(fonts);
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (mock, offline) = (cli.mock, cli.offline);
    // `--port` overrides the saved port; otherwise reuse the last-used one.
    let port = cli.port.or_else(|| config::load().port);
    // Lets the app (re)open the device on demand (Retry / Connect button).
    let reopen_port = port.clone();
    let reopen: app::Reopen = Box::new(move || device::open(mock, reopen_port.as_deref()));
    // Open now if we can; otherwise start disconnected with a never-read
    // placeholder and let the user Retry (e.g. after passing the right port).
    // `--offline` skips the connect attempt entirely and starts in offline mode.
    let (dev, connected) = if offline {
        (device::placeholder(), false)
    } else {
        match reopen() {
            Ok(dev) => (dev, true),
            Err(_) => (device::placeholder(), false),
        }
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
            let app = app::App::new(dev, connected, reopen, offline, port);
            cc.egui_ctx.set_zoom_factor(app.zoom());
            cc.egui_ctx.style_mut(|style| {
                style.spacing.slider_width = 160.0;
                // Reserve a gutter for scrollbars instead of floating them over the
                // content, so a list's scrollbar never clips the trailing names.
                style.spacing.scroll.floating = false;
            });
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))?;
    Ok(())
}
