//! The OUTPUT panel: master meters/fader/mute and the global DSP switches.
//!
//! The dB readout truncates the control value to an int; the loss is irrelevant.
#![allow(clippy::cast_possible_truncation)]

use eframe::egui;
use tascam_us16x08::{Control, Value};

use crate::app::App;
use crate::bridge::METER_HEIGHT;

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Output");
    // Fixed-width numeric value boxes, matching the editor.
    ui.spacing_mut().interact_size.x = crate::channel::VALUE_BOX_WIDTH;

    // Master fader + mute (the L/R meters live in the bridge).
    ui.label("Master");
    ui.label("vol");
    let mut volume = app.cached_int(Control::MasterVolume, 0);
    ui.spacing_mut().slider_width = METER_HEIGHT;
    let fader = egui::Slider::new(&mut volume, 0..=133)
        .vertical()
        .custom_formatter(|n, _| format!("{:+} dB", n as i32 - 127));
    if ui.add(fader).changed() {
        app.set(Control::MasterVolume, 0, Value::Int(volume));
    }

    let mut muted = app.cached_bool(Control::MasterMute, 0);
    if ui.checkbox(&mut muted, "Mute").changed() {
        app.set(Control::MasterMute, 0, Value::Bool(muted));
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
}
