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
use egui_plot::{Line, Plot, PlotBounds, PlotPoints};
use tascam_us16x08::{COMP_RATIO_VALUES, Control, Kind, Value, units};

use crate::app::App;
use crate::bridge;
use crate::curves::{self, BandType, EqBand};

/// Full-scale meter sample for the gain-reduction bar.
const METER_FULL_SCALE: f32 = 32768.0;

/// Column widths for the INPUT / EQ / COMPRESSOR boxes.
const INPUT_WIDTH: f32 = 300.0;
const DSP_WIDTH: f32 = 320.0;
/// Length of the INPUT volume fader — matched to the meter height beside it.
const VOLUME_FADER_LENGTH: f32 = bridge::METER_HEIGHT;
/// Footprint reserved for the meter+fader strip, so the switch column's width
/// pushes the strip flush to the right edge.
const VOLUME_STRIP_WIDTH: f32 = 90.0;
/// Minimum width for the numeric value boxes, wide enough for the longest
/// readout (e.g. `-127 dB`, `1000 ms`) so they are a fixed, uniform size.
pub(crate) const VALUE_BOX_WIDTH: f32 = 60.0;

/// Render the editor for the currently selected channel.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    let selected = u32::from(app.selected);
    let linked = app.linked(selected);
    let low = selected & !1;
    // When linked, edit/display via the lower channel of the pair.
    let ch = if linked { low } else { selected };

    // Fixed-width numeric value boxes across the editor.
    ui.spacing_mut().interact_size.x = VALUE_BOX_WIDTH;

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

            ui.horizontal_top(|ui| {
                // Switches stacked on the left; the fixed width pushes the
                // volume strip flush to the right edge.
                ui.vertical(|ui| {
                    ui.set_width(INPUT_WIDTH - VOLUME_STRIP_WIDTH);
                    let mut link_on = linked;
                    if ui
                        .checkbox(&mut link_on, format!("Stereo link {}-{}", low + 1, low + 2))
                        .changed()
                    {
                        app.toggle_link(selected);
                    }
                    let mut phase = app.cached_bool(Control::PhaseSwitch, ch);
                    if ui.checkbox(&mut phase, "Phase").changed() {
                        app.set(Control::PhaseSwitch, ch, Value::Bool(phase));
                    }
                    let mut mute = app.cached_bool(Control::MuteSwitch, ch);
                    if ui.checkbox(&mut mute, "Mute").changed() {
                        app.set(Control::MuteSwitch, ch, Value::Bool(mute));
                    }
                });

                // Volume fader strip, right-aligned: the channel meter beside
                // the vertical fader, both the same height.
                ui.vertical(|ui| {
                    ui.label("Volume");
                    ui.horizontal(|ui| {
                        let level = app.meters().channel_db(ch).unwrap_or(0);
                        bridge::meter_bar(ui, bridge::fraction(level));
                        ui.spacing_mut().slider_width = VOLUME_FADER_LENGTH;
                        let mut volume = app.cached_int(Control::LineVolume, ch);
                        let fader = egui::Slider::new(&mut volume, 0..=133)
                            .vertical()
                            .custom_formatter(|n, _| human_text(Control::LineVolume, n))
                            .custom_parser(|s| parse_human(Control::LineVolume, s));
                        if ui.add(fader).changed() {
                            app.set(Control::LineVolume, ch, Value::Int(volume));
                        }
                    });
                });
            });

            // Pan along the bottom. A linked pair shows a balance that tilts
            // the hard-panned stereo image instead of a single pan position.
            ui.horizontal(|ui| {
                ui.label(if linked { "Balance" } else { "Pan" });
                ui.spacing_mut().slider_width = INPUT_WIDTH - 130.0;
                if linked {
                    let mut balance = app.pair_balance(ch);
                    let slider = egui::Slider::new(&mut balance, 0..=254)
                        .custom_formatter(|n, _| human_text(Control::Pan, n))
                        .custom_parser(|s| parse_human(Control::Pan, s));
                    if ui.add(slider).changed() {
                        app.set_balance(ch, balance);
                    }
                } else {
                    let mut pan = app.cached_int(Control::Pan, ch);
                    let slider = egui::Slider::new(&mut pan, 0..=254)
                        .custom_formatter(|n, _| human_text(Control::Pan, n))
                        .custom_parser(|s| parse_human(Control::Pan, s));
                    if ui.add(slider).changed() {
                        app.set(Control::Pan, ch, Value::Int(pan));
                    }
                }
            });
        });
    });
}

