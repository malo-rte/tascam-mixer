//! The focused-channel editor: phase, fader, pan, EQ, and compressor.
//!
//! Widgets are driven by each control's [`tascam_us16x08::Kind`]; the EQ and
//! compressor sections add response/transfer curves (see [`crate::curves`]).
//! Small numeric casts for combo indices and plot/curve math are harmless.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use tascam_us16x08::{COMP_RATIO_VALUES, Control, Kind, Value};

use crate::app::App;
use crate::curves::{self, EqBand};

/// Full-scale meter sample for the gain-reduction bar.
const METER_FULL_SCALE: f32 = 32768.0;

/// Column widths for the INPUT / EQ / COMPRESSOR boxes.
const INPUT_WIDTH: f32 = 220.0;
const DSP_WIDTH: f32 = 320.0;

/// Render the editor for the currently selected channel.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    let selected = u32::from(app.selected);
    let linked = app.linked(selected);
    let low = selected & !1;
    // When linked, edit/display via the lower channel of the pair.
    let ch = if linked { low } else { selected };

    ui.horizontal_top(|ui| {
        input_box(app, ui, ch, selected, linked, low);
        eq_box(app, ui, ch);
        comp_box(app, ui, ch);
    });
}

/// The INPUT box: channel identity, link, phase, mute, fader, pan/balance.
fn input_box(app: &mut App, ui: &mut egui::Ui, ch: u32, selected: u32, linked: bool, low: u32) {
    ui.group(|ui| {
        ui.set_width(INPUT_WIDTH);
        ui.vertical(|ui| {
            if linked {
                ui.heading(format!("INPUT {}-{}", low + 1, low + 2));
            } else {
                ui.heading(format!("INPUT {}", selected + 1));
            }

            let mut link_on = linked;
            if ui
                .checkbox(&mut link_on, format!("Stereo link {}-{}", low + 1, low + 2))
                .changed()
            {
                app.toggle_link(selected);
            }

            egui::Grid::new("input_grid").num_columns(2).show(ui, |ui| {
                control(app, ui, "Phase", Control::PhaseSwitch, ch);
                control(app, ui, "Mute", Control::MuteSwitch, ch);
                control(app, ui, "Volume", Control::LineVolume, ch);
                control(
                    app,
                    ui,
                    if linked { "Balance" } else { "Pan" },
                    Control::Pan,
                    ch,
                );
            });
        });
    });
}

