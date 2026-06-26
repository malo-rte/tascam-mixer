//! `rackctl-gx700-gui` — graphical patch librarian and level balancer for the
//! BOSS GX-700.

mod app;
mod config;
mod device;
mod loader;

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

    let mut viewport = eframe::egui::ViewportBuilder::default().with_app_id("rackctl-gx700-gui");
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