/// The EQ box: response graph on top, then the band controls.
fn eq_box(app: &mut App, ui: &mut egui::Ui, ch: u32) {
    ui.group(|ui| {
        ui.set_width(DSP_WIDTH);
        ui.vertical(|ui| {
            title_row(app, ui, "EQ", Control::EqSwitch, ch);
            eq_curve(app, ui, ch);
            egui::Grid::new("eq_grid").num_columns(2).show(ui, |ui| {
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
            title_row(app, ui, "Compressor", Control::CompSwitch, ch);
            comp_curve(app, ui, ch);
            egui::Grid::new("comp_grid").num_columns(2).show(ui, |ui| {
                control(app, ui, "Threshold", Control::CompThreshold, ch);
                control(app, ui, "Ratio", Control::CompRatio, ch);
                control(app, ui, "Attack", Control::CompAttack, ch);
                control(app, ui, "Release", Control::CompRelease, ch);
                control(app, ui, "Gain", Control::CompGain, ch);
            });
        });
    });
}

/// The EQ response curve for the channel's current band settings. Bands are
/// modelled as biquads: low/high as shelves, the two mids as peaking filters.
fn eq_curve(app: &App, ui: &mut egui::Ui, ch: u32) {
    let band = |kind: BandType, freq: Control, gain: Control, q: f64| EqBand {
        kind,
        f0: units::freq_hz(freq, app.cached_int(freq, ch)).unwrap_or(1000.0),
        q,
        gain_db: curves::eq_gain_db(app.cached_int(gain, ch)),
    };
    let bands = [
        band(
            BandType::LowShelf,
            Control::EqLowFreq,
            Control::EqLowVolume,
            0.7,
        ),
        band(
            BandType::Peaking,
            Control::EqMidLowFreq,
            Control::EqMidLowVolume,
            curves::q_value(app.cached_int(Control::EqMidLowQ, ch)),
        ),
        band(
            BandType::Peaking,
            Control::EqMidHighFreq,
            Control::EqMidHighVolume,
            curves::q_value(app.cached_int(Control::EqMidHighQ, ch)),
        ),
        band(
            BandType::HighShelf,
            Control::EqHighFreq,
            Control::EqHighVolume,
            0.7,
        ),
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
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .show(ui, |plot| {
            // Fixed scale: input/output -60..0 dB, so the view does not jump as
            // the parameters change.
            plot.set_plot_bounds(PlotBounds::from_min_max([-60.0, -60.0], [0.0, 0.0]));
            plot.line(Line::new(PlotPoints::from(points)));
        });

    if let Some(gr) = app.meters().reduction_db(ch) {
        let fraction = (gr.max(0) as f32 / METER_FULL_SCALE).clamp(0.0, 1.0);
        ui.add(egui::ProgressBar::new(fraction).text("gain reduction"));
    }
}

/// A box title with its enable checkbox right-aligned on the same row.
fn title_row(app: &mut App, ui: &mut egui::Ui, title: &str, enable: Control, ch: u32) {
    ui.horizontal(|ui| {
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut enabled = app.cached_bool(enable, ch);
            if ui.checkbox(&mut enabled, "Enable").changed() {
                app.set(enable, ch, Value::Bool(enabled));
            }
        });
    });
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
                .custom_formatter(move |n, _| human_text(control, n))
                .custom_parser(move |s| parse_human(control, s));
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

/// Format a raw control value in its display units for a slider readout. Thin
/// wrapper over the shared [`units::format`] so the GUI and CLI agree.
pub(crate) fn human_text(control: Control, raw: f64) -> String {
    units::format(control, raw as i32)
}

/// Inverse of [`human_text`]: parse a typed human value back to the raw control
/// value, so editing a value box uses the same units it displays.
pub(crate) fn parse_human(control: Control, text: &str) -> Option<f64> {
    units::parse(control, text).map(f64::from)
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
