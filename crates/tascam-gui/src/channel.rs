//! The focused-channel editor: phase, fader, pan, EQ, and compressor.
//!
//! Widgets are driven by each control's [`tascam_us16x08::Kind`], so the range
//! and widget type come from the catalog. Small index casts for combo boxes are
//! harmless.
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]

use eframe::egui;
use tascam_us16x08::{Control, Kind, Value};

use crate::app::App;

/// Render the editor for the currently selected channel.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    let ch = u32::from(app.selected);
    ui.heading(format!("Channel {}", ch + 1));

    ui.horizontal(|ui| {
        control(app, ui, "Phase", Control::PhaseSwitch, ch);
        control(app, ui, "Mute", Control::MuteSwitch, ch);
    });

    control(app, ui, "Volume", Control::LineVolume, ch);
    control(app, ui, "Pan", Control::Pan, ch);

    ui.collapsing("EQ", |ui| {
        control(app, ui, "EQ enable", Control::EqSwitch, ch);
        control(app, ui, "Low gain", Control::EqLowVolume, ch);
        control(app, ui, "Low freq", Control::EqLowFreq, ch);
        control(app, ui, "Mid-low gain", Control::EqMidLowVolume, ch);
        control(app, ui, "Mid-low freq", Control::EqMidLowFreq, ch);
        control(app, ui, "Mid-low Q", Control::EqMidLowQ, ch);
        control(app, ui, "Mid-high gain", Control::EqMidHighVolume, ch);
        control(app, ui, "Mid-high freq", Control::EqMidHighFreq, ch);
        control(app, ui, "Mid-high Q", Control::EqMidHighQ, ch);
        control(app, ui, "High gain", Control::EqHighVolume, ch);
        control(app, ui, "High freq", Control::EqHighFreq, ch);
    });

    ui.collapsing("Compressor", |ui| {
        control(app, ui, "Comp enable", Control::CompSwitch, ch);
        control(app, ui, "Threshold", Control::CompThreshold, ch);
        control(app, ui, "Ratio", Control::CompRatio, ch);
        control(app, ui, "Attack", Control::CompAttack, ch);
        control(app, ui, "Release", Control::CompRelease, ch);
        control(app, ui, "Gain", Control::CompGain, ch);
    });
}

/// Render one control as the widget its kind calls for, writing through on edit.
fn control(app: &mut App, ui: &mut egui::Ui, label: &str, control: Control, index: u32) {
    match control.kind() {
        Kind::Bool => {
            let mut value = app.cached_bool(control, index);
            if ui.checkbox(&mut value, label).changed() {
                app.set(control, index, Value::Bool(value));
            }
        }
        Kind::Int { min, max, .. } => {
            let mut value = app.cached_int(control, index);
            ui.horizontal(|ui| {
                ui.label(label);
                if ui.add(egui::Slider::new(&mut value, min..=max)).changed() {
                    app.set(control, index, Value::Int(value));
                }
            });
        }
        Kind::Enum { values, .. } => {
            let current = app.cached_int(control, index);
            let mut selected = current;
            let text = usize::try_from(current)
                .ok()
                .and_then(|i| values.get(i))
                .copied()
                .unwrap_or("?");
            ui.horizontal(|ui| {
                ui.label(label);
                egui::ComboBox::from_id_salt((control, index))
                    .selected_text(text)
                    .show_ui(ui, |ui| {
                        for (i, name) in values.iter().enumerate() {
                            ui.selectable_value(&mut selected, i as i32, *name);
                        }
                    });
            });
            if selected != current {
                app.set(control, index, Value::Enum(selected));
            }
        }
        // Meter (and any future kind) is not an editable scalar control.
        _ => {}
    }
}
