//! The always-visible meter bridge (16 channels) and master strip.
//!
//! Rendering uses small numeric casts (control values and meter samples to
//! pixel fractions); the precision loss is irrelevant for drawing.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use eframe::egui;
use tascam_us16x08::{Control, NUM_CHANNELS, Value};

use crate::app::App;

/// Full-scale meter sample (see `Meters` / `convert::meter_scale`).
const METER_FULL_SCALE: f32 = 32768.0;
const METER_HEIGHT: f32 = 130.0;
const METER_SIZE: egui::Vec2 = egui::vec2(18.0, METER_HEIGHT);
/// Width of each channel column in the bridge.
const COLUMN_WIDTH: f32 = 44.0;
/// Length of the master volume fader — matched to the meter height so the
/// fader and the L/R meters line up as one block.
const FADER_LENGTH: f32 = METER_HEIGHT;

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for ch in 0..NUM_CHANNELS {
            channel_strip(app, ui, ch);
        }
        ui.separator();
        master_strip(app, ui);
    });
}

fn channel_strip(app: &mut App, ui: &mut egui::Ui, ch: u32) {
    ui.vertical(|ui| {
        ui.set_width(COLUMN_WIDTH);

        // Label above the meter, matching the master strip's layout.
        let selected = u32::from(app.selected) == ch;
        if ui
            .selectable_label(selected, format!("{}", ch + 1))
            .clicked()
        {
            app.selected = u8::try_from(ch).unwrap_or(0);
        }

        let level = app.meters().channel_db(ch).unwrap_or(0);
        meter_bar(ui, fraction(level));

        let muted = app.cached_bool(Control::MuteSwitch, ch);
        if ui.selectable_label(muted, "M").clicked() {
            app.set(Control::MuteSwitch, ch, Value::Bool(!muted));
        }
    });
}

fn master_strip(app: &mut App, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.label("MASTER");
        // Meters on the left; the fader column (vol label, fader, mute) on the
        // right, so the section stays short and mirrors the channel strips.
        // Top-aligned so the `vol` label sits above the fader, not above the
        // meters.
        ui.horizontal_top(|ui| {
            let (l, r) = app.meters().master_db();
            meter_bar(ui, fraction(l));
            meter_bar(ui, fraction(r));

            ui.vertical(|ui| {
                ui.label("vol");
                let mut volume = app.cached_int(Control::MasterVolume, 0);
                ui.spacing_mut().slider_width = FADER_LENGTH;
                if ui
                    .add(egui::Slider::new(&mut volume, 0..=133).vertical())
                    .changed()
                {
                    app.set(Control::MasterVolume, 0, Value::Int(volume));
                }

                let muted = app.cached_bool(Control::MasterMute, 0);
                if ui.selectable_label(muted, "MUTE").clicked() {
                    app.set(Control::MasterMute, 0, Value::Bool(!muted));
                }
            });
        });
    });
}

/// Normalise a scaled meter sample to a 0..=1 bar fraction.
fn fraction(level: i32) -> f32 {
    (level.max(0) as f32 / METER_FULL_SCALE).clamp(0.0, 1.0)
}

/// Paint a vertical meter bar filled from the bottom.
fn meter_bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(METER_SIZE, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(40));
    let height = rect.height() * fraction;
    let fill = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - height), rect.max);
    painter.rect_filled(fill, 2.0, egui::Color32::from_rgb(70, 200, 90));
}
