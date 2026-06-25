//! The always-visible meter bridge (16 channels). The meter-bar helpers are
//! shared with the OUTPUT panel's master meters.
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
/// Meter-bar height, shared with the master meters.
pub(crate) const METER_HEIGHT: f32 = 130.0;
const METER_SIZE: egui::Vec2 = egui::vec2(18.0, METER_HEIGHT);
/// Width of each channel column in the bridge.
const COLUMN_WIDTH: f32 = 44.0;

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for ch in 0..NUM_CHANNELS {
            channel_strip(app, ui, ch);
        }
        // Master L/R meters, right-aligned at the end of the bridge.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            master_meters(app, ui);
        });
    });
}

/// The master L/R level meters and mute, shown at the right of the bridge.
fn master_meters(app: &mut App, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.label("Master");
        ui.horizontal(|ui| {
            let (l, r) = app.meters().master_db();
            meter_bar(ui, fraction(l));
            meter_bar(ui, fraction(r));
        });
        // Mute button centred under the two meter bars (a single left-aligned
        // "M" would sit under the left bar only).
        let muted = app.cached_bool(Control::MasterMute, 0);
        let width = 2.0 * METER_SIZE.x + ui.spacing().item_spacing.x;
        let height = ui.spacing().interact_size.y;
        if ui
            .add_sized([width, height], egui::SelectableLabel::new(muted, "M"))
            .clicked()
        {
            app.set(Control::MasterMute, 0, Value::Bool(!muted));
        }
    });
}

fn channel_strip(app: &mut App, ui: &mut egui::Ui, ch: u32) {
    ui.vertical(|ui| {
        ui.set_width(COLUMN_WIDTH);

        // Label above the meter, matching the master strip's layout. A linked
        // pair highlights together when either channel is selected.
        let sel = u32::from(app.selected);
        let selected = sel == ch || (app.linked(ch) && sel ^ 1 == ch);
        if ui
            .selectable_label(selected, format!("{}", ch + 1))
            .clicked()
        {
            app.selected = u8::try_from(ch).unwrap_or(0);
        }

        // The user-given name, truncated to the column. A single line always
        // (a space when unset) keeps every column's meter at the same height.
        let name = app.channel_name(ch);
        let shown = if name.is_empty() {
            " ".to_owned()
        } else {
            short(name, 6)
        };
        let label = ui.small(shown);
        if !name.is_empty() {
            label.on_hover_text(name);
        }

        let level = app.meters().channel_db(ch).unwrap_or(0);
        meter_bar(ui, fraction(level));

        let muted = app.cached_bool(Control::MuteSwitch, ch);
        if ui.selectable_label(muted, "M").clicked() {
            app.set(Control::MuteSwitch, ch, Value::Bool(!muted));
        }

        if app.linked(ch) {
            ui.small("link");
        }
    });
}

/// Shorten a channel name to at most `max` characters, with an ellipsis if it
/// was cut, so it fits a narrow bridge column.
fn short(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_owned();
    }
    let kept: String = name.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

/// Normalise a scaled meter sample to a 0..=1 bar fraction.
pub(crate) fn fraction(level: i32) -> f32 {
    (level.max(0) as f32 / METER_FULL_SCALE).clamp(0.0, 1.0)
}

/// Paint a vertical meter bar filled from the bottom.
pub(crate) fn meter_bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(METER_SIZE, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(40));
    let height = rect.height() * fraction;
    let fill = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - height), rect.max);
    painter.rect_filled(fill, 2.0, egui::Color32::from_rgb(70, 200, 90));
}