/// The EQ box: response graph on top, then the band controls.
fn eq_box(app: &mut App, ui: &mut egui::Ui, ch: u32) {
    ui.group(|ui| {
        ui.set_width(DSP_WIDTH);
        ui.vertical(|ui| {
            ui.heading("EQ");
            eq_curve(app, ui, ch);
            egui::Grid::new("eq_grid").num_columns(2).show(ui, |ui| {
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
        });
    });
}

/// The COMPRESSOR box: transfer graph and GR on top, then the parameters.
fn comp_box(app: &mut App, ui: &mut egui::Ui, ch: u32) {
    ui.group(|ui| {
        ui.set_width(DSP_WIDTH);
        ui.vertical(|ui| {
            ui.heading("Compressor");
            comp_curve(app, ui, ch);
            egui::Grid::new("comp_grid").num_columns(2).show(ui, |ui| {
                control(app, ui, "Comp enable", Control::CompSwitch, ch);
                control(app, ui, "Threshold", Control::CompThreshold, ch);
                control(app, ui, "Ratio", Control::CompRatio, ch);
                control(app, ui, "Attack", Control::CompAttack, ch);
                control(app, ui, "Release", Control::CompRelease, ch);
                control(app, ui, "Gain", Control::CompGain, ch);
            });
        });
    });
}

/// The EQ response curve for the channel's current band settings (indicative).
fn eq_curve(app: &App, ui: &mut egui::Ui, ch: u32) {
    let bands = [
        EqBand {
            f0: curves::log_freq(app.cached_int(Control::EqLowFreq, ch), 31, 32.0, 1600.0),
            q: 0.7,
            gain_db: curves::eq_gain_db(app.cached_int(Control::EqLowVolume, ch)),
        },
        EqBand {
            f0: curves::log_freq(app.cached_int(Control::EqMidLowFreq, ch), 63, 100.0, 3200.0),
            q: curves::q_value(app.cached_int(Control::EqMidLowQ, ch)),
            gain_db: curves::eq_gain_db(app.cached_int(Control::EqMidLowVolume, ch)),
        },
        EqBand {
            f0: curves::log_freq(
                app.cached_int(Control::EqMidHighFreq, ch),
                63,
                500.0,
                8000.0,
            ),
            q: curves::q_value(app.cached_int(Control::EqMidHighQ, ch)),
            gain_db: curves::eq_gain_db(app.cached_int(Control::EqMidHighVolume, ch)),
        },
        EqBand {
            f0: curves::log_freq(app.cached_int(Control::EqHighFreq, ch), 31, 1600.0, 16000.0),
            q: 0.7,
            gain_db: curves::eq_gain_db(app.cached_int(Control::EqHighVolume, ch)),
        },
    ];

    // x is log10(Hz) over ~20 Hz .. 20 kHz.
    let points: Vec<[f64; 2]> = (0..=200)
        .map(|i| {
            let lf = 1.3 + (4.3 - 1.3) * (f64::from(i) / 200.0);
            [lf, curves::eq_response_db(&bands, 10f64.powf(lf))]
        })
        .collect();

    Plot::new("eq_curve")
        .height(130.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .include_y(-15.0)
        .include_y(15.0)
        .x_axis_formatter(|mark, _| hz_label(mark.value))
        .show(ui, |plot| plot.line(Line::new(PlotPoints::from(points))));
}

/// The compressor transfer curve (input dB -> output dB) plus a GR meter.
fn comp_curve(app: &App, ui: &mut egui::Ui, ch: u32) {
    let threshold = f64::from(app.cached_int(Control::CompThreshold, ch) - 32);
    let makeup = f64::from(app.cached_int(Control::CompGain, ch));
    let ratio = COMP_RATIO_VALUES
        .get(usize::try_from(app.cached_int(Control::CompRatio, ch)).unwrap_or(0))
        .map_or(1.0, |label| curves::ratio_from_label(label));

    let points: Vec<[f64; 2]> = (0..=60)
        .map(|i| {
            let input = -60.0 + f64::from(i);
            [
                input,
                curves::comp_output_db(input, threshold, ratio, makeup),
            ]
        })
        .collect();

    Plot::new("comp_curve")
        .height(130.0)
        .data_aspect(1.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .show(ui, |plot| plot.line(Line::new(PlotPoints::from(points))));

    if let Some(gr) = app.meters().reduction_db(ch) {
        let fraction = (gr.max(0) as f32 / METER_FULL_SCALE).clamp(0.0, 1.0);
        ui.add(egui::ProgressBar::new(fraction).text("gain reduction"));
    }
}

/// Render one control as the widget its kind calls for, writing through on edit.
fn control(app: &mut App, ui: &mut egui::Ui, label: &str, control: Control, index: u32) {
    // One grid row: label in column 1, the widget in column 2.
    ui.label(label);
    match control.kind() {
        Kind::Bool => {
            let mut value = app.cached_bool(control, index);
            if ui.checkbox(&mut value, "").changed() {
                app.set(control, index, Value::Bool(value));
            }
        }
        Kind::Int { min, max, .. } => {
            let mut value = app.cached_int(control, index);
            let slider = egui::Slider::new(&mut value, min..=max)
                .custom_formatter(move |n, _| human_text(control, n));
            if ui.add(slider).changed() {
                app.set(control, index, Value::Int(value));
            }
        }
        Kind::Enum { values, .. } => {
            let current = app.cached_int(control, index);
            let mut selected = current;
            let text = usize::try_from(current)
                .ok()
                .and_then(|i| values.get(i))
                .copied()
                .unwrap_or("?");
            egui::ComboBox::from_id_salt((control, index))
                .selected_text(text)
                .show_ui(ui, |ui| {
                    for (i, name) in values.iter().enumerate() {
                        ui.selectable_value(&mut selected, i as i32, *name);
                    }
                });
            if selected != current {
                app.set(control, index, Value::Enum(selected));
            }
        }
        // Meter (and any future kind) is not an editable scalar control.
        _ => {}
    }
    ui.end_row();
}

/// Format a raw control value in human units for the slider readout.
fn human_text(control: Control, raw: f64) -> String {
    let raw = raw as i32;
    match control {
        Control::EqLowVolume
        | Control::EqMidLowVolume
        | Control::EqMidHighVolume
        | Control::EqHighVolume => format!("{:+} dB", raw - 12),
        Control::CompThreshold => format!("{} dB", raw - 32),
        Control::CompGain => format!("+{raw} dB"),
        Control::CompAttack => format!("{} ms", raw + 2),
        Control::CompRelease => format!("{} ms", (raw + 1) * 10),
        _ => format!("{raw}"),
    }
}

/// Format a log10(Hz) plot mark as a frequency label.
fn hz_label(log_hz: f64) -> String {
    let hz = 10f64.powf(log_hz);
    if hz >= 1000.0 {
        format!("{:.0}k", hz / 1000.0)
    } else {
        format!("{hz:.0}")
    }
}
