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
use egui_plot::{Axis, AxisHints, Line, LineStyle, Plot, PlotPoint, PlotPoints, Polygon, Text};
use tascam_us16x08::{COMP_RATIO_VALUES, Control, Kind, Value, units};

use crate::app::{App, Group};
use crate::bridge;
use crate::curves::{self, BandType, EqBand};

/// Column widths for the INPUT / EQ / COMPRESSOR boxes.
const INPUT_WIDTH: f32 = 190.0;
const DSP_WIDTH: f32 = 320.0;
/// Length of the INPUT volume fader — matched to the meter height beside it.
const VOLUME_FADER_LENGTH: f32 = bridge::METER_HEIGHT;
/// Minimum width for the numeric value boxes, wide enough for the longest
/// readout (e.g. `-127 dB`, `1000 ms`) so they are a fixed, uniform size.
pub(crate) const VALUE_BOX_WIDTH: f32 = 60.0;

/// Headroom (dB) past 0 on the compressor graph axes, reserving a lane to the
/// right of the input-0 line for the gain-reduction bar.
const GR_LANE_MAX: f64 = 6.0;

/// EQ band controls the EQ `Reset` button returns to defaults (a flat curve),
/// excluding the enable switch.
const EQ_RESET: [Control; 10] = [
    Control::EqLowVolume,
    Control::EqLowFreq,
    Control::EqMidLowVolume,
    Control::EqMidLowFreq,
    Control::EqMidLowQ,
    Control::EqMidHighVolume,
    Control::EqMidHighFreq,
    Control::EqMidHighQ,
    Control::EqHighVolume,
    Control::EqHighFreq,
];

/// Compressor controls the Compressor `Reset` button returns to defaults (1:1
/// ratio and no make-up gain, so no compression occurs), excluding the switch.
const COMP_RESET: [Control; 5] = [
    Control::CompThreshold,
    Control::CompRatio,
    Control::CompAttack,
    Control::CompRelease,
    Control::CompGain,
];

/// Render the editor for the currently selected channel.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    let selected = u32::from(app.selected);
    let linked = app.linked(selected);
    let low = selected & !1;
    // When linked, edit/display via the lower channel of the pair.
    let ch = if linked { low } else { selected };

    // Fixed-width numeric value boxes across the editor.
    ui.spacing_mut().interact_size.x = VALUE_BOX_WIDTH;

    // Whole-channel actions: copy/paste this strip, or reset it to defaults.
    ui.horizontal(|ui| {
        copy_paste(app, ui, Group::Channel, "Copy channel", "Paste channel");
        if ui
            .button("Reset channel")
            .on_hover_text("Reset the whole channel to a neutral default")
            .clicked()
        {
            app.reset_channel();
        }
    });

    ui.horizontal_top(|ui| {
        input_box(app, ui, ch, selected, linked, low);
        eq_box(app, ui, ch);
        comp_box(app, ui, ch);
    });
}

