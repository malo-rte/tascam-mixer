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
use tascam_us16x08::{Control, Meters, NUM_CHANNELS, Value};

use crate::app::App;

/// Full-scale meter sample (see `Meters` / `convert::meter_scale`).
const METER_FULL_SCALE: f32 = 32768.0;
/// Meter-bar height, shared with the master meters.
pub(crate) const METER_HEIGHT: f32 = 130.0;
const METER_SIZE: egui::Vec2 = egui::vec2(18.0, METER_HEIGHT);
/// Width of each channel column in the bridge.
const COLUMN_WIDTH: f32 = 44.0;

/// Number of meters tracked for peak-hold: the 16 channels plus master L and R.
pub(crate) const NUM_METERS: usize = 18;
/// Peak-hold index of the master left and right meters.
const MASTER_L: u32 = 16;
const MASTER_R: u32 = 17;
/// Bar fraction at or above which a meter is treated as clipping.
const CLIP_THRESHOLD: f32 = 0.98;
/// Seconds the clip indicator stays lit after a clip.
const CLIP_HOLD: f64 = 1.5;
/// Seconds the held peak stays put before it starts to fall.
const PEAK_HOLD: f64 = 2.0;
/// How fast the held peak marker falls once the hold expires, in bar fractions
/// per second.
const PEAK_DECAY: f32 = 0.5;

/// Per-meter peak-hold and clip state, advanced once per frame.
#[derive(Clone, Copy, Default)]
pub(crate) struct PeakHold {
    /// Held peak as a 0..=1 bar fraction.
    peak: f32,
    /// Clock time (eframe seconds) until which the peak is held before decaying.
    hold_until: f64,
    /// Clock time (eframe seconds) until which the clip indicator shows.
    clip_until: f64,
}

impl PeakHold {
    /// Fold in the current bar `fraction` at time `now`, `dt` seconds after the
    /// last update: latch a clip; let the held peak rise instantly, stay put for
    /// [`PEAK_HOLD`], then fall slowly.
    fn observe(&mut self, fraction: f32, now: f64, dt: f32) {
        if fraction >= CLIP_THRESHOLD {
            self.clip_until = now + CLIP_HOLD;
        }
        if fraction >= self.peak {
            self.peak = fraction;
            self.hold_until = now + PEAK_HOLD;
        } else if now >= self.hold_until {
            self.peak = (self.peak - PEAK_DECAY * dt).max(fraction);
        }
    }

    fn clipping(self, now: f64) -> bool {
        now < self.clip_until
    }
}

/// Advance every meter's peak-hold/clip state from the latest meter snapshot.
pub(crate) fn observe_meters(
    peaks: &mut [PeakHold; NUM_METERS],
    meters: &Meters,
    now: f64,
    last: &mut f64,
) {
    let dt = (now - *last) as f32;
    *last = now;
    for ch in 0..NUM_CHANNELS {
        let f = fraction(meters.channel_db(ch).unwrap_or(0));
        if let Some(p) = peaks.get_mut(ch as usize) {
            p.observe(f, now, dt);
        }
    }
    let (l, r) = meters.master_db();
    if let Some(p) = peaks.get_mut(MASTER_L as usize) {
        p.observe(fraction(l), now, dt);
    }
    if let Some(p) = peaks.get_mut(MASTER_R as usize) {
        p.observe(fraction(r), now, dt);
    }
}

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    let now = ui.input(|i| i.time);
    ui.horizontal(|ui| {
        for ch in 0..NUM_CHANNELS {
            channel_strip(app, ui, ch, now);
        }
        // Master L/R meters, right-aligned at the end of the bridge.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            master_meters(app, ui, now);
        });
    });
}

/// The master L/R level meters and mute, shown at the right of the bridge.
fn master_meters(app: &mut App, ui: &mut egui::Ui, now: f64) {
    ui.vertical(|ui| {
        // An empty name line and a label matching the channel columns' name +
        // selector, so the master meters sit at the same height.
        ui.small(" ");
        let _ = ui.selectable_label(false, "Master");
        ui.horizontal(|ui| {
            let (l, r) = app.meters().master_db();
            meter_bar(ui, fraction(l), app.peak(MASTER_L), now);
            meter_bar(ui, fraction(r), app.peak(MASTER_R), now);
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

fn channel_strip(app: &mut App, ui: &mut egui::Ui, ch: u32, now: f64) {
    ui.vertical(|ui| {
        ui.set_width(COLUMN_WIDTH);

        // The user-given name above the selector. A single line always (a space
        // when unset) so every column -- and the master -- keeps its meter at the
        // same height. Truncated to the column, full name on hover.
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

        // Channel selector. A linked pair highlights together when either
        // channel is selected.
        let sel = u32::from(app.selected);
        let selected = sel == ch || (app.linked(ch) && sel ^ 1 == ch);
        if ui
            .selectable_label(selected, format!("{}", ch + 1))
            .clicked()
        {
            app.selected = u8::try_from(ch).unwrap_or(0);
        }

        let level = app.meters().channel_db(ch).unwrap_or(0);
        meter_bar(ui, fraction(level), app.peak(ch), now);

        // Mute and solo, side by side under the meter.
        ui.horizontal(|ui| {
            let muted = app.cached_bool(Control::MuteSwitch, ch);
            if ui.selectable_label(muted, "M").clicked() {
                app.set(Control::MuteSwitch, ch, Value::Bool(!muted));
            }
            let solo = app.soloed(ch);
            if ui
                .selectable_label(solo, "S")
                .on_hover_text("Solo: mute the other channels")
                .clicked()
            {
                app.toggle_solo(ch);
            }
        });

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

/// Paint a vertical meter bar filled from the bottom, with a held peak marker
/// and a clip cap from `peak`.
pub(crate) fn meter_bar(ui: &mut egui::Ui, fraction: f32, peak: PeakHold, now: f64) {
    let (rect, _) = ui.allocate_exact_size(METER_SIZE, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(40));
    let height = rect.height() * fraction;
    let fill = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - height), rect.max);
    painter.rect_filled(fill, 2.0, egui::Color32::from_rgb(70, 200, 90));

    // Held peak: a thin line that rises with the level and falls slowly.
    if peak.peak > 0.001 {
        let y = rect.bottom() - rect.height() * peak.peak;
        painter.hline(
            rect.x_range(),
            y,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(235, 225, 90)),
        );
    }
    // Clip: a red cap at the top, latched for a moment after a clip.
    if peak.clipping(now) {
        let cap =
            egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), rect.top() + 3.0));
        painter.rect_filled(cap, 1.0, egui::Color32::from_rgb(235, 60, 60));
    }
}
