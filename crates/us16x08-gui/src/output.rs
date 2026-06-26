//! The OUTPUT panel: master meters/fader/mute and the global DSP switches.

use eframe::egui;
use rackctl_ui::{ActionKind, action_button};
use rackctl_us16x08::{Control, Value};

use crate::app::App;
use crate::bridge::METER_HEIGHT;
use crate::channel::{human_text, parse_human};

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Output");
    // Fixed-width numeric value boxes, matching the editor.
    ui.spacing_mut().interact_size.x = crate::channel::VALUE_BOX_WIDTH;

    // Master fader (the L/R meters and mute live in the bridge).
    ui.label("Master");
    ui.label("vol");
    let mut volume = app.cached_int(Control::MasterVolume, 0);
    ui.spacing_mut().slider_width = METER_HEIGHT;
    let fader = egui::Slider::new(&mut volume, 0..=133)
        .vertical()
        .custom_formatter(|n, _| human_text(Control::MasterVolume, n))
        .custom_parser(|s| parse_human(Control::MasterVolume, s));
    if ui.add(fader).changed() {
        app.set(Control::MasterVolume, 0, Value::Int(volume));
    }

    ui.separator();

    let mut bypass = app.cached_bool(Control::DspBypass, 0);
    if ui.checkbox(&mut bypass, "DSP bypass").changed() {
        app.set(Control::DspBypass, 0, Value::Bool(bypass));
    }
    let mut buss = app.cached_bool(Control::BussOut, 0);
    if ui.checkbox(&mut buss, "Buss out").changed() {
        app.set(Control::BussOut, 0, Value::Bool(buss));
    }

    ui.separator();

    // The shared default preset: whole-mixer state saved to / restored from the
    // config directory, also reachable via `rackctl-us16x08 default`. The interface
    // zoom and window size are saved and restored alongside it as part of the
    // user's setup.
    if action_button(ui, "Save default", ActionKind::Commit).clicked() {
        let ctx = ui.ctx();
        let zoom = ctx.zoom_factor();
        // screen_rect is in egui points (scaled by zoom); the window inner size
        // is logical points = points * zoom.
        let size = ctx.screen_rect().size() * zoom;
        app.save_default(zoom, [size.x, size.y]);
    }
    if action_button(ui, "Load default", ActionKind::Read).clicked() {
        let (zoom, window) = app.load_default();
        let ctx = ui.ctx();
        ctx.set_zoom_factor(zoom);
        if let Some([w, h]) = window {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
    }
}