/// A Copy button and a Paste button (disabled until something is copied) for a
/// per-channel control group, writing through to the focused channel on paste.
fn copy_paste(app: &mut App, ui: &mut egui::Ui, group: Group, copy: &str, paste: &str) {
    if ui.button(copy).clicked() {
        app.copy_group(group);
    }
    if ui
        .add_enabled(app.has_clip(group), egui::Button::new(paste))
        .clicked()
    {
        app.paste_group(group);
    }
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

            // Editable channel name (GUI-only). Edits the displayed channel (the
            // lower one of a linked pair).
            ui.horizontal(|ui| {
                ui.label("Name");
                let mut name = app.channel_name(ch).to_owned();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("unnamed")
                            .desired_width(INPUT_WIDTH - 52.0),
                    )
                    .changed()
                {
                    app.set_channel_name(ch, name);
                }
            });
            ui.separator();

            // Switches: stereo link on its own row (long label), phase and mute
            // side by side.
            let mut link_on = linked;
            if ui
                .checkbox(&mut link_on, format!("Stereo link {}-{}", low + 1, low + 2))
                .changed()
            {
                app.toggle_link(selected);
            }
            ui.horizontal(|ui| {
                let mut phase = app.cached_bool(Control::PhaseSwitch, ch);
                if ui.checkbox(&mut phase, "Phase").changed() {
                    app.set(Control::PhaseSwitch, ch, Value::Bool(phase));
                }
                ui.add_space(24.0);
                let mut mute = app.cached_bool(Control::MuteSwitch, ch);
                if ui.checkbox(&mut mute, "Mute").changed() {
                    app.set(Control::MuteSwitch, ch, Value::Bool(mute));
                }
            });
            ui.separator();

            // Volume: the channel meter beside the vertical fader (same height),
            // with the dB readout below. The fixed-size region is centred by the
            // surrounding vertical_centered (a plain horizontal would not be).
            ui.vertical_centered(|ui| {
                ui.label("Volume");
                let strip = egui::vec2(VALUE_BOX_WIDTH + 28.0, VOLUME_FADER_LENGTH + 28.0);
                ui.allocate_ui_with_layout(
                    strip,
                    egui::Layout::left_to_right(egui::Align::TOP),
                    |ui| {
                        let level = app.meters().channel_db(ch).unwrap_or(0);
                        bridge::meter_bar(ui, bridge::fraction(level));
                        ui.spacing_mut().slider_width = VOLUME_FADER_LENGTH;
                        // When linked, the fader is the common level (the louder
                        // side); the balance offset between the channels is kept.
                        let mut volume = if linked {
                            app.pair_levels(ch).0
                        } else {
                            app.cached_int(Control::LineVolume, ch)
                        };
                        let fader = egui::Slider::new(&mut volume, 0..=133)
                            .vertical()
                            .custom_formatter(|n, _| human_text(Control::LineVolume, n))
                            .custom_parser(|s| parse_human(Control::LineVolume, s));
                        if ui.add(fader).changed() {
                            if linked {
                                let balance = app.pair_levels(ch).1;
                                app.set_pair_levels(ch, volume, balance);
                            } else {
                                app.set(Control::LineVolume, ch, Value::Int(volume));
                            }
                        }
                    },
                );
            });
            ui.separator();

            // Balance for a linked pair, or Pan for a single channel: label above
            // a full-width slider. The pair stays panned hard L/R; Balance
            // attenuates one channel's level (see `App::set_pair_levels`).
            ui.label(if linked { "Balance" } else { "Pan" });
            ui.spacing_mut().slider_width = INPUT_WIDTH - VALUE_BOX_WIDTH - 24.0;
            if linked {
                let (common, mut balance) = app.pair_levels(ch);
                let slider = egui::Slider::new(&mut balance, 0..=254)
                    .custom_formatter(|n, _| human_text(Control::Pan, n))
                    .custom_parser(|s| parse_human(Control::Pan, s));
                if ui.add(slider).changed() {
                    app.set_pair_levels(ch, common, balance);
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
}

/// The EQ box: response graph on top, then the bands as a Gain / Freq / Q grid
/// (one row per band, high at the top). The low and high bands are shelves with
/// no Q, so those cells are blank.
fn eq_box(app: &mut App, ui: &mut egui::Ui, ch: u32) {
    /// `(label, gain, freq, optional Q)` per band, high to low.
    const BANDS: [(&str, Control, Control, Option<Control>); 4] = [
        ("High", Control::EqHighVolume, Control::EqHighFreq, None),
        (
            "Mid-high",
            Control::EqMidHighVolume,
            Control::EqMidHighFreq,
            Some(Control::EqMidHighQ),
        ),
        (
            "Mid-low",
            Control::EqMidLowVolume,
            Control::EqMidLowFreq,
            Some(Control::EqMidLowQ),
        ),
        ("Low", Control::EqLowVolume, Control::EqLowFreq, None),
    ];

    ui.group(|ui| {
        ui.set_width(DSP_WIDTH);
        ui.vertical(|ui| {
            title_row(app, ui, "EQ", Group::Eq, Control::EqSwitch, &EQ_RESET, ch);
            eq_curve(app, ui, ch);
            egui::Grid::new("eq_grid").num_columns(4).show(ui, |ui| {
                ui.label("");
                ui.label("Gain");
                ui.label("Freq");
                ui.label("Q");
                ui.end_row();

                for (label, gain, freq, q) in BANDS {
                    ui.label(label);
                    drag_value(app, ui, gain, ch);
                    drag_value(app, ui, freq, ch);
                    match q {
                        Some(q) => drag_value(app, ui, q, ch),
                        None => {
                            ui.label("");
                        }
                    }
                    ui.end_row();
                }
            });
        });
    });
}

/// The COMPRESSOR box: transfer graph and GR on top, then the parameters.
fn comp_box(app: &mut App, ui: &mut egui::Ui, ch: u32) {
    ui.group(|ui| {
        ui.set_width(DSP_WIDTH);
        ui.vertical(|ui| {
            title_row(
                app,
                ui,
                "Compressor",
                Group::Comp,
                Control::CompSwitch,
                &COMP_RESET,
                ch,
            );
            comp_curve(app, ui, ch);
            // Threshold / Ratio / Gain on the first row, Attack / Release on the
            // second. Ratio is an enum, so it stays a dropdown.
            egui::Grid::new("comp_grid").num_columns(3).show(ui, |ui| {
                comp_cell(app, ui, "Threshold", Control::CompThreshold, ch);
                comp_cell(app, ui, "Ratio", Control::CompRatio, ch);
                comp_cell(app, ui, "Gain", Control::CompGain, ch);
                ui.end_row();
                comp_cell(app, ui, "Attack", Control::CompAttack, ch);
                comp_cell(app, ui, "Release", Control::CompRelease, ch);
                ui.label("");
                ui.end_row();
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

    // When the EQ is disabled (or the DSP is bypassed) it does nothing, so show
    // a flat response rather than the inactive band settings.
    let active = app.cached_bool(Control::EqSwitch, ch) && !app.cached_bool(Control::DspBypass, 0);

    // x is log10(Hz) over ~20 Hz .. 20 kHz.
    let points: Vec<[f64; 2]> = (0..=200)
        .map(|i| {
            let lf = 1.3 + (4.3 - 1.3) * (f64::from(i) / 200.0);
            let db = if active {
                curves::eq_response_db(&bands, 10f64.powf(lf))
            } else {
                0.0
            };
            [lf, db]
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
        .y_axis_formatter(|mark, _| format!("{:.0} dB", mark.value))
        .show(ui, |plot| plot.line(Line::new(PlotPoints::from(points))));
}

/// The compressor transfer curve (input dB -> output dB) plus a GR meter.
fn comp_curve(app: &App, ui: &mut egui::Ui, ch: u32) {
    let threshold = f64::from(app.cached_int(Control::CompThreshold, ch) - 32);
    let ratio = COMP_RATIO_VALUES
        .get(usize::try_from(app.cached_int(Control::CompRatio, ch)).unwrap_or(0))
        .map_or(1.0, |label| curves::ratio_from_label(label));

    // When the compressor is disabled (or the DSP is bypassed) it does nothing,
    // so show a 1:1 line and zero gain reduction rather than stale values.
    let active =
        app.cached_bool(Control::CompSwitch, ch) && !app.cached_bool(Control::DspBypass, 0);

    let points: Vec<[f64; 2]> = (0..=60)
        .map(|i| {
            let input = -60.0 + f64::from(i);
            // The curve shows the compression characteristic only; make-up gain
            // is a flat output trim and would just shift (and clip) the curve.
            let output = if active {
                curves::comp_output_db(input, threshold, ratio, 0.0)
            } else {
                input
            };
            [input, output]
        })
        .collect();

    // Gain-reduction level (0..=1), zero when inactive.
    let gr = if active {
        app.meters().gain_reduction(ch).unwrap_or(0.0)
    } else {
        0.0
    };

    Plot::new("comp_curve")
        // Square: input and output share the -60..0 dB range, so the 1:1
        // diagonal is at 45 degrees and the y ticks have room to render.
        .height(DSP_WIDTH)
        .width(DSP_WIDTH)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        // Fix the scale to -60..0 dB on both axes via the builder (not
        // set_plot_bounds in the closure) so the axis tick labels render. Keep
        // the default margin so the edge ticks are not clipped.
        // A little headroom past 0 dB on both axes (kept equal so the diagonal
        // stays ~45 degrees) gives the gain-reduction bar its own lane to the
        // right of the input-0 line, clear of the transfer curve.
        .include_x(-60.0)
        .include_x(GR_LANE_MAX)
        .include_y(-60.0)
        .include_y(GR_LANE_MAX)
        // Bare-number ticks (narrow, so more of them fit) with the dB unit on
        // the axis name; the positive headroom is unlabelled.
        .custom_x_axes(vec![
            AxisHints::new(Axis::X)
                .label("input dB")
                .formatter(db_tick)
                .label_spacing(egui::Rangef::new(24.0, 48.0)),
        ])
        .custom_y_axes(vec![
            AxisHints::new(Axis::Y)
                .label("output dB")
                .formatter(db_tick)
                .min_thickness(24.0)
                .label_spacing(egui::Rangef::new(8.0, 14.0)),
        ])
        .show(ui, |plot| {
            // Transfer curve as a region filled down to the graph floor.
            plot.line(
                Line::new(PlotPoints::from(points))
                    .color(egui::Color32::from_rgb(90, 170, 220))
                    .fill(-60.0),
            );
            // 1:1 reference diagonal (input == output), on top of the fill so it
            // stays visible.
            plot.line(
                Line::new(PlotPoints::from(vec![[-60.0, -60.0], [0.0, 0.0]]))
                    .color(egui::Color32::from_gray(110))
                    .style(LineStyle::dashed_loose()),
            );

            // Gain-reduction meter: a vertical bar in the headroom lane to the
            // right of the input-0 line, growing down from 0 dB.
            let depth = -60.0 * f64::from(gr);
            let bar = Polygon::new(PlotPoints::from(vec![
                [1.5, 0.0],
                [GR_LANE_MAX, 0.0],
                [GR_LANE_MAX, depth],
                [1.5, depth],
            ]))
            .fill_color(egui::Color32::from_rgba_unmultiplied(230, 120, 60, 200))
            .stroke(egui::Stroke::NONE);
            plot.polygon(bar);
            plot.text(Text::new(PlotPoint::new(3.7, 3.0), "GR"));
        });
}

/// A box title row: the title and Copy/Paste buttons for `group`, then a
/// right-aligned `Enable` checkbox with a `Reset` button to its left. Reset
/// returns `reset` controls to their defaults.
fn title_row(
    app: &mut App,
    ui: &mut egui::Ui,
    title: &str,
    group: Group,
    enable: Control,
    reset: &[Control],
    ch: u32,
) {
    ui.horizontal(|ui| {
        ui.heading(title);
        copy_paste(app, ui, group, "Copy", "Paste");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut enabled = app.cached_bool(enable, ch);
            if ui.checkbox(&mut enabled, "Enable").changed() {
                app.set(enable, ch, Value::Bool(enabled));
            }
            // Placed after the checkbox so it sits to its left in this layout.
            if ui.button("Reset").clicked() {
                reset_controls(app, reset, ch);
            }
        });
    });
}

/// Set each control to its catalog default. Used by the section Reset buttons;
/// EQ defaults give a flat response and compressor defaults give no compression.
fn reset_controls(app: &mut App, controls: &[Control], ch: u32) {
    for &control in controls {
        // The reset lists hold only int/enum parameters, not switches or meters.
        let value = match control.kind() {
            Kind::Int { default, .. } => Value::Int(default),
            Kind::Enum { default, .. } => Value::Enum(default),
            _ => continue,
        };
        app.set(control, ch, value);
    }
}

/// Render one control as the widget its kind calls for, writing through on edit.
fn comp_cell(app: &mut App, ui: &mut egui::Ui, label: &str, control: Control, index: u32) {
    ui.vertical(|ui| {
        ui.label(label);
        match control.kind() {
            Kind::Int { .. } => drag_value(app, ui, control, index),
            Kind::Enum { .. } => enum_combo(app, ui, control, index),
            // The compressor grid holds only int and enum parameters.
            _ => {}
        }
    });
}

/// Render an enum control as a dropdown of its labels.
fn enum_combo(app: &mut App, ui: &mut egui::Ui, control: Control, index: u32) {
    let Kind::Enum { values, .. } = control.kind() else {
        return;
    };
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

/// Render one integer control as a bare drag-value cell (no label, no row end),
/// in its display units. Used by the EQ band grid.
fn drag_value(app: &mut App, ui: &mut egui::Ui, control: Control, index: u32) {
    let Kind::Int { min, max, .. } = control.kind() else {
        return;
    };
    let mut value = app.cached_int(control, index);
    let widget = egui::DragValue::new(&mut value)
        .range(min..=max)
        .custom_formatter(move |n, _| human_text(control, n))
        .custom_parser(move |s| parse_human(control, s));
    if ui.add(widget).changed() {
        app.set(control, index, Value::Int(value));
    }
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

/// Compressor axis tick label: the dB value, leaving the positive
/// gain-reduction headroom unlabelled.
fn db_tick(mark: egui_plot::GridMark, _range: &std::ops::RangeInclusive<f64>) -> String {
    if mark.value > 0.5 {
        String::new()
    } else {
        format!("{:.0}", mark.value)
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
