//! The patch-librarian / level-balancer application.
//!
//! One screen: a list of all 100 user patches, each with an editable name and an
//! output-level slider. Clicking a row's id auditions the patch (writes it into
//! the current sound); editing the name or dragging the slider holds a pending
//! change. Each row's Save stores just that patch and Revert drops the edits back
//! to the on-unit values, while "Write changes to unit" stores all pending changes
//! at once. Storing to memory requires the GX-700 in front-panel BULK LOAD mode.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use egui_plot::{Line, LineStyle, Plot, PlotBounds, PlotPoints};
use rackctl_gx700::param::{EQ_MID_FREQ_VALUES, EQ_MID_Q_VALUES};
use rackctl_gx700::typed::Patch as TypedPatch;
use rackctl_gx700::{Block, Kind, NAME_LEN, Param, RawPatch, Value, param, units};
use rackctl_ui::comp::output_db as comp_output_db;
use rackctl_ui::eq::{BandType, EqBand, eq_response_db};
use rackctl_ui::{ActionKind, action_button};

use crate::config::{self, CachedRow, GuiConfig};
use crate::device::{self, Device, SharedDevice};
use crate::loader::{Loaded, Loader, PRESET_END, PRESET_SLOTS, PRESET_START, USER_SLOTS};
use crate::prober::{Probe, Prober};
use crate::writer::{Writer, Written};

/// Reopen the device on demand (the Retry button), e.g. after the port appears.
pub(crate) type Reopen = Box<dyn Fn() -> anyhow::Result<Device>>;

/// User slot the startup BULK LOAD probe writes-and-restores (see
/// [`rackctl_gx700::Gx700::probe_bulk_load`]). Its name is briefly renamed and
/// restored; in Play mode the write is a no-op, so the slot is untouched.
const PROBE_SLOT: u16 = 1;
/// Seconds between automatic BULK LOAD re-probes while waiting for the mode. Kept
/// fairly long so the probe's MIDI traffic doesn't make the unit's menu sluggish.
const PROBE_INTERVAL: f64 = 5.0;

/// One patch in the librarian list.
struct PatchRow {
    slot: u16,
    /// Patch name as stored on the unit (committed).
    name: String,
    /// The editable name buffer; differs from `name` while the user is editing.
    name_edit: String,
    /// Output level as stored on the unit (committed).
    stored_level: u8,
    /// Chain order bytes (read with the header; not edited in this view).
    chain: Vec<u8>,
    /// The full patch, loaded the first time the row is auditioned/edited. The GUI
    /// works in the typed model; raw bytes only appear at the device boundary.
    full: Option<TypedPatch>,
    /// A live-edited level not yet written to memory.
    pending_level: Option<u8>,
    /// A staged whole-patch replacement (from Paste or Clear) not yet written.
    /// When set, it — not `full` — is the basis for the next store.
    pending_patch: Option<TypedPatch>,
    /// Set when the bank read for this slot was skipped after exhausting retries;
    /// its name/level are stale (or empty). Cleared once a read succeeds.
    failed: bool,
}

impl PatchRow {
    /// An empty row for `slot`, before its header is read.
    fn empty(slot: u16) -> Self {
        Self {
            slot,
            name: String::new(),
            name_edit: String::new(),
            stored_level: 0,
            chain: Vec::new(),
            full: None,
            pending_level: None,
            pending_patch: None,
            failed: false,
        }
    }

    /// Whether the row has unsaved edits (a level, name, or whole-patch change).
    fn dirty(&self) -> bool {
        self.pending_level.is_some() || self.name_edit != self.name || self.pending_patch.is_some()
    }
}

/// Sentinel `edit_slot` value meaning "the Edit tab is editing the offline scratch
/// patch", not a real bank slot. Real slots are `1..=200`, so `0` is free.
const SCRATCH: u16 = 0;

// Nerd Font glyphs for the per-row list buttons, shared with the other Rackctl
// GUIs (rendered by the vendored `JetBrains Mono Nerd Font`; each button keeps a
// text tooltip, so the icon never stands alone).
use rackctl_ui::icon;

/// Where an offline edit (the Edit tab's scratch patch) is saved back to.
#[derive(Clone)]
enum OfflineSource {
    /// A slot in the scene composer.
    Composer(usize),
    /// A named patch in the on-disk patch library.
    Library(String),
}

/// Drag-and-drop payload in the Scene tab: either a patch dragged from the library
/// palette (assign), or a composer slot dragged onto another slot (re-order). One
/// payload type so a single `dnd_drop_zone` per row handles both.
#[derive(Clone)]
enum SceneDrag {
    /// A patch-library file name dragged from the palette onto a slot.
    Lib(String),
    /// A live device-bank slot dragged from the palette onto a scene slot.
    Bank(u16),
    /// A composer slot index dragged onto another slot to re-order.
    Slot(usize),
}

/// A UI interaction to apply after the render pass (avoids borrowing `self`
/// mutably while iterating the rows).
enum Action {
    Audition(u16),
    SetLevel(u16, u8),
    SetParam(u16, &'static str, Value),
    SelectTab(Tab),
    SelectBlock(Block),
    SelectAssign(usize),
    CopyBlock(Block),
    PasteBlock(u16, Block),
    RevertBlock(u16, Block),
    ReorderChain(u16, usize, usize),
    ReorderPatch(u16, u16),
    SetName(u16, String),
    SaveRow(u16),
    RevertRow(u16),
    CopyRow(u16),
    PasteRow(u16),
    ClearRow(u16),
    SavePatchLib,
    LoadPatchLib(String),
    CopyPatchLib(String),
    SavePatchOver(String),
    PastePatchLib,
    CopyScene(String),
    PasteScene,
    ComposeNew,
    ComposeCapture,
    ComposeLoad(String),
    ComposeAssign(usize, String),
    ComposeClear(usize),
    ComposeReorder(usize, usize),
    ComposeAssignBank(usize, u16),
    ComposeCopy(usize),
    ComposePaste(usize),
    ComposeRevert(usize),
    ComposeSave,
    ComposeSaveOver(String),
    ComposeApply,
    EditComposerSlot(usize),
    EditLibraryPatch(String),
    EditDevicePatch(u16),
    SaveOfflineEdit,
    CloseOfflineEdit,
    ToggleBlockLib,
    SaveBlockPreset(String),
    LoadBlockPreset(String),
    CopyBlockPreset(String),
    PasteBlockPreset(String),
    RequestDelete(std::path::PathBuf),
    ConfirmDelete,
    CancelDelete,
    Refresh,
    ConfirmRefresh,
    CancelRefresh,
    Retry,
    OpenBulkPrompt,
    CloseBulkPrompt,
    WriteAll,
    ProbeBulk,
}

/// Which screen is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    /// The patch librarian / level balancer.
    Patches,
    /// The per-block effect-parameter editor.
    Edit,
    /// The factory-preset browser (load a preset into the active sound).
    Presets,
    /// The on-disk library of saved patches and effect blocks.
    Library,
    /// The offline scene composer — build a whole-bank scene from the patch library.
    Scene,
}

impl Tab {
    /// Stable key for persistence (see `config::GuiConfig::tab`). `Edit` returns
    /// `None` — it's a transient screen entered via a per-patch Edit button, not a
    /// destination to restore to.
    fn as_key(self) -> Option<&'static str> {
        match self {
            Tab::Patches => Some("patches"),
            Tab::Presets => Some("presets"),
            Tab::Library => Some("library"),
            Tab::Scene => Some("scene"),
            Tab::Edit => None,
        }
    }

    /// The tab for a persisted key, or `None` if unknown.
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "patches" => Some(Tab::Patches),
            "presets" => Some(Tab::Presets),
            "library" => Some(Tab::Library),
            "scene" => Some(Tab::Scene),
            _ => None,
        }
    }
}

/// Parse a saved patch file: a typed `Patch` (the readable, grouped-by-block form
/// the GUI writes) — checked for device + version — or, as a fallback, a bare typed
/// `Patch` or a raw `RawPatch` (the CLI's / older form). `Err` with a reason (e.g.
/// from a different device) when the file is recognisable but unusable.
fn parse_patch_text(text: &str) -> Result<TypedPatch, String> {
    if let Some(res) = config::load_item::<TypedPatch>(text) {
        return res;
    }
    if let Ok(typed) = serde_json::from_str::<TypedPatch>(text) {
        return Ok(typed);
    }
    serde_json::from_str::<RawPatch>(text)
        .map(|raw| TypedPatch::from_raw(&raw))
        .map_err(|_| "unrecognised patch file".to_owned())
}

/// Parse a saved block preset: our envelope or a bare `BlockData`.
fn parse_block_text(text: &str) -> Result<rackctl_gx700::typed::BlockData, String> {
    if let Some(res) = config::load_item(text) {
        return res;
    }
    serde_json::from_str(text).map_err(|_| "unrecognised block file".to_owned())
}

/// Move the item at index `from` to index `to`, shifting the items in between (the
/// drag-to-reorder operation). Out-of-range or no-op moves leave the vec untouched.
fn move_within<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from == to || from >= items.len() || to >= items.len() {
        return;
    }
    let item = items.remove(from);
    items.insert(to, item);
}

/// Parse a saved scene (a whole bank): our envelope or a bare patch array.
fn parse_scene_text(text: &str) -> Result<Vec<TypedPatch>, String> {
    if let Some(res) = config::load_item(text) {
        return res;
    }
    serde_json::from_str(text).map_err(|_| "unrecognised scene file".to_owned())
}

/// Render a library list: each saved `name` with a Load (gated on `can_load`) and
/// a Delete button. `make_load` builds the load action; Delete requests a confirm.
#[allow(clippy::too_many_arguments)]
fn lib_list(
    ui: &mut egui::Ui,
    names: &[String],
    empty_msg: &str,
    can_use: bool,
    load_hover: &str,
    dir: Option<&std::path::Path>,
    make_load: impl Fn(String) -> Action,
    make_save: impl Fn(String) -> Action,
    make_copy: impl Fn(String) -> Action,
    make_edit: Option<fn(String) -> Action>,
    actions: &mut Vec<Action>,
) {
    ui.add_space(4.0);
    if names.is_empty() {
        ui.label(egui::RichText::new(empty_msg).weak());
        return;
    }
    for name in names {
        // Canonical action order (shared by every list): Edit, Load, Save, Copy,
        // Delete. (Edit is offline-capable, so it stays enabled regardless of
        // `can_use`; Delete only needs a directory.)
        ui.horizontal(|ui| {
            if let Some(make_edit) = make_edit
                && action_button(ui, icon::EDIT, ActionKind::Read)
                    .on_hover_text("edit this patch offline (no device)")
                    .clicked()
            {
                actions.push(make_edit(name.clone()));
            }
            ui.add_enabled_ui(can_use, |ui| {
                if action_button(ui, icon::LOAD, ActionKind::Read)
                    .on_hover_text(load_hover)
                    .clicked()
                {
                    actions.push(make_load(name.clone()));
                }
                if action_button(ui, icon::SAVE, ActionKind::Commit)
                    .on_hover_text("overwrite this with the current one")
                    .clicked()
                {
                    actions.push(make_save(name.clone()));
                }
                if action_button(ui, icon::COPY, ActionKind::Read)
                    .on_hover_text("copy this to the clipboard")
                    .clicked()
                {
                    actions.push(make_copy(name.clone()));
                }
            });
            if action_button(ui, icon::DELETE, ActionKind::Destructive)
                .on_hover_text("delete this from the library")
                .clicked()
                && let Some(d) = dir
            {
                actions.push(Action::RequestDelete(
                    d.join(format!("{}.json", config::sanitize(name))),
                ));
            }
            ui.label(name);
        });
    }
}

/// Stable on-disk subdirectory name for a block's preset library.
fn block_dir_name(block: Block) -> &'static str {
    match block {
        Block::Compressor => "compressor",
        Block::Wah => "wah",
        Block::Distortion => "distortion",
        Block::Preamp => "preamp",
        Block::Loop => "loop",
        Block::Equalizer => "equalizer",
        Block::SpeakerSim => "speaker_sim",
        Block::NoiseSuppressor => "noise_suppressor",
        Block::Modulation => "modulation",
        Block::Delay => "delay",
        Block::Chorus => "chorus",
        Block::TremoloPan => "tremolo_pan",
        Block::Reverb => "reverb",
        Block::LevelChain => "level_chain",
        _ => "other",
    }
}

/// The preset-library directory for `block` (`<settings>/blocks/<type>`), so each
/// effect type's presets live in their own folder.
fn block_presets_dir(block: Block) -> Option<std::path::PathBuf> {
    config::blocks_dir().map(|d| d.join(block_dir_name(block)))
}

/// Display label for a slot: `U001..U100` for user patches, `P001..P100` for the
/// factory presets (slots `101..=200`).
fn slot_label(slot: u16) -> String {
    if slot <= USER_SLOTS {
        format!("U{slot:03}")
    } else {
        format!("P{:03}", slot - USER_SLOTS)
    }
}

/// A block's enable parameter (its offset-0 bool), if any.
fn block_enable_param(block: Block) -> Option<Param> {
    param::ALL
        .iter()
        .copied()
        .find(|p| p.block() == block && p.offset() == 0 && matches!(p.kind(), Kind::Bool))
}

/// Whether `block`'s enable byte (its offset-0 bool) is on in `typed`.
fn block_enabled(typed: &TypedPatch, block: Block) -> bool {
    block_enable_param(block)
        .and_then(|p| typed.get(p.key()))
        .is_some_and(|v| matches!(v, Value::Bool(true)))
}

/// Draw the GX-700 3-band EQ response curve for the patch's current settings.
/// Low/high shelf corner frequencies are fixed on the device (not published), so
/// the absolute low/high Hz are indicative; the mid band and gains are exact.
fn show_eq_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    // EQ gains are raw 0..40 centred at 20 = 0 dB.
    let gain = |k: &str| f64::from(raw(k) - 20);
    let mid_freq = EQ_MID_FREQ_VALUES
        .get(usize::try_from(raw("eq-mid-freq")).unwrap_or(0))
        .map_or(1000.0, |s| hz_from_label(s));
    let q = EQ_MID_Q_VALUES
        .get(usize::try_from(raw("eq-mid-q")).unwrap_or(0))
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.0);
    let level = gain("eq-level");
    let active = matches!(typed.get("eq-enable"), Some(Value::Bool(true)));
    let bands = [
        EqBand {
            kind: BandType::LowShelf,
            f0: 100.0,
            q: 0.7,
            gain_db: gain("eq-low-gain"),
        },
        EqBand {
            kind: BandType::Peaking,
            f0: mid_freq,
            q,
            gain_db: gain("eq-mid-gain"),
        },
        EqBand {
            kind: BandType::HighShelf,
            f0: 8000.0,
            q: 0.7,
            gain_db: gain("eq-high-gain"),
        },
    ];
    // x is log10(Hz) over ~20 Hz .. 20 kHz; the output level shifts the whole curve.
    let points: Vec<[f64; 2]> = (0..=200)
        .map(|i| {
            let lf = 1.3 + (4.3 - 1.3) * (f64::from(i) / 200.0);
            let db = if active {
                eq_response_db(&bands, 10f64.powf(lf)) + level
            } else {
                0.0
            };
            [lf, db]
        })
        .collect();
    Plot::new("gx700-eq")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_y(-24.0)
        .include_y(24.0)
        .x_axis_formatter(|mark, _| hz_label(mark.value))
        .y_axis_formatter(|mark, _| format!("{:.0} dB", mark.value))
        .show(ui, |plot| plot.line(Line::new(PlotPoints::from(points))));
}

/// Draw an *indicative* compressor transfer curve (input dB -> output dB). The
/// GX-700 does not publish a threshold in dB or a ratio, so the mapping is
/// approximate: in Limiter mode the threshold byte sets a near-hard limit knee; in
/// Compressor mode the sustain byte scales the ratio at a fixed knee. It shows how
/// the controls reshape the response, not exact numbers.
fn show_comp_curve(ui: &mut egui::Ui, typed: &TypedPatch, limiter: bool) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("comp-enable"), Some(Value::Bool(true)));
    let (threshold_db, ratio) = if limiter {
        // threshold byte 0..100 -> -40..0 dB; near-hard limiting above it.
        (f64::from(raw("comp-threshold")).mul_add(0.4, -40.0), 20.0)
    } else {
        // sustain byte 0..100 -> ratio 1:1..8:1 at a fixed -30 dB knee.
        (-30.0, f64::from(raw("comp-sustain")) / 100.0 * 7.0 + 1.0)
    };
    let points: Vec<[f64; 2]> = (0..=60)
        .map(|i| {
            let input = -60.0 + f64::from(i);
            let output = if active {
                comp_output_db(input, threshold_db, ratio, 0.0)
            } else {
                input
            };
            [input, output]
        })
        .collect();
    Plot::new("gx700-comp")
        .height(170.0)
        .width(170.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_x(-60.0)
        .include_x(2.0)
        .include_y(-60.0)
        .include_y(2.0)
        .x_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .y_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .show(ui, |plot| {
            plot.line(
                Line::new(PlotPoints::from(points)).color(egui::Color32::from_rgb(90, 170, 220)),
            );
            // 1:1 reference diagonal (input == output).
            plot.line(
                Line::new(PlotPoints::from(vec![[-60.0, -60.0], [0.0, 0.0]]))
                    .color(egui::Color32::from_gray(110))
                    .style(LineStyle::dashed_loose()),
            );
        });
}

/// Draw an *indicative* noise-gate response (input level -> output level, both the
/// relative `0..100` the device uses). Below the threshold the gate closes and the
/// signal is suppressed toward silence; at/above it the signal passes. The knee is
/// approximate (the GX-700 publishes no dB), so this shows the *shape*, not numbers.
fn show_ns_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("ns-enable"), Some(Value::Bool(true)));
    let threshold = f64::from(raw("ns-threshold")); // 0..100
    let knee = 12.0;
    let points: Vec<[f64; 2]> = (0..=100)
        .map(|i| {
            let input = f64::from(i);
            let output = if active {
                // Gain ramps 0 -> 1 across [threshold - knee, threshold] (smoothstep).
                let g = ((input - (threshold - knee)) / knee).clamp(0.0, 1.0);
                input * (g * g * (3.0 - 2.0 * g))
            } else {
                input
            };
            [input, output]
        })
        .collect();
    Plot::new("gx700-ns")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_x(0.0)
        .include_x(100.0)
        .include_y(0.0)
        .include_y(100.0)
        .x_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .y_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .show(ui, |plot| {
            plot.line(
                Line::new(PlotPoints::from(points)).color(egui::Color32::from_rgb(90, 170, 220)),
            );
            // 1:1 reference (fully open) diagonal.
            plot.line(
                Line::new(PlotPoints::from(vec![[0.0, 0.0], [100.0, 100.0]]))
                    .color(egui::Color32::from_gray(110))
                    .style(LineStyle::dashed_loose()),
            );
            // Threshold marker.
            if active {
                plot.line(
                    Line::new(PlotPoints::from(vec![[threshold, 0.0], [threshold, 100.0]]))
                        .color(egui::Color32::from_rgb(170, 100, 100))
                        .style(LineStyle::dashed_dense()),
                );
            }
        });
}

/// Draw an *indicative* reverb decay envelope (level over time): the dry signal as
/// a spike at t=0 (its height = Direct level), then after the pre-delay gap the wet
/// tail starting at the Effect level and decaying over the reverb Time. Tonal
/// controls (mode, cuts, diffusion) aren't shown; this is the shape, not a measure.
fn show_reverb_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("reverb-enable"), Some(Value::Bool(true)));
    let time_s = f64::from(raw("reverb-time").max(1)) / 10.0; // 0.1..10.0 s
    let pre_s = f64::from(raw("reverb-pre-delay")) / 1000.0; // 0..0.1 s
    let effect = f64::from(raw("reverb-effect-level")); // 0..100 (wet tail height)
    let direct = f64::from(raw("reverb-direct-level")); // 0..100 (dry spike height)
    // Fixed 3-second window, so the axis doesn't move as the reverb Time changes.
    let span = 3.0;
    // Tail decays to ~5% of its start by the end of the reverb Time.
    let tau = (time_s / 3.0).max(0.01);
    let tail: Vec<[f64; 2]> = (0..=180)
        .map(|i| {
            let t = pre_s + (time_s).min(span) * (f64::from(i) / 180.0);
            let level = if active {
                effect * (-(t - pre_s) / tau).exp()
            } else {
                0.0
            };
            [t, level]
        })
        .collect();
    Plot::new("gx700-reverb")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .x_axis_formatter(|mark, _| format!("{:.1}s", mark.value))
        .y_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .show(ui, |plot| {
            // Pin the view to 0..3 s × 0..100, regardless of the reverb Time.
            plot.set_plot_bounds(PlotBounds::from_min_max([0.0, 0.0], [span, 100.0]));
            // Dry signal: a spike at t=0 whose height is the Direct level.
            plot.line(
                Line::new(PlotPoints::from(vec![[0.0, 0.0], [0.0, direct]]))
                    .color(egui::Color32::from_gray(150)),
            );
            // Wet tail: starts after the pre-delay at the Effect level, then decays.
            plot.line(
                Line::new(PlotPoints::from(tail)).color(egui::Color32::from_rgb(90, 170, 220)),
            );
        });
}

/// A *hand-picked* clipping "hardness" (0 = smooth overdrive .. 1 = hard, squared
/// fuzz) for each `dist-type`, by ear/reputation only — the GX-700 exposes nothing
/// about each type's real transfer function. Purely to make the schematic curve
/// differ by type; replace with measured shapes if we ever capture them.
fn dist_hardness(type_index: i32) -> f64 {
    match type_index {
        0 => 0.10, // Vintage OD
        1 => 0.30, // Turbo OD
        2 => 0.12, // Blues
        3 => 0.45, // Distortion
        4 => 0.60, // Turbo DS
        5 => 0.78, // Metal
        6 => 0.95, // Fuzz
        _ => 0.40,
    }
}

/// Draw a *schematic* distortion transfer curve (input -> output, normalised ±1):
/// a waveshaper that steepens with Drive and squares off more for the harder types
/// (Metal/Fuzz) than the overdrives. The per-type hardness is hand-picked, not
/// measured (see [`dist_hardness`]), and tone/level aren't modelled — it conveys
/// the *idea*, not the device's real clipping.
fn show_dist_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("dist-enable"), Some(Value::Bool(true)));
    let drive = f64::from(raw("dist-drive")); // 0..100
    let hardness = dist_hardness(raw("dist-type"));
    // Harder types reach more gain and lean toward a hard clip; softer ones stay
    // near a smooth tanh saturation.
    let gain = 1.0 + drive / 100.0 * (6.0 + hardness * 10.0);
    let norm = gain.tanh(); // keep the soft curve's ends at ±1
    let points: Vec<[f64; 2]> = (0..=120)
        .map(|i| {
            let x = -1.0 + 2.0 * (f64::from(i) / 120.0);
            let y = if active {
                let soft = (gain * x).tanh() / norm;
                let hard = (gain * x).clamp(-1.0, 1.0);
                soft * (1.0 - hardness) + hard * hardness
            } else {
                x
            };
            [x, y]
        })
        .collect();
    Plot::new("gx700-dist")
        .height(150.0)
        .data_aspect(1.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_x(-1.0)
        .include_x(1.0)
        .include_y(-1.0)
        .include_y(1.0)
        .x_axis_formatter(|_, _| String::new())
        .y_axis_formatter(|_, _| String::new())
        .show(ui, |plot| {
            plot.line(
                Line::new(PlotPoints::from(points)).color(egui::Color32::from_rgb(90, 170, 220)),
            );
            // 1:1 reference (clean / no clipping) diagonal.
            plot.line(
                Line::new(PlotPoints::from(vec![[-1.0, -1.0], [1.0, 1.0]]))
                    .color(egui::Color32::from_gray(110))
                    .style(LineStyle::dashed_loose()),
            );
        });
}

/// Draw an *indicative* preamp tone-stack response from the Bass / Middle / Treble
/// / Presence controls (each 0..100, 50 ≈ flat) plus the Bright switch. The band
/// frequencies are nominal and the real amp models interact differently, so this
/// shows roughly how the tone controls tilt the voicing, not an exact response.
fn show_preamp_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("preamp-enable"), Some(Value::Bool(true)));
    let bright = matches!(typed.get("preamp-bright"), Some(Value::Bool(true)));
    // 0..100 control -> ±span dB, centred at 50.
    let g = |k: &str, span: f64| (f64::from(raw(k)) - 50.0) / 50.0 * span;
    let bands = [
        EqBand {
            kind: BandType::LowShelf,
            f0: 100.0,
            q: 0.7,
            gain_db: g("preamp-bass", 12.0),
        },
        EqBand {
            kind: BandType::Peaking,
            f0: 600.0,
            q: 0.7,
            gain_db: g("preamp-middle", 12.0),
        },
        EqBand {
            kind: BandType::HighShelf,
            f0: 3000.0,
            q: 0.7,
            gain_db: g("preamp-treble", 12.0),
        },
        EqBand {
            kind: BandType::HighShelf,
            f0: 6000.0,
            q: 0.7,
            gain_db: g("preamp-presence", 9.0),
        },
        EqBand {
            kind: BandType::HighShelf,
            f0: 2000.0,
            q: 0.7,
            gain_db: if bright { 4.0 } else { 0.0 },
        },
    ];
    let points: Vec<[f64; 2]> = (0..=200)
        .map(|i| {
            let lf = 1.3 + (4.3 - 1.3) * (f64::from(i) / 200.0);
            let db = if active {
                eq_response_db(&bands, 10f64.powf(lf))
            } else {
                0.0
            };
            [lf, db]
        })
        .collect();
    Plot::new("gx700-preamp")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_y(-18.0)
        .include_y(18.0)
        .x_axis_formatter(|mark, _| hz_label(mark.value))
        .y_axis_formatter(|mark, _| format!("{:.0} dB", mark.value))
        .show(ui, |plot| plot.line(Line::new(PlotPoints::from(points))));
}

/// Center-tap delay time in milliseconds: a literal time in Normal mode, or derived
/// from the tempo (BPM) and the note interval in Tempo mode.
fn delay_center_ms(typed: &TypedPatch) -> f64 {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    if matches!(typed.get("delay-mode"), Some(Value::Enum(1))) {
        let bpm = f64::from(raw("delay-tempo") + 50).max(50.0); // raw 0..250 = 50..300
        // Note-value fractions of a beat (1/4), matching DELAY_INTERVAL_VALUES.
        let frac = [
            0.25, 0.333_333, 0.375, 0.5, 0.666_667, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0,
        ]
        .get(usize::try_from(raw("delay-interval-c")).unwrap_or(0))
        .copied()
        .unwrap_or(1.0);
        60_000.0 / bpm * frac
    } else {
        f64::from(raw("delay-time-c")) // already in ms
    }
}

/// Draw the delay as a *tap diagram*: a dry spike at t=0, the Left/Right taps at
/// their times (a % of the centre time) and levels, and the centre tap echoing
/// every centre-time, decaying by the feedback. Heights are the per-tap levels.
fn show_delay_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("delay-enable"), Some(Value::Bool(true)));
    let c = delay_center_ms(typed).max(1.0);
    let lt = c * f64::from(raw("delay-time-l")) / 100.0;
    let rt = c * f64::from(raw("delay-time-r")) / 100.0;
    let fb = f64::from(raw("delay-feedback")) / 100.0;
    // A fixed 2-second window, so the axis never moves as the delay times change;
    // taps past 2 s simply fall off the right edge.
    let span = 2000.0;
    let blue = egui::Color32::from_rgb(90, 170, 220);
    let green = egui::Color32::from_rgb(80, 200, 100); // left tap
    let red = egui::Color32::from_rgb(220, 80, 80); // right tap
    // A vertical impulse line (time, height).
    let tap = |t: f64, h: f64| PlotPoints::from(vec![[t, 0.0], [t, h]]);
    Plot::new("gx700-delay")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .x_axis_formatter(|mark, _| format!("{:.2}s", mark.value / 1000.0))
        .y_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .show(ui, |plot| {
            // Pin the view to 0..2 s × 0..100, regardless of the tap positions.
            plot.set_plot_bounds(PlotBounds::from_min_max([0.0, 0.0], [span, 100.0]));
            // Dry signal at t=0.
            plot.line(
                Line::new(tap(0.0, f64::from(raw("delay-direct-level"))))
                    .color(egui::Color32::from_gray(150)),
            );
            if !active {
                return;
            }
            // Each tap recirculates through the feedback loop (period = centre time),
            // so the C / L / R taps all repeat, decaying by the feedback each pass.
            // Fan the three series by a small x offset so coincident taps show as
            // adjacent lines (both colours) instead of hiding behind each other.
            let dx = span * 0.006;
            for (base_t, offset, level_key, color) in [
                (c, 0.0, "delay-level-c", blue),
                (lt, -dx, "delay-level-l", green),
                (rt, dx, "delay-level-r", red),
            ] {
                let mut t = base_t;
                let mut h = f64::from(raw(level_key));
                while t <= span && h >= 1.0 {
                    plot.line(Line::new(tap((t + offset).max(0.0), h)).color(color));
                    t += c;
                    h *= fb;
                }
            }
        });
}

/// A one-line description of each `speaker-type` cabinet (from the GX-700 manual):
/// enclosure + speaker count, the simulated mic ("On Mic" = dynamic, "Off Mic" =
/// condenser), and the preamp model it's voiced to pair with.
fn speaker_cab_desc(index: i32) -> &'static str {
    match index {
        0 => "Small open-back 1×10, dynamic mic",
        1 => "Open-back 1×12, dynamic mic",
        2 => "Open-back 2×12, dynamic mic — Roland JC-120",
        3 => "Open-back 2×12, dynamic mic — pairs with Clean Twin",
        4 => "Open-back 2×12, condenser mic — pairs with Clean Twin",
        5 => "Open-back 2×12, dynamic mic — pairs with Match Drive",
        6 => "Open-back 2×12, condenser mic — pairs with Match Drive",
        7 => "Sealed 2×12 stack, dynamic mic — pairs with BG Lead",
        8 => "Sealed 2×12 stack, condenser mic — pairs with BG Lead",
        9 => "Sealed 4×12 stack, dynamic mic — pairs with MS1959",
        10 => "Sealed 4×12 stack, condenser mic — pairs with MS1959",
        11 => "Large dual 4×12 stack, condenser mic",
        _ => "",
    }
}

/// Draw a *generic* speaker-cabinet response: a guitar cab is essentially a
/// band-pass (lows roll off, a presence bump, then the highs roll off), and the
/// Mic setting tilts the top end (lower ≈ brighter, higher ≈ darker). The actual
/// 12 cabinet models differ — this conveys the band-limiting, not their voicings.
fn show_speaker_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("speaker-enable"), Some(Value::Bool(true)));
    // Mic 1..10 -> a few dB of high-shelf tilt, centred near 5.
    let tilt = (5.5 - f64::from(raw("speaker-mic-setting").clamp(1, 10))) * 1.5;
    let bands = [
        EqBand {
            kind: BandType::LowShelf,
            f0: 90.0,
            q: 0.7,
            gain_db: -12.0,
        },
        EqBand {
            kind: BandType::Peaking,
            f0: 2600.0,
            q: 1.0,
            gain_db: 5.0,
        },
        EqBand {
            kind: BandType::HighShelf,
            f0: 4500.0,
            q: 0.7,
            gain_db: -16.0 + tilt,
        },
    ];
    let points: Vec<[f64; 2]> = (0..=200)
        .map(|i| {
            let lf = 1.3 + (4.3 - 1.3) * (f64::from(i) / 200.0);
            let db = if active {
                eq_response_db(&bands, 10f64.powf(lf))
            } else {
                0.0
            };
            [lf, db]
        })
        .collect();
    Plot::new("gx700-speaker")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_y(-30.0)
        .include_y(12.0)
        .x_axis_formatter(|mark, _| hz_label(mark.value))
        .y_axis_formatter(|mark, _| format!("{:.0} dB", mark.value))
        .show(ui, |plot| plot.line(Line::new(PlotPoints::from(points))));
}

/// Draw the wah as a resonant peak filter: a boost at the centre frequency whose
/// width follows the Peak control. In the pedal modes the heel/toe sweep range
/// (Pedal Min/Max) is shown as faint peaks; in Auto Wah the single peak sits at
/// Manual. The Hz mapping is nominal (the device publishes no exact frequencies).
fn show_wah_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("wah-enable"), Some(Value::Bool(true)));
    let auto = matches!(typed.get("wah-mode"), Some(Value::Enum(2)));
    let q = 0.7 + f64::from(raw("wah-peak")) / 100.0 * 6.0;
    // A 0..100 frequency control -> log Hz over ~250 Hz .. 2.8 kHz (a wah's range).
    let peak_curve = |ctrl: i32| -> Vec<[f64; 2]> {
        let f0 = 10f64.powf(2.4 + (3.45 - 2.4) * f64::from(ctrl.clamp(0, 100)) / 100.0);
        let band = [EqBand {
            kind: BandType::Peaking,
            f0,
            q,
            gain_db: 15.0,
        }];
        (0..=200)
            .map(|i| {
                let lf = 1.3 + (4.3 - 1.3) * (f64::from(i) / 200.0);
                let db = if active {
                    eq_response_db(&band, 10f64.powf(lf))
                } else {
                    0.0
                };
                [lf, db]
            })
            .collect()
    };
    let center = peak_curve(if auto {
        raw("wah-auto-manual")
    } else {
        raw("wah-pedal-freq")
    });
    let sweep = (!auto).then(|| {
        (
            peak_curve(raw("wah-pedal-min")),
            peak_curve(raw("wah-pedal-max")),
        )
    });
    Plot::new("gx700-wah")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_y(-3.0)
        .include_y(18.0)
        .x_axis_formatter(|mark, _| hz_label(mark.value))
        .y_axis_formatter(|mark, _| format!("{:.0} dB", mark.value))
        .show(ui, |plot| {
            if let Some((lo, hi)) = sweep {
                let faint = egui::Color32::from_gray(110);
                plot.line(
                    Line::new(PlotPoints::from(lo))
                        .color(faint)
                        .style(LineStyle::dashed_loose()),
                );
                plot.line(
                    Line::new(PlotPoints::from(hi))
                        .color(faint)
                        .style(LineStyle::dashed_loose()),
                );
            }
            plot.line(
                Line::new(PlotPoints::from(center)).color(egui::Color32::from_rgb(90, 170, 220)),
            );
        });
}

/// Draw the chorus LFO: the modulation waveform over time, blending triangle→sine
/// per Mod wave, with Rate as the cycle density and Depth as the amplitude. In
/// Stereo mode a second (anti-phase) trace shows the L/R offset. Depth 0 is flat
/// (doubling). It illustrates the movement, not absolute pitch/time.
fn show_chorus_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("chorus-enable"), Some(Value::Bool(true)));
    let stereo = matches!(typed.get("chorus-mode"), Some(Value::Enum(1)));
    let depth = f64::from(raw("chorus-depth")) / 100.0;
    let blend = f64::from(raw("chorus-mod-wave").clamp(0, 10)) / 10.0; // 0=triangle 1=sine
    let freq = 0.3 + f64::from(raw("chorus-rate")) / 100.0 * 6.0; // cycles over the window
    let window = 2.0;
    let amp = if active { depth * 50.0 } else { 0.0 };
    let lfo = |phase: f64| -> Vec<[f64; 2]> {
        (0..=240)
            .map(|i| {
                let t = window * f64::from(i) / 240.0;
                let ph = std::f64::consts::TAU * freq * t + phase;
                let sine = ph.sin();
                let tri = (2.0 / std::f64::consts::PI) * sine.asin();
                [t, amp * ((1.0 - blend) * tri + blend * sine)]
            })
            .collect()
    };
    Plot::new("gx700-chorus")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_x(0.0)
        .include_x(window)
        .include_y(-55.0)
        .include_y(55.0)
        .x_axis_formatter(|_, _| String::new())
        .y_axis_formatter(|_, _| String::new())
        .show(ui, |plot| {
            if stereo {
                plot.line(
                    Line::new(PlotPoints::from(lfo(std::f64::consts::PI)))
                        .color(egui::Color32::from_rgb(80, 200, 100)),
                );
            }
            plot.line(
                Line::new(PlotPoints::from(lfo(0.0))).color(egui::Color32::from_rgb(90, 170, 220)),
            );
        });
}

/// Draw the Tremolo/Pan LFO: the modulation waveform (triangle or square per mode)
/// with Rate as cycle density and Depth as amplitude. Tremolo shows one trace (the
/// volume); Pan shows anti-phase L (green) / R (red) — when L rises, R falls.
fn show_trem_curve(ui: &mut egui::Ui, typed: &TypedPatch) {
    let raw = |k: &str| match typed.get(k) {
        Some(Value::Int(v) | Value::Enum(v)) => v,
        _ => 0,
    };
    let active = matches!(typed.get("tremolo-enable"), Some(Value::Bool(true)));
    let mode = match typed.get("tremolo-mode") {
        Some(Value::Enum(v)) => v,
        _ => 0,
    };
    let pan = mode >= 2; // 0/1 = Tremolo, 2/3 = Pan
    let square = mode == 1 || mode == 3;
    let freq = 0.3 + f64::from(raw("tremolo-rate")) / 100.0 * 6.0;
    let amp = if active {
        f64::from(raw("tremolo-depth")) / 100.0 * 50.0
    } else {
        0.0
    };
    let window = 2.0;
    let wave = |sign: f64| -> Vec<[f64; 2]> {
        (0..=240)
            .map(|i| {
                let t = window * f64::from(i) / 240.0;
                let s = (std::f64::consts::TAU * freq * t).sin();
                let w = if square {
                    if s >= 0.0 { 1.0 } else { -1.0 }
                } else {
                    (2.0 / std::f64::consts::PI) * s.asin()
                };
                [t, sign * amp * w]
            })
            .collect()
    };
    Plot::new("gx700-trem")
        .height(150.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_double_click_reset(false)
        .include_x(0.0)
        .include_x(window)
        .include_y(-55.0)
        .include_y(55.0)
        .x_axis_formatter(|_, _| String::new())
        .y_axis_formatter(|_, _| String::new())
        .show(ui, |plot| {
            if pan {
                plot.line(
                    Line::new(PlotPoints::from(wave(1.0)))
                        .color(egui::Color32::from_rgb(80, 200, 100)),
                );
                plot.line(
                    Line::new(PlotPoints::from(wave(-1.0)))
                        .color(egui::Color32::from_rgb(220, 80, 80)),
                );
            } else {
                plot.line(
                    Line::new(PlotPoints::from(wave(1.0)))
                        .color(egui::Color32::from_rgb(90, 170, 220)),
                );
            }
        });
}

/// A grid cell pair: a label then the parameter `key` as a drag-value.
fn grid_drag(
    ui: &mut egui::Ui,
    slot: u16,
    label: &str,
    key: &'static str,
    typed: &TypedPatch,
    on: bool,
    actions: &mut Vec<Action>,
) {
    ui.label(label);
    param_drag(ui, slot, key, typed, on, actions);
}

/// A grid cell pair: a label then the parameter `key` as a combo box.
fn grid_combo(
    ui: &mut egui::Ui,
    slot: u16,
    label: &str,
    key: &'static str,
    typed: &TypedPatch,
    on: bool,
    actions: &mut Vec<Action>,
) {
    ui.label(label);
    param_combo(ui, slot, key, typed, on, actions);
}

/// A short caption line under a Modulation type's controls.
fn mod_caption(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).weak());
}

fn mod_flanger(ui: &mut egui::Ui, slot: u16, t: &TypedPatch, on: bool, a: &mut Vec<Action>) {
    egui::Grid::new("mod-flanger")
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            grid_drag(ui, slot, "Rate", "mod-rate", t, on, a);
            grid_drag(ui, slot, "Depth", "mod-depth", t, on, a);
            ui.end_row();
            grid_drag(ui, slot, "Manual", "mod-manual", t, on, a);
            grid_drag(ui, slot, "Resonance", "mod-resonance", t, on, a);
            ui.end_row();
            grid_drag(ui, slot, "Separation", "mod-flanger-separation", t, on, a);
            grid_drag(ui, slot, "Gate", "mod-flanger-gate", t, on, a);
            ui.end_row();
        });
    mod_caption(
        ui,
        "Jet-like sweep. Resonance feeds back (negative = reversed phase); Gate chops the output.",
    );
}

fn mod_phaser(ui: &mut egui::Ui, slot: u16, t: &TypedPatch, on: bool, a: &mut Vec<Action>) {
    egui::Grid::new("mod-phaser")
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            grid_combo(ui, slot, "Stage", "mod-phaser-stage", t, on, a);
            grid_drag(ui, slot, "Rate", "mod-rate", t, on, a);
            ui.end_row();
            grid_drag(ui, slot, "Depth", "mod-depth", t, on, a);
            grid_drag(ui, slot, "Manual", "mod-manual", t, on, a);
            ui.end_row();
            grid_drag(ui, slot, "Resonance", "mod-resonance", t, on, a);
            grid_drag(ui, slot, "Step", "mod-phaser-step-rate", t, on, a);
            ui.end_row();
        });
    mod_caption(
        ui,
        "Swirling phase effect. Stage = number of phase stages; Step makes the sweep stepped.",
    );
}

fn mod_vibrato(ui: &mut egui::Ui, slot: u16, t: &TypedPatch, on: bool, a: &mut Vec<Action>) {
    egui::Grid::new("mod-vibrato")
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            grid_combo(ui, slot, "Trigger", "mod-vibrato-trigger", t, on, a);
            grid_drag(ui, slot, "Rise time", "mod-vibrato-rise-time", t, on, a);
            ui.end_row();
            grid_drag(ui, slot, "Rate", "mod-rate", t, on, a);
            grid_drag(ui, slot, "Depth", "mod-depth", t, on, a);
            ui.end_row();
        });
    mod_caption(
        ui,
        "Pitch vibrato. Trigger: On follows a footswitch, Auto applies on each pick.",
    );
}

fn mod_ring(ui: &mut egui::Ui, slot: u16, t: &TypedPatch, on: bool, a: &mut Vec<Action>) {
    egui::Grid::new("mod-ring")
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            grid_drag(ui, slot, "Frequency", "mod-ring-frequency", t, on, a);
            grid_drag(ui, slot, "Effect level", "mod-ring-effect-level", t, on, a);
            ui.end_row();
            grid_drag(ui, slot, "Direct level", "mod-ring-direct-level", t, on, a);
            ui.end_row();
        });
    mod_caption(
        ui,
        "Bell-like, unpitched. Frequency 'INT' (0) tracks your playing's pitch.",
    );
}

fn mod_humanizer(ui: &mut egui::Ui, slot: u16, t: &TypedPatch, on: bool, a: &mut Vec<Action>) {
    let pedal = matches!(t.get("mod-humanizer-type"), Some(Value::Enum(1)));
    egui::Grid::new("mod-humanizer")
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            grid_combo(ui, slot, "Type", "mod-humanizer-type", t, on, a);
            ui.end_row();
            grid_combo(ui, slot, "Vowel 1", "mod-humanizer-vowel1", t, on, a);
            grid_combo(ui, slot, "Vowel 2", "mod-humanizer-vowel2", t, on, a);
            ui.end_row();
            if pedal {
                grid_drag(
                    ui,
                    slot,
                    "Pedal source",
                    "mod-humanizer-pedal-source",
                    t,
                    on,
                    a,
                );
            } else {
                grid_drag(ui, slot, "Rate", "mod-rate", t, on, a);
                grid_drag(ui, slot, "Depth", "mod-depth", t, on, a);
                ui.end_row();
                grid_combo(ui, slot, "Trigger", "mod-humanizer-trigger", t, on, a);
            }
            ui.end_row();
        });
    mod_caption(
        ui,
        "Talk/vowel effect between two vowels. Auto sweeps; Pedal switches them by pedal.",
    );
}

fn mod_pitch_shifter(ui: &mut egui::Ui, slot: u16, t: &TypedPatch, on: bool, a: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label("Type");
        param_combo(ui, slot, "mod-ps-type", t, on, a);
    });
    ui.add_space(4.0);
    egui::Grid::new("mod-ps-voices")
        .num_columns(5)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("");
            ui.strong("Pitch");
            ui.strong("Fine");
            ui.strong("Pan");
            ui.strong("Level");
            ui.end_row();
            let voices = [
                (
                    "mod-ps-pitch1",
                    "mod-ps-fine1",
                    "mod-pshr-pan1",
                    "mod-pshr-level1",
                ),
                (
                    "mod-ps-pitch2",
                    "mod-ps-fine2",
                    "mod-pshr-pan2",
                    "mod-pshr-level2",
                ),
                (
                    "mod-ps-pitch3",
                    "mod-ps-fine3",
                    "mod-pshr-pan3",
                    "mod-pshr-level3",
                ),
            ];
            for (n, (pitch, fine, pan, level)) in voices.into_iter().enumerate() {
                ui.label(format!("Voice {}", n + 1));
                param_drag(ui, slot, pitch, t, on, a);
                param_drag(ui, slot, fine, t, on, a);
                param_drag(ui, slot, pan, t, on, a);
                param_drag(ui, slot, level, t, on, a);
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    egui::Grid::new("mod-ps-mix")
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            grid_drag(ui, slot, "Balance", "mod-pshr-balance", t, on, a);
            grid_drag(ui, slot, "Total level", "mod-pshr-total-level", t, on, a);
            ui.end_row();
        });
    mod_caption(
        ui,
        "Up to 3 pitched voices (±2 oct). Fine: ±50 = one semitone.",
    );
}

fn mod_harmonist(ui: &mut egui::Ui, slot: u16, t: &TypedPatch, on: bool, a: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.label("Key")
            .on_hover_text("Song key, so the harmony fits the scale (0–24).");
        param_drag(ui, slot, "mod-harmonist-key", t, on, a);
    });
    ui.add_space(4.0);
    egui::Grid::new("mod-hr-voices")
        .num_columns(4)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("");
            ui.strong("Interval");
            ui.strong("Pan");
            ui.strong("Level");
            ui.end_row();
            let voices = [
                (
                    "mod-harmonist-interval1",
                    "mod-pshr-pan1",
                    "mod-pshr-level1",
                ),
                (
                    "mod-harmonist-interval2",
                    "mod-pshr-pan2",
                    "mod-pshr-level2",
                ),
                (
                    "mod-harmonist-interval3",
                    "mod-pshr-pan3",
                    "mod-pshr-level3",
                ),
            ];
            for (n, (interval, pan, level)) in voices.into_iter().enumerate() {
                ui.label(format!("Voice {}", n + 1));
                param_drag(ui, slot, interval, t, on, a);
                param_drag(ui, slot, pan, t, on, a);
                param_drag(ui, slot, level, t, on, a);
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    egui::Grid::new("mod-hr-mix")
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            grid_drag(ui, slot, "Balance", "mod-pshr-balance", t, on, a);
            grid_drag(ui, slot, "Total level", "mod-pshr-total-level", t, on, a);
            ui.end_row();
        });
    mod_caption(
        ui,
        "Scale-aware harmony — play single notes. (The user scale isn't editable here.)",
    );
}

/// One EQ band row in the Gain/Freq/Q grid. Shelves pass `None` for freq/q and
/// get an em dash in those cells; the mid band passes its enum keys.
#[allow(clippy::too_many_arguments)]
fn eq_row(
    ui: &mut egui::Ui,
    slot: u16,
    name: &str,
    gain_key: &'static str,
    freq_key: Option<&'static str>,
    q_key: Option<&'static str>,
    typed: &TypedPatch,
    enabled: bool,
    actions: &mut Vec<Action>,
) {
    ui.label(name);
    param_drag(ui, slot, gain_key, typed, enabled, actions);
    for key in [freq_key, q_key] {
        match key {
            Some(key) => {
                param_combo(ui, slot, key, typed, enabled, actions);
            }
            None => {
                ui.weak("—");
            }
        }
    }
    ui.end_row();
}

/// The Level/Chain assign grid: field label (a row) and the four `assignN-*` keys
/// for it (the columns). Mode is an enum (combo); every other field is a drag-value.
const ASSIGN_ROWS: [(&str, [&str; 4]); 7] = [
    (
        "Target",
        [
            "assign1-target",
            "assign2-target",
            "assign3-target",
            "assign4-target",
        ],
    ),
    (
        "Source",
        [
            "assign1-source",
            "assign2-source",
            "assign3-source",
            "assign4-source",
        ],
    ),
    (
        "Mode",
        [
            "assign1-mode",
            "assign2-mode",
            "assign3-mode",
            "assign4-mode",
        ],
    ),
    (
        "Min",
        ["assign1-min", "assign2-min", "assign3-min", "assign4-min"],
    ),
    (
        "Max",
        ["assign1-max", "assign2-max", "assign3-max", "assign4-max"],
    ),
    (
        "Action lo",
        [
            "assign1-act-lo",
            "assign2-act-lo",
            "assign3-act-lo",
            "assign4-act-lo",
        ],
    ),
    (
        "Action hi",
        [
            "assign1-act-hi",
            "assign2-act-hi",
            "assign3-act-hi",
            "assign4-act-hi",
        ],
    ),
];

/// A schematic of one control assign, reflecting its live values and mode. `lo`/`hi`
/// are the Action lo/hi (Source `0..127`); `min`/`max` are the Target values, placed
/// on the Y axis within the target's `range` so each sits at its true height and
/// editing it moves the line. **Normal** ramps the Target Min->Max across the Action
/// lo..hi window; **Toggle** flips the Target between the Min/Max states on each
/// operation.
// A cohesive drawing routine (axes, reference lines, the two mode layouts, labels);
// splitting it would only scatter the shared painter/geometry locals.
#[allow(clippy::too_many_lines)]
fn show_assign_schematic(
    ui: &mut egui::Ui,
    lo: i32,
    hi: i32,
    min: i32,
    max: i32,
    range: (i32, i32),
    toggle: bool,
) {
    let w = ui.available_width().min(440.0);
    let (resp, painter) = ui.allocate_painter(egui::vec2(w, 170.0), egui::Sense::hover());
    let rect = resp.rect;
    let v = ui.visuals();
    let axis = egui::Stroke::new(1.0, v.text_color());
    let refln = egui::Stroke::new(1.0, v.weak_text_color());
    let accent = egui::Color32::from_rgb(90, 160, 230);
    let (txt, weak) = (v.text_color(), v.weak_text_color());
    let font = egui::FontId::proportional(11.0);

    let plot = egui::Rect::from_min_max(
        rect.min + egui::vec2(64.0, 18.0),
        rect.max - egui::vec2(56.0, 28.0),
    );
    let px = |fx: f32| plot.left() + fx * plot.width();
    let py = |fy: f32| plot.bottom() - fy * plot.height();
    let frac = |val: i32| f32::from(u8::try_from(val.clamp(0, 127)).unwrap_or(0)) / 127.0;
    // Map a Min/Max value onto the Y axis within the target parameter's range, so each
    // sits at its true height and editing it moves the line. (When the target range is
    // unknown the caller passes the Min/Max span, which just fills the height.)
    let (r_lo, r_hi) = range;
    let span = (r_hi - r_lo).max(1);
    let lvl = |val: i32| {
        let frac = f32::from(u16::try_from((val.clamp(r_lo, r_hi) - r_lo).max(0)).unwrap_or(0))
            / f32::from(u16::try_from(span).unwrap_or(1));
        0.1 + 0.8 * frac
    };
    let lo_x = frac(lo);
    let hi_x = frac(hi).max(lo_x);
    let (y_min, y_max) = (lvl(min), lvl(max));
    let dashed = |a: egui::Pos2, b: egui::Pos2| egui::Shape::dashed_line(&[a, b], refln, 4.0, 4.0);
    let lbl = |pos: egui::Pos2, anchor: egui::Align2, s: &str, c: egui::Color32| {
        painter.text(pos, anchor, s, font.clone(), c);
    };

    painter.line_segment([plot.left_top(), plot.left_bottom()], axis);
    painter.line_segment([plot.left_bottom(), plot.right_bottom()], axis);
    for (y, name) in [(y_min, "Min"), (y_max, "Max")] {
        painter.extend(dashed(
            egui::pos2(plot.left(), py(y)),
            egui::pos2(plot.right(), py(y)),
        ));
        lbl(
            egui::pos2(plot.left() - 6.0, py(y)),
            egui::Align2::RIGHT_CENTER,
            name,
            txt,
        );
    }

    if toggle {
        // Two latched states; the Source flips the Target between them.
        let x = px(lo_x);
        let (top, bot) = (py(y_min).min(py(y_max)), py(y_min).max(py(y_max)));
        painter.line_segment(
            [egui::pos2(x, top), egui::pos2(x, bot)],
            egui::Stroke::new(2.0, accent),
        );
        let head = |tip: egui::Pos2, dy: f32| {
            egui::Shape::convex_polygon(
                vec![
                    tip,
                    egui::pos2(tip.x - 5.0, tip.y + dy),
                    egui::pos2(tip.x + 5.0, tip.y + dy),
                ],
                accent,
                egui::Stroke::NONE,
            )
        };
        painter.add(head(egui::pos2(x, top), 6.0));
        painter.add(head(egui::pos2(x, bot), -6.0));
        lbl(
            egui::pos2(x, bot + 8.0),
            egui::Align2::CENTER_TOP,
            &format!("Trigger ({lo})"),
            txt,
        );
        lbl(
            egui::pos2(plot.center().x, plot.top() - 4.0),
            egui::Align2::CENTER_BOTTOM,
            "Toggle — flips Min\u{2194}Max each operation",
            weak,
        );
        lbl(
            egui::pos2(plot.center().x, rect.bottom()),
            egui::Align2::CENTER_BOTTOM,
            "Source (trigger)",
            weak,
        );
    } else {
        // Normal: ramp from Min to Max across the Action window.
        for x in [lo_x, hi_x] {
            painter.extend(dashed(
                egui::pos2(px(x), plot.bottom()),
                egui::pos2(px(x), plot.top()),
            ));
        }
        painter.add(egui::Shape::line(
            vec![
                egui::pos2(px(0.0), py(y_min)),
                egui::pos2(px(lo_x), py(y_min)),
                egui::pos2(px(hi_x), py(y_max)),
                egui::pos2(px(1.0), py(y_max)),
            ],
            egui::Stroke::new(2.5, accent),
        ));
        lbl(
            egui::pos2(px(lo_x), plot.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            &format!("lo {lo}"),
            txt,
        );
        lbl(
            egui::pos2(px(hi_x), plot.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            &format!("hi {hi}"),
            txt,
        );
        lbl(
            egui::pos2(plot.center().x, rect.bottom()),
            egui::Align2::CENTER_BOTTOM,
            "Source (controller)",
            weak,
        );
    }
    lbl(
        egui::pos2(rect.left(), plot.top() - 2.0),
        egui::Align2::LEFT_BOTTOM,
        "Target",
        weak,
    );
}

/// An int parameter as a compact drag-value in display units (US-16x08 style).
/// Returns whether the value changed this frame.
fn param_drag(
    ui: &mut egui::Ui,
    slot: u16,
    key: &'static str,
    typed: &TypedPatch,
    enabled: bool,
    actions: &mut Vec<Action>,
) -> bool {
    let Some(p) = Param::from_key(key) else {
        return false;
    };
    let Kind::Int { min, max, .. } = p.kind() else {
        return false;
    };
    let mut val = match typed.get(key) {
        Some(Value::Int(v)) => v,
        _ => 0,
    };
    ui.add_enabled_ui(enabled, |ui| {
        let widget = egui::DragValue::new(&mut val)
            .range(min..=max)
            .custom_formatter(move |n, _| display_raw(p, n));
        let changed = ui.add(widget).changed();
        if changed {
            actions.push(Action::SetParam(slot, key, Value::Int(val)));
        }
        changed
    })
    .inner
}

/// An enum parameter as a dropdown of its labels. Returns whether it changed.
fn param_combo(
    ui: &mut egui::Ui,
    slot: u16,
    key: &'static str,
    typed: &TypedPatch,
    enabled: bool,
    actions: &mut Vec<Action>,
) -> bool {
    let Some(p) = Param::from_key(key) else {
        return false;
    };
    let Kind::Enum { values, .. } = p.kind() else {
        return false;
    };
    let idx = match typed.get(key) {
        Some(Value::Enum(v)) => v,
        _ => 0,
    };
    let cur = usize::try_from(idx)
        .ok()
        .and_then(|i| values.get(i))
        .copied()
        .unwrap_or("?");
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt((slot, key))
            .selected_text(cur)
            .show_ui(ui, |ui| {
                for (i, lbl) in values.iter().enumerate() {
                    let this = i32::try_from(i).unwrap_or(-1);
                    if ui.selectable_label(this == idx, *lbl).clicked() {
                        actions.push(Action::SetParam(slot, key, Value::Enum(this)));
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// A control-assign Target as a dropdown of named targets (MI Table 1.2). The value
/// is the target id; the menu lists every assignable parameter by name. Returns
/// whether it changed.
fn param_target_combo(
    ui: &mut egui::Ui,
    slot: u16,
    key: &'static str,
    typed: &TypedPatch,
    enabled: bool,
    actions: &mut Vec<Action>,
) -> bool {
    let cur = match typed.get(key) {
        Some(Value::Int(v)) => v,
        _ => 0,
    };
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt((slot, key))
            .selected_text(rackctl_gx700::param::assign_target_name(cur))
            .width(190.0)
            .show_ui(ui, |ui| {
                for id in 0..rackctl_gx700::param::ASSIGN_TARGETS.len() {
                    let id = i32::try_from(id).unwrap_or(0);
                    let name = rackctl_gx700::param::assign_target_name(id);
                    if ui.selectable_label(id == cur, name).clicked() {
                        actions.push(Action::SetParam(slot, key, Value::Int(id)));
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// Format a raw drag-value in `p`'s display units (the dB string).
#[allow(clippy::cast_possible_truncation)]
fn display_raw(p: Param, n: f64) -> String {
    units::display(p, Value::Int(n as i32))
}

/// Format a log10(Hz) axis value as a frequency label (`100`, `1k`, `10k`).
fn hz_label(log_hz: f64) -> String {
    let hz = 10f64.powf(log_hz);
    if hz >= 1000.0 {
        format!("{:.0}k", hz / 1000.0)
    } else {
        format!("{hz:.0}")
    }
}

/// Parse a frequency label (`"100Hz"`, `"1.6kHz"`) into Hz.
fn hz_from_label(s: &str) -> f64 {
    let t = s.trim();
    if let Some(k) = t.strip_suffix("kHz") {
        k.trim().parse::<f64>().unwrap_or(1.0) * 1000.0
    } else if let Some(h) = t.strip_suffix("Hz") {
        h.trim().parse().unwrap_or(1000.0)
    } else {
        t.parse().unwrap_or(1000.0)
    }
}

/// Render one parameter as a live widget (checkbox / slider / combo by kind),
/// pushing a [`Action::SetParam`] when the user changes it.
fn param_widget(
    ui: &mut egui::Ui,
    slot: u16,
    p: Param,
    value: Value,
    enabled: bool,
    actions: &mut Vec<Action>,
) {
    // Drop the block prefix for a shorter label (e.g. "preamp-volume" -> "volume").
    let label = p.key().split_once('-').map_or(p.key(), |(_, rest)| rest);
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| match (p.kind(), value) {
            (Kind::Bool, Value::Bool(b)) => {
                let mut on = b;
                if ui.checkbox(&mut on, label).changed() {
                    actions.push(Action::SetParam(slot, p.key(), Value::Bool(on)));
                }
            }
            (Kind::Int { min, max, .. }, Value::Int(v)) => {
                ui.label(label);
                let mut val = v;
                if ui.add(egui::Slider::new(&mut val, min..=max)).changed() {
                    actions.push(Action::SetParam(slot, p.key(), Value::Int(val)));
                }
                ui.label(units::display(p, Value::Int(val)));
            }
            (Kind::Enum { values, .. }, Value::Enum(idx)) => {
                ui.label(label);
                let cur = usize::try_from(idx)
                    .ok()
                    .and_then(|i| values.get(i))
                    .copied()
                    .unwrap_or("?");
                egui::ComboBox::from_id_salt((slot, p.key()))
                    .selected_text(cur)
                    .show_ui(ui, |ui| {
                        for (i, lbl) in values.iter().enumerate() {
                            let this = i32::try_from(i).unwrap_or(-1);
                            if ui.selectable_label(this == idx, *lbl).clicked() {
                                actions.push(Action::SetParam(slot, p.key(), Value::Enum(this)));
                            }
                        }
                    });
            }
            _ => {}
        });
    });
}

#[allow(clippy::struct_excessive_bools)] // aggregate UI state, not a state machine
pub(crate) struct App {
    device: SharedDevice,
    connected: bool,
    reopen: Reopen,
    loader: Option<Loader>,
    /// Slots received from the loader in the current load (for the progress bar).
    progress: u16,
    rows: Vec<PatchRow>,
    /// The factory presets (slots `101..=200`), read-only; loaded lazily the first
    /// time the Presets tab is opened.
    presets: Vec<PatchRow>,
    /// The background read of the preset headers, while it runs.
    preset_loader: Option<Loader>,
    /// Presets received from the preset loader in the current load (progress bar).
    preset_progress: u16,
    /// Set once the preset headers are populated (from cache or a completed read),
    /// so the lazy load runs at most once per session.
    presets_loaded: bool,
    now_playing: Option<u16>,
    /// The copied patch and its source slot, set by Copy and stamped by Paste.
    clipboard: Option<(u16, TypedPatch)>,
    /// Additive per-block clipboard: a scratch patch holding the blocks copied in
    /// the Edit tab, plus the list of which blocks are actually held (so Paste
    /// greys out until that block has been copied). Lets the user combine blocks
    /// from different patches into a new one.
    block_clip: TypedPatch,
    clip_blocks: Vec<Block>,
    /// Scene clipboard (a whole bank), for copy/paste in the scene library.
    scene_clip: Option<Vec<TypedPatch>>,
    /// Save-as name fields for the Library tab (patch / block / scene).
    lib_patch_name: String,
    lib_block_name: String,
    /// A library file pending a delete confirmation.
    pending_delete: Option<std::path::PathBuf>,
    /// Whether the Edit tab's right panel shows the selected block's preset library
    /// (toggled by the per-block "Library" button) instead of its parameters.
    block_lib_open: bool,
    /// A Refresh is awaiting confirmation because it would discard staged edits.
    confirm_refresh: bool,
    /// The scene being composed in the Scene tab: one patch per user slot, edited
    /// offline (independent of the live bank) until applied to the unit or saved.
    /// Always `USER_SLOTS` long; an untouched slot holds an INIT (default) patch.
    compose: Vec<TypedPatch>,
    /// Baseline for the composer — its state when the scene was last established
    /// (New / Capture / Load / Save). Per-slot Revert restores from this.
    compose_base: Vec<TypedPatch>,
    /// Save-as / loaded-scene name for the composer.
    compose_name: String,
    /// What the Edit tab is editing: a bank slot (live, preview when in BULK LOAD),
    /// or [`SCRATCH`] for the offline patch in `edit_scratch`. `None` until selected.
    edit_slot: Option<u16>,
    /// The offline patch being edited (when `edit_slot == Some(SCRATCH)`): no device,
    /// no preview. Saved back to `edit_source` on demand.
    edit_scratch: TypedPatch,
    /// The offline patch as it was opened — the baseline for "block changed" / Revert.
    edit_base: TypedPatch,
    /// Where an offline edit is saved back to (a composer slot or a library patch).
    edit_source: Option<OfflineSource>,
    /// Which screen is showing.
    tab: Tab,
    /// The effect block selected in the Edit tab.
    selected_block: Block,
    /// Which control assign (0..=3) the Level/Chain schematic illustrates.
    selected_assign: usize,
    bulk_prompt: bool,
    /// The background batch write (scene / "Write changes"), while it runs, plus its
    /// running tally for the progress bar and final report.
    writer: Option<Writer>,
    write_progress: usize,
    write_stored: usize,
    write_failed: Vec<u16>,
    /// Whether the unit is in BULK LOAD mode: `None` until first probed, then
    /// `Some(true/false)`. While `Some(false)` a blocking dialog asks the user to
    /// enter BULK LOAD; the whole session then stays in it, so no mode switching.
    bulk_ok: Option<bool>,
    /// `egui` input-time of the last BULK LOAD probe, to throttle re-probes.
    last_probe: f64,
    /// A BULK LOAD probe running off the UI thread, while in flight (see
    /// `drive_startup` / `drain_prober`).
    prober: Option<Prober>,
    /// An on-demand single-slot read started to audition or copy a factory preset
    /// (read shallow, so its full patch isn't loaded yet) — kept off the UI thread.
    lazy_loader: Option<Loader>,
    /// The action to re-run once `lazy_loader` lands the patch it was reading
    /// (`Audition`/`CopyRow` of the preset that needed loading).
    pending_after_load: Option<Action>,
    /// Launched with `--offline`: started without connecting, for scene/library
    /// editing. The top bar offers Connect instead of Retry.
    offline: bool,
    /// The rawmidi port in use (resolved from `--port` or the saved config),
    /// persisted so the next launch reuses it.
    port: Option<String>,
    status: String,
    zoom: f32,
    window: Option<[f32; 2]>,
}

impl App {
    pub(crate) fn new(
        device: Device,
        connected: bool,
        reopen: Reopen,
        offline: bool,
        port: Option<String>,
    ) -> Self {
        let cfg = config::load();
        // Restore the last-active tab. Offline forces the Scene tab unless the saved
        // tab also works offline (Scene/Library); the device tabs are useless then.
        let saved_tab = cfg.tab.as_deref().and_then(Tab::from_key);
        let tab = if offline {
            match saved_tab {
                Some(t @ (Tab::Scene | Tab::Library)) => t,
                _ => Tab::Scene,
            }
        } else {
            saved_tab.unwrap_or(Tab::Patches)
        };
        let mut rows: Vec<PatchRow> = (1..=USER_SLOTS).map(PatchRow::empty).collect();
        let mut presets: Vec<PatchRow> = (PRESET_START..=PRESET_END).map(PatchRow::empty).collect();
        // Show the cached bank instantly, before the (slow) re-read fills it in.
        // The cache holds both user (`1..=100`) and preset (`101..=200`) rows.
        let mut presets_loaded = false;
        for cached in config::load_cache() {
            let target = if cached.slot <= USER_SLOTS {
                rows.get_mut(usize::from(cached.slot - 1))
            } else {
                presets_loaded = true;
                presets.get_mut(usize::from(cached.slot - PRESET_START))
            };
            if let Some(row) = target {
                row.name.clone_from(&cached.name);
                row.name_edit = cached.name;
                row.stored_level = cached.output_level;
                row.chain = cached.chain;
            }
        }

        let device = Arc::new(Mutex::new(device));
        // The bank read is deferred until the BULK LOAD probe confirms the mode
        // (driven from `update`), so the session starts in BULK LOAD and stays.
        Self {
            device,
            connected,
            reopen,
            loader: None,
            progress: 0,
            rows,
            presets,
            preset_loader: None,
            preset_progress: 0,
            presets_loaded,
            now_playing: None,
            clipboard: None,
            block_clip: TypedPatch::default(),
            clip_blocks: Vec::new(),
            scene_clip: None,
            lib_patch_name: String::new(),
            lib_block_name: String::new(),
            pending_delete: None,
            block_lib_open: false,
            confirm_refresh: false,
            compose: vec![TypedPatch::init(); usize::from(USER_SLOTS)],
            compose_base: vec![TypedPatch::init(); usize::from(USER_SLOTS)],
            compose_name: String::new(),
            edit_slot: None,
            edit_scratch: TypedPatch::default(),
            edit_base: TypedPatch::default(),
            edit_source: None,
            tab,
            selected_block: Block::Compressor,
            selected_assign: 0,
            bulk_prompt: false,
            writer: None,
            write_progress: 0,
            write_stored: 0,
            write_failed: Vec::new(),
            bulk_ok: None,
            last_probe: 0.0,
            prober: None,
            lazy_loader: None,
            pending_after_load: None,
            offline,
            port,
            status: if connected {
                "checking BULK LOAD mode…".to_owned()
            } else if offline {
                "offline — editing scenes and the library; Connect to go online".to_owned()
            } else {
                "not connected — pass --port hw:CARD,DEV (or --mock), then Retry".to_owned()
            },
            zoom: cfg.zoom,
            window: cfg.window,
        }
    }

    pub(crate) fn zoom(&self) -> f32 {
        self.zoom
    }

    /// The row for any slot: a user patch (`1..=100`) or a factory preset
    /// (`101..=200`). This lets audition / `effective_patch` work for presets too.
    fn row(&self, slot: u16) -> Option<&PatchRow> {
        if slot <= USER_SLOTS {
            self.rows.get(usize::from(slot.saturating_sub(1)))
        } else {
            self.presets
                .get(usize::from(slot.saturating_sub(PRESET_START)))
        }
    }

    fn row_mut(&mut self, slot: u16) -> Option<&mut PatchRow> {
        if slot <= USER_SLOTS {
            self.rows.get_mut(usize::from(slot.saturating_sub(1)))
        } else {
            self.presets
                .get_mut(usize::from(slot.saturating_sub(PRESET_START)))
        }
    }

    fn dirty_count(&self) -> usize {
        self.rows.iter().filter(|r| r.dirty()).count()
    }

    /// Whether interactive controls (edits, audition, writes) are usable: the
    /// device is connected, no bank read is in flight, and we are not blocked
    /// waiting for the unit to enter BULK LOAD mode.
    fn editable(&self) -> bool {
        self.connected
            && self.loader.is_none()
            && self.writer.is_none()
            && self.bulk_ok != Some(false)
    }

    /// Whether every user-bank row holds a loaded patch (so the whole bank can be
    /// captured/snapshotted). Independent of the device — true from cache offline.
    fn bank_loaded(&self) -> bool {
        self.rows.iter().all(|r| r.full.is_some())
    }

    /// Probe the unit's BULK LOAD mode and react: start the bank read once it's in
    /// BULK LOAD (or if the probe can't run, e.g. on the mock), otherwise mark it
    /// not-yet-ready so the blocking dialog stays up. Restores the probed slot.
    /// Start a BULK LOAD probe off the UI thread (if none is already running and
    /// we're connected). The result is handled in `drain_prober`.
    fn request_probe(&mut self) {
        if self.connected && self.prober.is_some() {
            return;
        }
        if self.connected {
            self.prober = Some(Prober::spawn(Arc::clone(&self.device), PROBE_SLOT));
        }
    }

    /// Pump every background task each frame: drive the BULK LOAD probe, drain the
    /// loaders/writer, and keep the window repainting while any device work (probe,
    /// bank/preset/on-demand read, batch write) is in flight — so the UI thread
    /// never blocks on device I/O.
    fn pump_background(&mut self, ctx: &egui::Context) {
        self.drive_startup(ctx);
        self.drain_prober();
        self.drain_loader();
        self.maybe_load_presets();
        self.drain_preset_loader();
        self.drain_lazy();
        self.drain_write();
        if self.loader.is_some()
            || self.preset_loader.is_some()
            || self.writer.is_some()
            || self.prober.is_some()
            || self.lazy_loader.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        // Keep re-probing while waiting for the unit to enter BULK LOAD mode.
        if self.bulk_ok == Some(false) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    /// React to a finished background BULK LOAD probe (see `request_probe`).
    fn drain_prober(&mut self) {
        let Some(prober) = &self.prober else {
            return;
        };
        let Some(outcome) = prober.poll() else {
            return; // still probing
        };
        self.prober = None;
        match outcome {
            Probe::Waiting => {
                self.bulk_ok = Some(false);
                "waiting for BULK LOAD mode on the unit…".clone_into(&mut self.status);
            }
            // In BULK LOAD, or the probe can't run (mock / silent unit) — proceed and
            // let the bank read surface any real trouble.
            Probe::InBulkLoad | Probe::CantRun => {
                let first = self.bulk_ok != Some(true);
                self.bulk_ok = Some(true);
                if first {
                    self.start_load();
                }
            }
        }
    }

    /// Drive the startup BULK LOAD check: probe immediately the first time, then
    /// re-probe on an interval while still waiting (so entering the mode on the
    /// unit is picked up automatically). Quiet once the unit is confirmed.
    fn drive_startup(&mut self, ctx: &egui::Context) {
        if !self.connected || self.bulk_ok == Some(true) || self.prober.is_some() {
            return;
        }
        let now = ctx.input(|i| i.time);
        if self.bulk_ok.is_some() && now - self.last_probe < PROBE_INTERVAL {
            return;
        }
        self.last_probe = now;
        self.request_probe();
    }

    /// Load a row's full patch if it isn't loaded yet (needed before storing,
    /// e.g. a name-only edit on a patch that was never auditioned).
    ///
    /// User-bank rows are deep-read by the bank loader and edits are gated until it
    /// finishes, so in practice this is a no-op there. The factory presets are read
    /// shallow, but their on-demand load goes through `ensure_loaded_or_defer` (off
    /// the UI thread) — so this synchronous read should never actually fire.
    fn ensure_loaded(&mut self, slot: u16) {
        if self.row(slot).is_some_and(|r| r.full.is_none()) {
            let read = device::lock(&self.device).read_patch(slot);
            if let Ok(raw) = read
                && let Some(row) = self.row_mut(slot)
            {
                row.full = Some(TypedPatch::from_raw(&raw));
            }
        }
    }

    /// Ensure `slot`'s full patch is loaded *without blocking the UI*. Returns
    /// `true` if it's already loaded (the caller may proceed); otherwise starts a
    /// background single-slot read and stashes `then` to be re-run once it lands
    /// (see `drain_lazy`), returning `false`. Used by the preset audition/copy
    /// paths, whose rows are read shallow.
    fn ensure_loaded_or_defer(&mut self, slot: u16, then: Action) -> bool {
        if self.row(slot).is_none_or(|r| r.full.is_some()) {
            return true;
        }
        self.pending_after_load = Some(then);
        if self.lazy_loader.is_none() {
            self.lazy_loader = Some(Loader::spawn_range(
                Arc::clone(&self.device),
                slot,
                slot,
                true,
            ));
            self.status = format!("loading {}\u{2026}", slot_label(slot));
        }
        false
    }

    /// Drain the on-demand single-slot read started by `ensure_loaded_or_defer`;
    /// once the patch lands, re-run the action (audition/copy) that needed it.
    fn drain_lazy(&mut self) {
        let Some(events) = self.lazy_loader.as_ref().map(Loader::drain) else {
            return;
        };
        let mut done = false;
        let mut failed: Option<String> = None;
        for ev in events {
            match ev {
                Loaded::Patch(slot, raw) => {
                    if let Some(row) = self.row_mut(slot) {
                        row.full = Some(TypedPatch::from_raw(&raw));
                        row.failed = false;
                    }
                }
                Loaded::Failed(_, msg) | Loaded::Aborted(msg) => failed = Some(msg),
                Loaded::Done => done = true,
                Loaded::Header(..) => {}
            }
        }
        if !(done || failed.is_some()) {
            return;
        }
        self.lazy_loader = None;
        let pending = self.pending_after_load.take();
        if let Some(msg) = failed {
            self.status = format!("load failed: {msg}");
        } else if let Some(action) = pending {
            // The patch is loaded now, so these take their non-deferred path. (A
            // failed read leaves `full` unset, but that path is unreachable here —
            // `failed` is handled above — so this can't re-defer into a loop.)
            match action {
                Action::Audition(slot) => self.audition(slot),
                Action::CopyRow(slot) => self.copy_row(slot),
                _ => {}
            }
        }
    }

    /// The patch a store would write for `slot`: the staged whole-patch (from
    /// Paste or Clear) or the loaded patch, with the row's edited name and level
    /// overlaid. `None` if nothing is loaded for the row yet.
    fn effective_patch(&self, slot: u16) -> Option<TypedPatch> {
        if slot == SCRATCH {
            return Some(self.edit_scratch.clone());
        }
        let row = self.row(slot)?;
        let mut patch = row.pending_patch.clone().or_else(|| row.full.clone())?;
        patch.output_level = row.pending_level.unwrap_or(row.stored_level);
        patch.name.clone_from(&row.name_edit);
        Some(patch)
    }

    /// After a successful store, commit the edits: the written patch becomes the
    /// row's stored state, clearing every pending change (and the dirty flag).
    fn commit_row(&mut self, slot: u16) {
        let Some(patch) = self.effective_patch(slot) else {
            return;
        };
        if let Some(row) = self.row_mut(slot) {
            row.stored_level = patch.output_level;
            row.name.clone_from(&patch.name);
            row.name_edit.clone_from(&patch.name);
            row.chain = patch.chain.to_vec();
            row.full = Some(patch);
            row.pending_level = None;
            row.pending_patch = None;
        }
    }

    /// Spawn (or restart) the background bank read.
    /// Drop every row's staged edits, returning each to its last-stored state. Used
    /// when a Refresh is confirmed (the re-read would overwrite them anyway).
    fn discard_edits(&mut self) {
        for row in &mut self.rows {
            row.pending_patch = None;
            row.pending_level = None;
            row.name_edit.clone_from(&row.name);
        }
    }

    fn start_load(&mut self) {
        if !self.connected {
            return;
        }
        self.loader = None; // cancel + join any in-flight load first
        self.progress = 0;
        for row in &mut self.rows {
            row.failed = false; // clear stale marks; the re-read re-reports them
        }
        self.loader = Some(Loader::spawn(Arc::clone(&self.device)));
        "reading patch bank…".clone_into(&mut self.status);
    }

    fn retry(&mut self) {
        match (self.reopen)() {
            Ok(dev) => {
                self.loader = None;
                self.device = Arc::new(Mutex::new(dev));
                self.connected = true;
                self.now_playing = None;
                self.edit_slot = None;
                // Re-run the BULK LOAD probe (which then starts the bank read).
                self.bulk_ok = None;
                "checking BULK LOAD mode…".clone_into(&mut self.status);
            }
            Err(e) => self.status = format!("connect failed: {e}"),
        }
    }

    /// Write `slot`'s patch (including any staged Paste/Clear/level edits) into the
    /// current sound so it can be heard.
    fn audition(&mut self, slot: u16) {
        if !self.connected {
            return;
        }
        // A factory preset is read shallow; if its patch isn't loaded yet, read it
        // in the background and audition once it lands (don't block the UI thread).
        if !self.ensure_loaded_or_defer(slot, Action::Audition(slot)) {
            return;
        }
        let Some(patch) = self.effective_patch(slot) else {
            return;
        };
        let written = device::lock(&self.device).write_current_patch(&patch.to_raw());
        match written {
            Ok(_) => {
                self.now_playing = Some(slot);
                // Auditioning a bank slot also makes it the Edit tab's target.
                self.edit_slot = Some(slot);
                self.status = format!("auditioning {} {:?}", slot_label(slot), patch.name);
            }
            Err(e) => self.status = format!("audition {}: {e}", slot_label(slot)),
        }
    }

    /// Audition `slot` (if not already playing), set its level live, and record it
    /// as a pending change.
    fn set_level(&mut self, slot: u16, level: u8) {
        if self.now_playing != Some(slot) {
            self.audition(slot);
        }
        if self.now_playing == Some(slot)
            && let Some(param) = Param::from_key("output-level")
        {
            let result = device::lock(&self.device).set(param, Value::Int(i32::from(level)));
            if let Err(e) = result {
                self.status = format!("set level: {e}");
                return;
            }
        }
        // The level is overlaid by `effective_patch` at store/audition time, so we
        // only record it as pending here.
        if let Some(row) = self.row_mut(slot) {
            row.pending_level = Some(level);
        }
    }

    /// Set an effect parameter (by catalog key) on the now-playing patch: apply it
    /// live for instant audio, and stage it into the row's pending patch (via the
    /// typed model) so it saves/reverts with the rest.
    fn set_param(&mut self, slot: u16, key: &str, value: Value) {
        let Some(param) = Param::from_key(key) else {
            return;
        };
        // Offline scratch: a pure typed-model edit, no device, no preview.
        if slot == SCRATCH {
            if self.edit_scratch.set(key, value).is_ok() {
                self.status = format!("offline: {key} = {}", units::display(param, value));
            }
            return;
        }
        // Preview live only while this slot is the sound on the unit and we can write
        // (in BULK LOAD); otherwise just stage — so editing works offline.
        if self.now_playing == Some(slot)
            && self.editable()
            && let Err(e) = device::lock(&self.device).set(param, value)
        {
            self.status = format!("set {key}: {e}");
            return;
        }
        // Stage onto the row's base patch (no name/level overlay — those stay separate).
        let base = self
            .row(slot)
            .and_then(|r| r.pending_patch.clone().or_else(|| r.full.clone()));
        if let Some(mut typed) = base
            && typed.set(key, value).is_ok()
            && let Some(row) = self.row_mut(slot)
        {
            row.pending_patch = Some(typed);
        }
        self.status = format!("U{slot:03}: {key} = {}", units::display(param, value));
    }

    /// Move the effect block at chain position `from` to position `to` on the
    /// now-playing patch: re-order its chain, stage it into the row's pending
    /// patch, and re-audition so the new signal order is heard live.
    fn reorder_chain(&mut self, slot: u16, from: usize, to: usize) {
        if from == to {
            return;
        }
        // Resolve the patch to re-order: the offline scratch, or the bank row's.
        let base = if slot == SCRATCH {
            Some(self.edit_scratch.clone())
        } else {
            self.row(slot)
                .and_then(|r| r.pending_patch.clone().or_else(|| r.full.clone()))
        };
        let Some(mut base) = base else {
            return;
        };
        let mut chain = base.chain.to_vec();
        if from >= chain.len() || to >= chain.len() {
            return;
        }
        let id = chain.remove(from);
        chain.insert(to, id);
        if chain.len() != base.chain.len() {
            return;
        }
        base.chain.copy_from_slice(&chain);
        if slot == SCRATCH {
            self.edit_scratch = base;
            return;
        }
        if let Some(row) = self.row_mut(slot) {
            row.pending_patch = Some(base);
        }
        // Re-audition so the re-ordered chain is applied to the current sound.
        self.audition(slot);
    }

    /// Copy the now-playing patch's `block` into the additive block clipboard.
    fn copy_block(&mut self, block: Block) {
        let Some(slot) = self.edit_slot else {
            return;
        };
        let Some(src) = self.effective_patch(slot) else {
            return;
        };
        self.block_clip.copy_block_from(&src, block);
        if !self.clip_blocks.contains(&block) {
            self.clip_blocks.push(block);
        }
        self.status = format!("copied {} block", block.label());
    }

    /// Whether the block clipboard holds `block` (enables Paste).
    fn has_block_clip(&self, block: Block) -> bool {
        self.clip_blocks.contains(&block)
    }

    /// Whether `slot`'s `block` differs from its stored value (enables Revert).
    fn block_changed(&self, slot: u16, block: Block) -> bool {
        if slot == SCRATCH {
            return rackctl_gx700::typed::BlockData::from_patch(&self.edit_scratch, block)
                != rackctl_gx700::typed::BlockData::from_patch(&self.edit_base, block);
        }
        let Some(row) = self.row(slot) else {
            return false;
        };
        let Some(full) = &row.full else {
            return false;
        };
        let cur = row.pending_patch.as_ref().unwrap_or(full);
        rackctl_gx700::typed::BlockData::from_patch(cur, block)
            != rackctl_gx700::typed::BlockData::from_patch(full, block)
    }

    /// Replace `slot`'s `block` with the same block from `source`, stage it, and
    /// apply it live. Clears the staged patch if it returns to the stored state.
    fn apply_block(&mut self, slot: u16, block: Block, source: &TypedPatch) {
        if slot == SCRATCH {
            self.edit_scratch.copy_block_from(source, block);
            return;
        }
        let base = self
            .row(slot)
            .and_then(|r| r.pending_patch.clone().or_else(|| r.full.clone()));
        let Some(mut typed) = base else {
            return;
        };
        typed.copy_block_from(source, block);
        if let Some(row) = self.row_mut(slot) {
            row.pending_patch = if row.full.as_ref() == Some(&typed) {
                None
            } else {
                Some(typed)
            };
        }
        self.audition(slot);
    }

    /// Paste the clipboard's `block` onto `slot`'s same block.
    fn paste_block(&mut self, slot: u16, block: Block) {
        if !self.has_block_clip(block) {
            return;
        }
        let source = self.block_clip.clone();
        self.apply_block(slot, block, &source);
        self.status = format!("pasted {} block", block.label());
    }

    /// Revert `slot`'s `block` to its stored (on-unit) value.
    fn revert_block(&mut self, slot: u16, block: Block) {
        let stored = if slot == SCRATCH {
            self.edit_base.clone()
        } else {
            let Some(s) = self.row(slot).and_then(|r| r.full.clone()) else {
                return;
            };
            s
        };
        self.apply_block(slot, block, &stored);
        self.status = format!("reverted {} block", block.label());
    }

    // ---- On-disk library: single patches, single blocks, whole-bank scenes ----

    /// Write `typed` to the patch library under `name`. Returns whether it saved.
    fn save_patch_named(&mut self, name: &str, typed: &TypedPatch) -> bool {
        let name = name.trim();
        if name.is_empty() {
            "enter a name for the patch".clone_into(&mut self.status);
            return false;
        }
        let Some(path) = config::lib_path(config::patches_dir(), name) else {
            return false;
        };
        match config::save_item(&path, typed) {
            Ok(()) => {
                self.status = format!("saved patch \u{201c}{name}\u{201d}");
                true
            }
            Err(e) => {
                self.status = format!("save failed: {e}");
                false
            }
        }
    }

    /// Insert: save the now-playing patch as a new library patch (`lib_patch_name`).
    fn save_patch_lib(&mut self) {
        let Some(slot) = self.now_playing else {
            "audition a patch first".clone_into(&mut self.status);
            return;
        };
        let Some(typed) = self.effective_patch(slot) else {
            return;
        };
        let name = self.lib_patch_name.clone();
        if self.save_patch_named(&name, &typed) {
            self.lib_patch_name.clear();
        }
    }

    /// Overwrite an existing library patch `name` with the now-playing patch.
    fn save_patch_over(&mut self, name: &str) {
        let Some(slot) = self.now_playing else {
            "audition a patch first".clone_into(&mut self.status);
            return;
        };
        if let Some(typed) = self.effective_patch(slot) {
            self.save_patch_named(name, &typed);
        }
    }

    /// Copy a library patch `name` into the patch clipboard (paste onto a slot).
    fn copy_patch_lib(&mut self, name: &str) {
        let Some(path) = config::lib_path(config::patches_dir(), name) else {
            return;
        };
        match config::read_text(&path).as_deref().map(parse_patch_text) {
            Some(Ok(typed)) => {
                self.status = format!("copied patch \u{201c}{name}\u{201d} to the clipboard");
                self.clipboard = Some((0, typed));
            }
            Some(Err(e)) => self.status = format!("can't copy \u{201c}{name}\u{201d}: {e}"),
            None => self.status = format!("could not read patch \u{201c}{name}\u{201d}"),
        }
    }

    /// Paste: save the patch clipboard as a new library patch (`lib_patch_name`).
    fn paste_patch_lib(&mut self) {
        let Some((_, typed)) = self.clipboard.clone() else {
            "clipboard is empty — Copy a patch first".clone_into(&mut self.status);
            return;
        };
        let name = self.lib_patch_name.clone();
        if self.save_patch_named(&name, &typed) {
            self.lib_patch_name.clear();
        }
    }

    /// Load a library patch by `name` into the now-playing slot (staged).
    fn load_patch_lib(&mut self, name: &str) {
        let Some(slot) = self.now_playing else {
            "audition a patch first to load into that slot".clone_into(&mut self.status);
            return;
        };
        let Some(path) = config::lib_path(config::patches_dir(), name) else {
            return;
        };
        let loaded = match config::read_text(&path).as_deref().map(parse_patch_text) {
            Some(Ok(p)) => p,
            Some(Err(e)) => {
                self.status = format!("can't load \u{201c}{name}\u{201d}: {e}");
                return;
            }
            None => {
                self.status = format!("could not read patch \u{201c}{name}\u{201d}");
                return;
            }
        };
        if let Some(row) = self.row_mut(slot) {
            row.name_edit.clone_from(&loaded.name);
            row.pending_level = Some(loaded.output_level);
            row.pending_patch = Some(loaded);
        }
        self.audition(slot);
        self.status =
            format!("loaded patch \u{201c}{name}\u{201d} into U{slot:03} — Save to store");
    }

    /// Save the whole user bank (all 100 slots, with staged edits) as a named scene.
    /// The whole user bank (all slots' effective patches), or `None` if any slot
    /// isn't loaded yet (the deep bank read hasn't reached it).
    fn current_bank(&mut self) -> Option<Vec<TypedPatch>> {
        let mut patches = Vec::with_capacity(usize::from(USER_SLOTS));
        for slot in 1..=USER_SLOTS {
            let Some(patch) = self.effective_patch(slot) else {
                self.status =
                    format!("U{slot:03} not loaded yet — wait for the bank read to finish");
                return None;
            };
            patches.push(patch);
        }
        Some(patches)
    }

    /// Write `patches` to the scene library under `name`. Returns whether it saved.
    fn save_scene_named(&mut self, name: &str, patches: &[TypedPatch]) -> bool {
        let name = name.trim();
        if name.is_empty() {
            "enter a name for the scene".clone_into(&mut self.status);
            return false;
        }
        let Some(path) = config::lib_path(config::scenes_dir(), name) else {
            return false;
        };
        match config::save_item(&path, &patches.to_vec()) {
            Ok(()) => {
                self.status = format!(
                    "saved scene \u{201c}{name}\u{201d} ({} patches)",
                    patches.len()
                );
                true
            }
            Err(e) => {
                self.status = format!("save failed: {e}");
                false
            }
        }
    }

    /// Copy a library scene `name` into the scene clipboard.
    fn copy_scene(&mut self, name: &str) {
        let Some(path) = config::lib_path(config::scenes_dir(), name) else {
            return;
        };
        match config::read_text(&path).map(|t| parse_scene_text(&t)) {
            Some(Ok(patches)) => {
                self.status = format!("copied scene \u{201c}{name}\u{201d}");
                self.scene_clip = Some(patches);
            }
            Some(Err(e)) => self.status = format!("can't copy \u{201c}{name}\u{201d}: {e}"),
            None => self.status = format!("could not read scene \u{201c}{name}\u{201d}"),
        }
    }

    /// Paste: save the scene clipboard as a new scene file (named `compose_name`).
    fn paste_scene(&mut self) {
        let Some(patches) = self.scene_clip.clone() else {
            "clipboard is empty — Copy a scene first".clone_into(&mut self.status);
            return;
        };
        let name = self.compose_name.clone();
        if self.save_scene_named(&name, &patches) {
            self.compose_name.clear();
        }
    }

    /// Stage `patches` into the user-bank rows, one per slot (capped at `USER_SLOTS`),
    /// marking each slot dirty so a subsequent Write stores it. Returns the count.
    fn stage_scene(&mut self, patches: Vec<TypedPatch>) -> u16 {
        let mut staged = 0u16;
        for (i, patch) in patches.into_iter().enumerate() {
            let Ok(slot) = u16::try_from(i + 1) else {
                break;
            };
            if slot > USER_SLOTS {
                break;
            }
            if let Some(row) = self.row_mut(slot) {
                row.name_edit.clone_from(&patch.name);
                row.pending_level = Some(patch.output_level);
                row.pending_patch = Some(patch);
                staged += 1;
            }
        }
        staged
    }

    // ---- The offline scene composer (Scene tab) ----

    /// Start a fresh scene: every slot reset to an INIT patch.
    fn compose_new(&mut self) {
        self.compose = vec![TypedPatch::init(); usize::from(USER_SLOTS)];
        self.compose_base = self.compose.clone();
        self.compose_name.clear();
        "started a new scene (all slots INIT)".clone_into(&mut self.status);
    }

    /// Copy the current device bank into the composer (needs the bank fully read).
    fn compose_capture(&mut self) {
        if let Some(bank) = self.current_bank() {
            self.compose = bank;
            self.compose_base = self.compose.clone();
            "captured the current bank into the composer".clone_into(&mut self.status);
        }
    }

    /// Load a saved scene from the library into the composer for editing.
    fn compose_load(&mut self, name: &str) {
        let Some(path) = config::lib_path(config::scenes_dir(), name) else {
            return;
        };
        let patches = match config::read_text(&path).map(|t| parse_scene_text(&t)) {
            Some(Ok(p)) => p,
            Some(Err(e)) => {
                self.status = format!("can't load scene \u{201c}{name}\u{201d}: {e}");
                return;
            }
            None => {
                self.status = format!("could not read scene \u{201c}{name}\u{201d}");
                return;
            }
        };
        let mut slots = patches;
        // A scene file may be short or long; normalise to exactly one patch per slot.
        slots.resize(usize::from(USER_SLOTS), TypedPatch::init());
        self.compose = slots;
        self.compose_base = self.compose.clone();
        name.clone_into(&mut self.compose_name);
        self.status = format!("loaded scene \u{201c}{name}\u{201d} into the composer");
    }

    /// Place patch-library patch `name` into composer slot `idx` (0-based).
    fn compose_assign(&mut self, idx: usize, name: &str) {
        let Some(path) = config::lib_path(config::patches_dir(), name) else {
            return;
        };
        let loaded = match config::read_text(&path).as_deref().map(parse_patch_text) {
            Some(Ok(p)) => p,
            Some(Err(e)) => {
                self.status = format!("can't place \u{201c}{name}\u{201d}: {e}");
                return;
            }
            None => {
                self.status = format!("could not read patch \u{201c}{name}\u{201d}");
                return;
            }
        };
        match self.compose.get_mut(idx) {
            Some(slot) => *slot = loaded,
            None => return,
        }
        self.status = format!("placed \u{201c}{name}\u{201d} into U{:03}", idx + 1);
    }

    /// Place the live device-bank patch at `slot` into composer slot `idx`.
    fn compose_assign_bank(&mut self, idx: usize, slot: u16) {
        let Some(patch) = self.effective_patch(slot) else {
            self.status = format!("{} isn't loaded yet", slot_label(slot));
            return;
        };
        match self.compose.get_mut(idx) {
            Some(s) => *s = patch,
            None => return,
        }
        self.status = format!("placed {} into U{:03}", slot_label(slot), idx + 1);
    }

    /// Reset composer slot `idx` to an INIT patch.
    fn compose_clear(&mut self, idx: usize) {
        if let Some(slot) = self.compose.get_mut(idx) {
            *slot = TypedPatch::init();
        }
    }

    /// Move a composer slot from one position to another (drag re-order).
    fn compose_reorder(&mut self, from: usize, to: usize) {
        move_within(&mut self.compose, from, to);
    }

    /// Copy composer slot `idx`'s patch to the shared patch clipboard.
    fn compose_copy(&mut self, idx: usize) {
        if let Some(patch) = self.compose.get(idx).cloned() {
            self.clipboard = Some((0, patch));
            self.status = format!("copied U{:03}", idx + 1);
        }
    }

    /// Paste the clipboard patch into composer slot `idx`.
    fn compose_paste(&mut self, idx: usize) {
        let Some((_, patch)) = self.clipboard.clone() else {
            "clipboard is empty — Copy a patch first".clone_into(&mut self.status);
            return;
        };
        if let Some(slot) = self.compose.get_mut(idx) {
            *slot = patch;
            self.status = format!("pasted into U{:03}", idx + 1);
        }
    }

    /// Save the composed scene to the scene library under its current name.
    fn compose_save(&mut self) {
        let name = self.compose_name.clone();
        let scene = self.compose.clone();
        if self.save_scene_named(&name, &scene) {
            // The saved state is the new baseline for Revert.
            self.compose_base = scene;
        }
    }

    /// Overwrite an existing scene file `name` with the current composer.
    fn compose_save_over(&mut self, name: &str) {
        let scene = self.compose.clone();
        if self.save_scene_named(name, &scene) {
            self.compose_base = scene;
        }
    }

    /// Revert composer slot `idx` to its baseline (last New / Capture / Load / Save).
    fn compose_revert(&mut self, idx: usize) {
        let Some(base) = self.compose_base.get(idx).cloned() else {
            return;
        };
        if let Some(slot) = self.compose.get_mut(idx) {
            *slot = base;
            self.status = format!("reverted U{:03}", idx + 1);
        }
    }

    // ---- Offline patch editing (the Edit tab on a non-bank patch) ----

    /// Whether the Edit tab's controls should be enabled: a live bank slot needs the
    /// device editable (connected + BULK LOAD), but the offline scratch is always on.
    fn edit_enabled(&self) -> bool {
        self.edit_slot == Some(SCRATCH) || self.editable()
    }

    /// Open `patch` in the Edit tab as an offline edit saved back to `source`.
    fn open_offline(&mut self, patch: TypedPatch, source: OfflineSource) {
        self.edit_base = patch.clone();
        self.edit_scratch = patch;
        self.edit_source = Some(source);
        self.edit_slot = Some(SCRATCH);
        self.block_lib_open = false;
        self.tab = Tab::Edit;
    }

    /// Edit composer slot `idx` offline (Save writes back to that slot).
    fn edit_composer_slot(&mut self, idx: usize) {
        let Some(patch) = self.compose.get(idx).cloned() else {
            return;
        };
        self.open_offline(patch, OfflineSource::Composer(idx));
        self.status = format!("editing composer U{:03} offline", idx + 1);
    }

    /// Edit library patch `name` offline (Save writes back to the library file).
    fn edit_library_patch(&mut self, name: &str) {
        let Some(path) = config::lib_path(config::patches_dir(), name) else {
            return;
        };
        let loaded = match config::read_text(&path).as_deref().map(parse_patch_text) {
            Some(Ok(p)) => p,
            Some(Err(e)) => {
                self.status = format!("can't edit \u{201c}{name}\u{201d}: {e}");
                return;
            }
            None => {
                self.status = format!("could not read patch \u{201c}{name}\u{201d}");
                return;
            }
        };
        self.open_offline(loaded, OfflineSource::Library(name.to_owned()));
        self.status = format!("editing patch \u{201c}{name}\u{201d} offline");
    }

    /// Save the offline scratch back to its source (composer slot or library file).
    fn save_offline_edit(&mut self) {
        let Some(source) = self.edit_source.clone() else {
            return;
        };
        let patch = self.edit_scratch.clone();
        match source {
            OfflineSource::Composer(idx) => {
                if let Some(slot) = self.compose.get_mut(idx) {
                    *slot = patch;
                    self.edit_base = self.edit_scratch.clone();
                    self.status = format!("saved into composer U{:03}", idx + 1);
                }
            }
            OfflineSource::Library(name) => {
                if self.save_patch_named(&name, &patch) {
                    self.edit_base = self.edit_scratch.clone();
                }
            }
        }
    }

    /// Open device bank `slot` in the editor: audition it (so edits preview live),
    /// then switch to the editor screen. Needs the patch loaded (a connected bank
    /// read); offline there is no data to edit.
    fn edit_device_patch(&mut self, slot: u16) {
        self.ensure_loaded(slot);
        if self.effective_patch(slot).is_none() {
            "read the bank first to edit this patch".clone_into(&mut self.status);
            return;
        }
        // Best-effort live audition (sets edit_slot when it works); ensure the edit
        // target regardless so a loaded-but-not-auditioning patch is still editable.
        self.audition(slot);
        self.edit_slot = Some(slot);
        self.block_lib_open = false;
        self.tab = Tab::Edit;
    }

    /// Close the offline editor (discarding any unsaved scratch edits), returning to
    /// whichever tab the edit was opened from.
    fn close_offline_edit(&mut self) {
        self.tab = match self.edit_source {
            Some(OfflineSource::Composer(_)) => Tab::Scene,
            _ => Tab::Library,
        };
        self.edit_slot = None;
        self.edit_source = None;
        self.edit_scratch = TypedPatch::default();
        self.edit_base = TypedPatch::default();
    }

    /// Apply the composed scene to the unit: stage every slot, then batch-write
    /// (verified per slot, behind the progress bar) — replacing the whole bank.
    fn compose_apply(&mut self) {
        if !self.editable() {
            "connect and enter BULK LOAD mode before applying".clone_into(&mut self.status);
            return;
        }
        let scene = self.compose.clone();
        self.stage_scene(scene);
        self.start_write();
    }

    /// The selected block's current data (from the now-playing patch), if any.
    fn current_block_data(&self) -> Option<rackctl_gx700::typed::BlockData> {
        let typed = self.effective_patch(self.edit_slot?)?;
        rackctl_gx700::typed::BlockData::from_patch(&typed, self.selected_block)
    }

    /// Write `data` to the selected block's preset library as `name`.
    fn write_block_preset(&mut self, name: &str, data: &rackctl_gx700::typed::BlockData) {
        let name = name.trim();
        if name.is_empty() {
            "enter a name for the preset".clone_into(&mut self.status);
            return;
        }
        let Some(path) = config::lib_path(block_presets_dir(self.selected_block), name) else {
            return;
        };
        match config::save_item(&path, data) {
            Ok(()) => {
                self.status = format!(
                    "saved {} preset \u{201c}{name}\u{201d}",
                    self.selected_block.label()
                );
                self.lib_block_name.clear();
            }
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }

    /// Save the selected block (live) to its preset library as `name`.
    fn save_block_preset(&mut self, name: &str) {
        let Some(data) = self.current_block_data() else {
            "audition a patch first".clone_into(&mut self.status);
            return;
        };
        self.write_block_preset(name, &data);
    }

    /// Read a preset `name` for the selected block from its library.
    fn read_block_preset(&self, name: &str) -> Option<rackctl_gx700::typed::BlockData> {
        let path = config::lib_path(block_presets_dir(self.selected_block), name)?;
        let text = config::read_text(&path)?;
        parse_block_text(&text).ok()
    }

    /// Load a preset onto the now-playing patch's matching block (staged).
    fn load_block_preset(&mut self, name: &str) {
        let Some(slot) = self.edit_slot else {
            "audition or open a patch first".clone_into(&mut self.status);
            return;
        };
        let Some(data) = self.read_block_preset(name) else {
            self.status = format!("could not read preset \u{201c}{name}\u{201d}");
            return;
        };
        let block = data.block();
        let mut src = TypedPatch::default();
        data.apply_to(&mut src);
        self.apply_block(slot, block, &src);
        self.status = format!("loaded {} preset \u{201c}{name}\u{201d}", block.label());
    }

    /// Copy a preset into the block clipboard (so it can be pasted onto a block, or
    /// saved as another preset).
    fn copy_block_preset(&mut self, name: &str) {
        let Some(data) = self.read_block_preset(name) else {
            return;
        };
        let block = data.block();
        data.apply_to(&mut self.block_clip);
        if !self.clip_blocks.contains(&block) {
            self.clip_blocks.push(block);
        }
        self.status = format!("copied {} preset \u{201c}{name}\u{201d}", block.label());
    }

    /// Save the block clipboard's current-block data as a new preset `name`.
    fn paste_block_preset(&mut self, name: &str) {
        let block = self.selected_block;
        if !self.has_block_clip(block) {
            "clipboard is empty for this block".clone_into(&mut self.status);
            return;
        }
        let Some(data) = rackctl_gx700::typed::BlockData::from_patch(&self.block_clip, block)
        else {
            return;
        };
        self.write_block_preset(name, &data);
    }

    /// Delete the library file pending confirmation.
    fn confirm_delete(&mut self) {
        let Some(path) = self.pending_delete.take() else {
            return;
        };
        match config::delete_file(&path) {
            Ok(()) => "deleted".clone_into(&mut self.status),
            Err(e) => self.status = format!("delete failed: {e}"),
        }
    }

    fn set_name_edit(&mut self, slot: u16, name: String) {
        if let Some(row) = self.row_mut(slot) {
            row.name_edit = name;
        }
    }

    /// Save one patch (name + level) to the unit (per-row Save button), via the
    /// background writer — so the write + read-back verify never blocks the UI.
    fn save_row(&mut self, slot: u16) {
        if !self.row(slot).is_some_and(PatchRow::dirty) {
            return;
        }
        self.ensure_loaded(slot);
        let Some(raw) = self.effective_patch(slot).map(|t| t.to_raw()) else {
            self.status = format!("U{slot:03}: patch not loaded — audition it first");
            return;
        };
        self.start_write_for(vec![(slot, raw)]);
    }

    /// Revert one patch's edits (name, level, and any staged Paste/Clear) back to
    /// the state stored on the unit (per-row Revert button), re-previewing if it's
    /// the patch currently playing.
    fn revert_row(&mut self, slot: u16) {
        let Some(stored_name) = self.row(slot).map(|r| r.name.clone()) else {
            return;
        };
        if let Some(row) = self.row_mut(slot) {
            row.pending_level = None;
            row.pending_patch = None;
            row.name_edit = stored_name;
        }
        // The original patch is still in `full`, so re-audition restores the sound.
        if self.now_playing == Some(slot) {
            self.audition(slot);
        }
        self.status = format!("reverted U{slot:03}");
    }

    /// Copy `slot`'s patch (including any staged edits) into the clipboard.
    fn copy_row(&mut self, slot: u16) {
        // Copying a factory preset may need its shallow row's full patch read first;
        // do that in the background and re-run the copy once it lands.
        if !self.ensure_loaded_or_defer(slot, Action::CopyRow(slot)) {
            return;
        }
        match self.effective_patch(slot) {
            Some(patch) => {
                self.status = format!("copied {} {:?}", slot_label(slot), patch.name);
                self.clipboard = Some((slot, patch));
            }
            None => {
                self.status = format!("{}: nothing to copy — read it first", slot_label(slot));
            }
        }
    }

    /// Paste the clipboard patch into `slot` as a staged change (the original stays
    /// in `full` so Revert restores it), previewing it if the row is playing.
    fn paste_row(&mut self, slot: u16) {
        let Some((from, patch)) = self.clipboard.clone() else {
            "clipboard is empty — Copy a patch first".clone_into(&mut self.status);
            return;
        };
        let name = patch.name.clone();
        let level = patch.output_level;
        self.ensure_loaded(slot);
        if let Some(row) = self.row_mut(slot) {
            row.name_edit.clone_from(&name);
            row.pending_level = Some(level);
            row.pending_patch = Some(patch);
        }
        if self.now_playing == Some(slot) {
            self.audition(slot);
        }
        self.status = format!("pasted U{from:03} into U{slot:03} — Save to store");
    }

    /// Clear `slot` to an empty patch (name "Empty", level 0, all effects bypassed)
    /// as a staged change, previewing it if the row is playing.
    fn clear_row(&mut self, slot: u16) {
        self.ensure_loaded(slot);
        let Some(mut patch) = self.effective_patch(slot) else {
            self.status = format!("U{slot:03}: read the bank first");
            return;
        };
        patch.clear();
        let name = patch.name.clone();
        let level = patch.output_level;
        if let Some(row) = self.row_mut(slot) {
            row.name_edit.clone_from(&name);
            row.pending_level = Some(level);
            row.pending_patch = Some(patch);
        }
        if self.now_playing == Some(slot) {
            self.audition(slot);
        }
        self.status = format!("cleared U{slot:03} to Empty — Save to store");
    }

    /// Move the patch at `from_slot` to `to_slot`, shifting the slots in between by
    /// one to fill the gap. The new contents are *staged* per slot (exactly like
    /// Paste) so nothing is written until the user runs "Write changes" in BULK LOAD
    /// mode, and each affected row's Revert still restores its original patch.
    fn reorder_patches(&mut self, from_slot: u16, to_slot: u16) {
        if from_slot == to_slot {
            return;
        }
        let from = usize::from(from_slot.saturating_sub(1));
        let to = usize::from(to_slot.saturating_sub(1));
        if from >= self.rows.len() || to >= self.rows.len() {
            return;
        }
        let (lo, hi) = (from.min(to), from.max(to));
        // Snapshot every affected slot's current content (loading on demand). Bail
        // as a whole if any can't be read, so we never stage a partial reorder.
        let mut contents = Vec::with_capacity(hi - lo + 1);
        for idx in lo..=hi {
            let slot = u16::try_from(idx + 1).unwrap_or(0);
            self.ensure_loaded(slot);
            let Some(patch) = self.effective_patch(slot) else {
                self.status =
                    format!("U{slot:03}: read the bank first — reorder needs every patch loaded");
                return;
            };
            contents.push(patch);
        }
        // Rotate the window: the dragged patch lands at `to`, the rest shift by one.
        let moved = contents.remove(from - lo);
        contents.insert(to - lo, moved);
        // Stage each affected slot with its new content (name + level + patch).
        for (patch, idx) in contents.into_iter().zip(lo..=hi) {
            let slot = u16::try_from(idx + 1).unwrap_or(0);
            let name = patch.name.clone();
            let level = patch.output_level;
            if let Some(row) = self.row_mut(slot) {
                row.name_edit = name;
                row.pending_level = Some(level);
                row.pending_patch = Some(patch);
            }
        }
        // Re-audition the now-playing slot if it sits inside the moved window.
        if let Some(playing) = self.now_playing {
            let pi = usize::from(playing.saturating_sub(1));
            if (lo..=hi).contains(&pi) {
                self.audition(playing);
            }
        }
        self.status = format!(
            "moved U{from_slot:03} → U{to_slot:03}; {} patches staged — Write changes in BULK LOAD",
            hi - lo + 1
        );
    }

    /// Store every pending change (name + level) to memory in one batch (the
    /// "Write changes to unit" button). Attempts every dirty row even if some fail,
    /// committing each success so its row clears, and reports any failures. Returns
    /// the number of patches successfully stored.
    fn start_write(&mut self) {
        // Capture the effective bytes of every dirty slot now, and hand them to the
        // background writer (so the UI stays responsive and shows progress).
        let writes: Vec<(u16, RawPatch)> = (1..=USER_SLOTS)
            .filter(|&slot| self.row(slot).is_some_and(PatchRow::dirty))
            .filter_map(|slot| self.effective_patch(slot).map(|t| (slot, t.to_raw())))
            .collect();
        self.start_write_for(writes);
    }

    /// Spawn the background writer for `writes` (off the UI thread, with read-back
    /// verify and a progress bar). Rows commit / fail as `drain_write` processes the
    /// results. Shared by the batch "Write changes" and the per-row Save.
    fn start_write_for(&mut self, writes: Vec<(u16, RawPatch)>) {
        if writes.is_empty() {
            "no pending changes to store".clone_into(&mut self.status);
            return;
        }
        let total = writes.len();
        self.write_progress = 0;
        self.write_stored = 0;
        self.write_failed.clear();
        self.writer = Some(Writer::spawn(Arc::clone(&self.device), writes));
        self.status = format!("writing {total} patch(es) to the unit…");
    }

    /// Drain the background writer, committing each stored slot and reporting when
    /// the batch finishes.
    fn drain_write(&mut self) {
        let results = match &self.writer {
            Some(w) => w.drain(),
            None => return,
        };
        let mut done = false;
        for ev in results {
            match ev {
                Written::Ok(slot) => {
                    self.commit_row(slot);
                    self.write_stored += 1;
                    self.write_progress += 1;
                }
                Written::Failed(slot) => {
                    self.write_failed.push(slot);
                    self.write_progress += 1;
                }
                Written::Done => done = true,
            }
        }
        if done {
            self.writer = None;
            self.save_cache();
            if self.write_failed.is_empty() {
                self.status = format!("stored {} patch change(s)", self.write_stored);
            } else {
                // A failed store almost always means the unit dropped out of BULK
                // LOAD mode: re-block on the probe so the dialog guides the user back.
                self.bulk_ok = Some(false);
                let slots: Vec<String> = self.write_failed.iter().map(|s| slot_label(*s)).collect();
                self.status = format!(
                    "stored {}, {} failed ({}) — put the GX-700 in BULK LOAD mode",
                    self.write_stored,
                    self.write_failed.len(),
                    slots.join(", ")
                );
            }
        }
    }

    fn save_cache(&self) {
        let to_cached = |r: &PatchRow| CachedRow {
            slot: r.slot,
            name: r.name.clone(),
            output_level: r.stored_level,
            chain: r.chain.clone(),
        };
        // User rows always; preset rows only once read (non-empty name), so the
        // cache doubles as the "presets already loaded" marker on next launch.
        let mut rows: Vec<CachedRow> = self.rows.iter().map(to_cached).collect();
        rows.extend(
            self.presets
                .iter()
                .filter(|r| !r.name.is_empty())
                .map(to_cached),
        );
        config::save_cache(&rows);
    }

    /// Render the scrollable patch list (name, level slider, Save/Revert), pushing
    /// any interactions into `actions` to apply after the render pass.
    /// The Edit tab's left column: the now-playing patch's effect blocks in chain
    /// (signal) order, selectable, each marked with its enable state.
    fn show_block_list(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.heading("Effect blocks");
        let Some(slot) = self.edit_slot else {
            ui.label("Audition a patch, or open one offline, to edit it here.");
            return;
        };
        let Some(typed) = self.effective_patch(slot) else {
            return;
        };
        let where_label = if slot == SCRATCH {
            "offline".to_owned()
        } else {
            format!("U{slot:03}")
        };
        ui.label(format!("{where_label}  {:?}", typed.name));
        ui.label(egui::RichText::new("Drag the ↕ handle to re-order the chain.").weak());
        ui.separator();
        // The patch-common section (output level + the 4 control assigns). It isn't
        // part of the effect chain, so it has no handle/bypass — just selectable.
        if ui
            .selectable_label(
                self.selected_block == Block::LevelChain,
                Block::LevelChain.label(),
            )
            .on_hover_text("patch output level and control assigns")
            .clicked()
        {
            actions.push(Action::SelectBlock(Block::LevelChain));
        }
        ui.separator();
        // Drag-to-reorder: a separate ↕ handle per row is the drag source carrying
        // the chain index; the whole row is the drop target. Keeping the handle off
        // the name's rect means the name's click (select) isn't stolen by the drag.
        let mut reorder: Option<(usize, usize)> = None;
        for (idx, &id) in typed.chain.iter().enumerate() {
            let Some(block) = Block::from_base(id) else {
                continue;
            };
            let enabled = block_enabled(&typed, block);
            let selected = self.selected_block == block;
            let cell = ui.horizontal(|ui| {
                let drag_id = egui::Id::new(("chain-drag", idx));
                ui.add_enabled_ui(self.edit_enabled(), |ui| {
                    ui.dnd_drag_source(drag_id, idx, |ui| {
                        ui.label(egui::RichText::new("↕").weak());
                    })
                    .response
                    .on_hover_text("drag to re-order the chain");
                });
                // A checkbox toggles the block's bypass directly; the name selects it.
                if let Some(p) = block_enable_param(block) {
                    ui.add_enabled_ui(self.edit_enabled(), |ui| {
                        let mut on = enabled;
                        if ui.checkbox(&mut on, "").changed() {
                            actions.push(Action::SetParam(slot, p.key(), Value::Bool(on)));
                        }
                    });
                }
                let label = if enabled {
                    egui::RichText::new(block.label())
                } else {
                    egui::RichText::new(block.label()).weak()
                };
                if ui.selectable_label(selected, label).clicked() {
                    actions.push(Action::SelectBlock(block));
                }
            });
            // Explicit hover-sensing drop target over the row (a bare layout response
            // misses drops released over the row's interactive children).
            let drop = ui.interact(
                cell.response.rect,
                egui::Id::new(("chain-drop", idx)),
                egui::Sense::hover(),
            );
            if self.edit_enabled()
                && let Some(from) = drop.dnd_release_payload::<usize>()
            {
                reorder = Some((*from, idx));
            }
        }
        if let Some((from, to)) = reorder {
            actions.push(Action::ReorderChain(slot, from, to));
        }
    }

    /// The Edit tab's main area: the selected block's parameters as live widgets.
    /// Values are read through the typed model; edits stage like the balancer.
    /// The offline-edit banner shown above the block editor: where it saves back to,
    /// plus Save and Close. Save is disabled until the scratch differs from the base.
    fn show_offline_banner(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let where_to = match &self.edit_source {
            Some(OfflineSource::Composer(idx)) => format!("composer U{:03}", idx + 1),
            Some(OfflineSource::Library(name)) => format!("library \u{201c}{name}\u{201d}"),
            None => "nowhere".to_owned(),
        };
        let dirty = self.edit_scratch != self.edit_base;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Offline edit \u{2192} {where_to} (no preview)"))
                    .weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if action_button(ui, "Close", ActionKind::Neutral)
                    .on_hover_text("close the offline editor (unsaved edits are discarded)")
                    .clicked()
                {
                    actions.push(Action::CloseOfflineEdit);
                }
                ui.add_enabled_ui(dirty, |ui| {
                    if action_button(ui, "Save", ActionKind::Commit)
                        .on_hover_text("write the edited patch back to its source")
                        .clicked()
                    {
                        actions.push(Action::SaveOfflineEdit);
                    }
                });
            });
        });
        ui.separator();
    }

    /// The banner above the block editor identifying what's being edited and the way
    /// back (the editor is no longer a tab): offline edits get Save + Close, a live
    /// device-bank slot gets Done.
    fn show_edit_banner(&self, ui: &mut egui::Ui, slot: u16, actions: &mut Vec<Action>) {
        if slot == SCRATCH {
            self.show_offline_banner(ui, actions);
            return;
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Editing {} (live)", slot_label(slot))).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if action_button(ui, "Done", ActionKind::Neutral)
                    .on_hover_text("back to the patch list")
                    .clicked()
                {
                    actions.push(Action::SelectTab(Tab::Patches));
                }
            });
        });
        ui.separator();
    }

    /// The patch-common Level/Chain editor: output level, then the four control
    /// assigns as a grid — one row per field (target, source, mode, the min/max the
    /// target sweeps between, the action lo/hi range), one column per assign.
    fn show_levelchain_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let enabled = self.edit_enabled();
        egui::Grid::new("gx700-lc-level")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Output level")
                    .on_hover_text("Patch master level (also on the patch row).");
                param_drag(ui, slot, "output-level", typed, enabled, actions);
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Control assigns — route a Source controller to a Target parameter, \
                 sweeping it between Min and Max while the Source is within Action lo..hi.",
            )
            .weak(),
        );

        // Schematic for the selected assign, above the grid — value-, mode- and
        // range-aware. (Editing a grid cell auto-selects its assign, so this updates.)
        let n = self.selected_assign + 1;
        let raw = |suffix: &str| -> i32 {
            match typed.get(&format!("assign{n}-{suffix}")) {
                Some(Value::Int(v) | Value::Enum(v)) => v,
                _ => 0,
            }
        };
        let (lo, hi) = (raw("act-lo"), raw("act-hi"));
        let (min, max) = (raw("min"), raw("max"));
        let toggle = raw("mode") == 1;
        // Scale Min/Max within the selected target's value range; with no target set,
        // fall back to the Min/Max span so the schematic still fills the height.
        let range = rackctl_gx700::param::assign_target_range(raw("target"))
            .unwrap_or((min.min(max), min.max(max)));
        ui.label(
            egui::RichText::new(format!(
                "Assign {n} — {}",
                if toggle { "Toggle" } else { "Normal" }
            ))
            .strong(),
        );
        show_assign_schematic(ui, lo, hi, min, max, range, toggle);
        ui.add_space(4.0);

        egui::Grid::new("gx700-assigns")
            .num_columns(5)
            .spacing([12.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                for i in 0..4 {
                    if ui
                        .selectable_label(
                            self.selected_assign == i,
                            egui::RichText::new(format!("Assign {}", i + 1)).strong(),
                        )
                        .on_hover_text("show this assign in the schematic above")
                        .clicked()
                    {
                        actions.push(Action::SelectAssign(i));
                    }
                }
                ui.end_row();
                for (label, keys) in ASSIGN_ROWS {
                    ui.label(label);
                    for (col, key) in keys.into_iter().enumerate() {
                        let changed = if key.ends_with("-target") {
                            param_target_combo(ui, slot, key, typed, enabled, actions)
                        } else if key.ends_with("-mode") {
                            param_combo(ui, slot, key, typed, enabled, actions)
                        } else {
                            param_drag(ui, slot, key, typed, enabled, actions)
                        };
                        // Editing a cell selects its assign, so the schematic above
                        // reflects what you're tweaking.
                        if changed {
                            actions.push(Action::SelectAssign(col));
                        }
                    }
                    ui.end_row();
                }
            });
    }

    fn show_block_params(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(slot) = self.edit_slot else {
            ui.label(
                "Audition a patch to edit it live, or open one offline from the Scene tab \
                 or the Library tab.",
            );
            return;
        };
        let Some(typed) = self.effective_patch(slot) else {
            return;
        };
        self.show_edit_banner(ui, slot, actions);
        let block = self.selected_block;
        // Title row: the block name, with right-aligned Copy / Paste / Revert (the
        // additive block clipboard) and Library (this block type's preset library).
        ui.horizontal(|ui| {
            ui.heading(block.label());
            // The block clipboard / preset library applies to effect blocks only, not
            // the patch-common Level/Chain section.
            if block != Block::LevelChain {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let edit = self.edit_enabled();
                    // Added right-to-left, so they read Copy · Paste · Revert · Library.
                    if ui
                        .selectable_label(self.block_lib_open, "Library")
                        .on_hover_text("this effect's saved presets")
                        .clicked()
                    {
                        actions.push(Action::ToggleBlockLib);
                    }
                    ui.add_enabled_ui(edit && self.block_changed(slot, block), |ui| {
                        if action_button(ui, "Revert", ActionKind::Caution)
                            .on_hover_text("discard this block's edits, back to the stored values")
                            .clicked()
                        {
                            actions.push(Action::RevertBlock(slot, block));
                        }
                    });
                    ui.add_enabled_ui(edit && self.has_block_clip(block), |ui| {
                        if action_button(ui, "Paste", ActionKind::Neutral)
                            .on_hover_text(format!("paste the copied {} block here", block.label()))
                            .clicked()
                        {
                            actions.push(Action::PasteBlock(slot, block));
                        }
                    });
                    ui.add_enabled_ui(edit, |ui| {
                        if action_button(ui, "Copy", ActionKind::Read)
                            .on_hover_text("copy this block (combine blocks from other patches)")
                            .clicked()
                        {
                            actions.push(Action::CopyBlock(block));
                        }
                    });
                });
            }
        });
        ui.separator();
        if self.block_lib_open && block != Block::LevelChain {
            self.show_block_library(ui, slot, block, actions);
            return;
        }
        self.show_block_editor(ui, slot, block, &typed, actions);
    }

    /// Render the selected block's parameter editor inside the scrolling area: a
    /// custom layout for blocks that have one, else the generic per-parameter list.
    fn show_block_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        block: Block,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if block == Block::LevelChain {
                    self.show_levelchain_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Equalizer {
                    self.show_eq_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Compressor {
                    self.show_comp_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::NoiseSuppressor {
                    self.show_ns_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Reverb {
                    self.show_reverb_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Distortion {
                    self.show_dist_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Preamp {
                    self.show_preamp_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Delay {
                    self.show_delay_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::SpeakerSim {
                    self.show_speaker_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Wah {
                    self.show_wah_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Loop {
                    self.show_loop_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Chorus {
                    self.show_chorus_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::TremoloPan {
                    self.show_trem_editor(ui, slot, typed, actions);
                    return;
                }
                if block == Block::Modulation {
                    self.show_mod_editor(ui, slot, typed, actions);
                    return;
                }
                for &p in param::ALL {
                    if p.block() != block {
                        continue;
                    }
                    let value = typed.get(p.key()).unwrap_or(Value::Int(0));
                    param_widget(ui, slot, p, value, self.edit_enabled(), actions);
                }
            });
    }

    /// The selected block's preset library (shown when its "Library" is open):
    /// insert a new preset from the live block, paste one from the clipboard, and
    /// load / overwrite / copy / delete the saved presets for this effect type.
    fn show_block_library(
        &mut self,
        ui: &mut egui::Ui,
        _slot: u16,
        block: Block,
        actions: &mut Vec<Action>,
    ) {
        ui.label(
            egui::RichText::new(format!(
                "{} presets — saved on this computer, one library per effect type.",
                block.label()
            ))
            .weak(),
        );
        let has_clip = self.has_block_clip(block);
        ui.horizontal(|ui| {
            ui.label("New preset:");
            ui.add(
                egui::TextEdit::singleline(&mut self.lib_block_name)
                    .hint_text("name")
                    .desired_width(150.0),
            );
            let name = self.lib_block_name.trim().to_owned();
            let named = !name.is_empty();
            ui.add_enabled_ui(named, |ui| {
                if action_button(ui, "Insert", ActionKind::Commit)
                    .on_hover_text("save the current block as a new preset")
                    .clicked()
                {
                    actions.push(Action::SaveBlockPreset(name.clone()));
                }
            });
            ui.add_enabled_ui(named && has_clip, |ui| {
                if action_button(ui, "Paste", ActionKind::Neutral)
                    .on_hover_text("save the copied block as a new preset")
                    .clicked()
                {
                    actions.push(Action::PasteBlockPreset(name.clone()));
                }
            });
        });
        ui.separator();
        let names = config::json_stems(block_presets_dir(block));
        if names.is_empty() {
            ui.label(egui::RichText::new("No presets for this effect yet.").weak());
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for name in &names {
                    ui.horizontal(|ui| {
                        if action_button(ui, icon::LOAD, ActionKind::Read)
                            .on_hover_text("apply this preset to the block")
                            .clicked()
                        {
                            actions.push(Action::LoadBlockPreset(name.clone()));
                        }
                        if action_button(ui, icon::SAVE, ActionKind::Commit)
                            .on_hover_text("overwrite this preset with the current block")
                            .clicked()
                        {
                            actions.push(Action::SaveBlockPreset(name.clone()));
                        }
                        if action_button(ui, icon::COPY, ActionKind::Read)
                            .on_hover_text("copy this preset to the clipboard")
                            .clicked()
                        {
                            actions.push(Action::CopyBlockPreset(name.clone()));
                        }
                        if action_button(ui, icon::DELETE, ActionKind::Destructive)
                            .on_hover_text("delete this preset")
                            .clicked()
                            && let Some(dir) = block_presets_dir(block)
                        {
                            actions.push(Action::RequestDelete(
                                dir.join(format!("{}.json", config::sanitize(name))),
                            ));
                        }
                        ui.label(name);
                    });
                }
            });
    }

    /// The Equalizer's custom UI: enable, the response curve, then a band table
    /// (Gain / Freq / Q per band, high → low) in the US-16x08 style — drag-values
    /// for gains, combos for the mid band's frequency and Q.
    fn show_eq_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Equalizer);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "EQ enabled").changed() {
                actions.push(Action::SetParam(slot, "eq-enable", Value::Bool(on)));
            }
        });
        show_eq_curve(ui, typed);
        ui.add_space(6.0);
        egui::Grid::new("gx700-eq-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("");
                ui.strong("Gain");
                ui.strong("Freq");
                ui.strong("Q");
                ui.end_row();

                // Shelves (no frequency or Q control on the device); the mid band
                // is a sweepable peak with a Q.
                eq_row(
                    ui,
                    slot,
                    "High",
                    "eq-high-gain",
                    None,
                    None,
                    typed,
                    connected,
                    actions,
                );
                eq_row(
                    ui,
                    slot,
                    "Mid",
                    "eq-mid-gain",
                    Some("eq-mid-freq"),
                    Some("eq-mid-q"),
                    typed,
                    connected,
                    actions,
                );
                eq_row(
                    ui,
                    slot,
                    "Low",
                    "eq-low-gain",
                    None,
                    None,
                    typed,
                    connected,
                    actions,
                );

                ui.separator();
                ui.end_row();
                ui.label("Level");
                param_drag(ui, slot, "eq-level", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Compressor's custom UI: enable + type, an indicative transfer curve,
    /// then the mode-relevant controls (Sustain/Attack for Compressor, Threshold/
    /// Release for Limiter) plus Tone and Level, in the US-16x08 style.
    fn show_comp_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Compressor);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Compressor enabled").changed() {
                actions.push(Action::SetParam(slot, "comp-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Type");
                param_combo(ui, slot, "comp-type", typed, connected, actions);
            });
        });
        // comp-type: 0 = Compressor, 1 = Limiter.
        let limiter = matches!(typed.get("comp-type"), Some(Value::Enum(1)));
        show_comp_curve(ui, typed, limiter);
        ui.add_space(6.0);
        egui::Grid::new("gx700-comp-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                if limiter {
                    ui.label("Threshold");
                    param_drag(ui, slot, "comp-threshold", typed, connected, actions);
                    ui.label("Release");
                    param_drag(ui, slot, "comp-release", typed, connected, actions);
                } else {
                    ui.label("Sustain").on_hover_text(
                        "Compression amount — the GX-700 has no separate ratio control.",
                    );
                    param_drag(ui, slot, "comp-sustain", typed, connected, actions);
                    ui.label("Attack");
                    param_drag(ui, slot, "comp-attack", typed, connected, actions);
                }
                ui.end_row();
                ui.label("Tone");
                param_drag(ui, slot, "comp-tone", typed, connected, actions);
                ui.label("Level")
                    .on_hover_text("Compressor output level — acts as the make-up gain.");
                param_drag(ui, slot, "comp-level", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Noise Suppressor's custom UI: enable + detection source, an indicative
    /// gate-response curve (signals below the threshold are suppressed), then the
    /// Threshold / Release / Level controls with explanatory tooltips.
    fn show_ns_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::NoiseSuppressor);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Noise Suppressor enabled").changed() {
                actions.push(Action::SetParam(slot, "ns-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Detect").on_hover_text(
                    "Which signal the gate listens to when deciding whether to open or close.",
                );
                param_combo(ui, slot, "ns-detect", typed, connected, actions);
            });
        });
        show_ns_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Indicative — below the threshold the signal is suppressed.")
                .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-ns-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Threshold").on_hover_text(
                    "Signals below this level are suppressed — raise it to cut more noise \
                     (too high also chokes quiet playing and sustain).",
                );
                param_drag(ui, slot, "ns-threshold", typed, connected, actions);
                ui.label("Release").on_hover_text(
                    "How fast the gate closes once the signal drops below the threshold.",
                );
                param_drag(ui, slot, "ns-release", typed, connected, actions);
                ui.end_row();
                ui.label("Level").on_hover_text(
                    "Output volume of this block — a mid-chain gain stage (0–100), \
                     not make-up gain. It sits inside the signal chain, so turning it \
                     down attenuates everything downstream. For overall patch loudness \
                     use the master output level on the Patches tab.",
                );
                param_drag(ui, slot, "ns-level", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Reverb's custom UI: enable + mode, a decay-envelope graphic (pre-delay
    /// gap, then a tail decaying over the reverb Time), then the time/tone controls
    /// and the wet (Effect) / dry (Direct) level mix.
    fn show_reverb_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Reverb);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Reverb enabled").changed() {
                actions.push(Action::SetParam(slot, "reverb-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Mode").on_hover_text(
                    "Reverb character — small rooms through large halls and a plate.",
                );
                param_combo(ui, slot, "reverb-mode", typed, connected, actions);
            });
        });
        show_reverb_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Indicative — dry spike at 0, then the wet tail decays over Time.")
                .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-reverb-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Time")
                    .on_hover_text("Reverb decay length, 0.1–10.0 s.");
                param_drag(ui, slot, "reverb-time", typed, connected, actions);
                ui.label("Pre-delay")
                    .on_hover_text("Gap before the reverb tail starts (0–100 ms).");
                param_drag(ui, slot, "reverb-pre-delay", typed, connected, actions);
                ui.end_row();
                ui.label("Low cut")
                    .on_hover_text("Roll off the tail below this frequency (thins boom).");
                param_combo(ui, slot, "reverb-low-cut", typed, connected, actions);
                ui.label("Hi cut")
                    .on_hover_text("Roll off the tail above this frequency (darkens the tail).");
                param_combo(ui, slot, "reverb-hi-cut", typed, connected, actions);
                ui.end_row();
                ui.label("Diffusion")
                    .on_hover_text("Density of the reflections, 0–10 (lower = grainier).");
                param_drag(ui, slot, "reverb-diffusion", typed, connected, actions);
                ui.label("");
                ui.label("");
                ui.end_row();
                ui.label("Effect (wet)")
                    .on_hover_text("Level of the reverb tail in the mix.");
                param_drag(ui, slot, "reverb-effect-level", typed, connected, actions);
                ui.label("Direct (dry)")
                    .on_hover_text("Level of the un-reverbed signal in the mix.");
                param_drag(ui, slot, "reverb-direct-level", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Distortion's custom UI: enable + type, an indicative waveshaper transfer
    /// curve that hardens with Drive, then Drive / Level and the Bass / Treble tone
    /// controls (each −50…+50).
    fn show_dist_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Distortion);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Distortion enabled").changed() {
                actions.push(Action::SetParam(slot, "dist-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Type")
                    .on_hover_text("Overdrive / distortion voicing, from light overdrive to fuzz.");
                param_combo(ui, slot, "dist-type", typed, connected, actions);
            });
        });
        show_dist_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(
                "Schematic — shape estimated by type (hand-picked, not measured) and Drive.",
            )
            .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-dist-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Drive").on_hover_text(
                    "Amount of overdrive/distortion driven into the clipping stage.",
                );
                param_drag(ui, slot, "dist-drive", typed, connected, actions);
                ui.label("Level").on_hover_text(
                    "Output volume of this block (a mid-chain gain stage). For overall \
                     patch loudness use the master output level on the Patches tab.",
                );
                param_drag(ui, slot, "dist-level", typed, connected, actions);
                ui.end_row();
                ui.label("Bass")
                    .on_hover_text("Low-frequency tone, −50…+50.");
                param_drag(ui, slot, "dist-bass", typed, connected, actions);
                ui.label("Treble")
                    .on_hover_text("High-frequency tone, −50…+50.");
                param_drag(ui, slot, "dist-treble", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Preamp's custom UI: enable + amp model, gain range and Bright switch, an
    /// indicative tone-stack response curve, then the Bass/Middle/Treble/Presence
    /// tone controls and the Volume / Master gain stages.
    fn show_preamp_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Preamp);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Preamp enabled").changed() {
                actions.push(Action::SetParam(slot, "preamp-enable", Value::Bool(on)));
            }
            egui::Grid::new("gx700-preamp-head")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Model").on_hover_text(
                        "Amp model — cleans (JC-120, Clean Twin) through high gain (Metal 5150).",
                    );
                    param_combo(ui, slot, "preamp-type", typed, connected, actions);
                    ui.end_row();
                    ui.label("Gain").on_hover_text(
                        "Input gain range: Low / Mid / Hi — the overall drive amount.",
                    );
                    param_combo(ui, slot, "preamp-gain", typed, connected, actions);
                    ui.end_row();
                    ui.label("Bright").on_hover_text(
                        "Bright switch — extra high-end sparkle (mostly on the clean models).",
                    );
                    let mut bright = matches!(typed.get("preamp-bright"), Some(Value::Bool(true)));
                    if ui.checkbox(&mut bright, "").changed() {
                        actions.push(Action::SetParam(slot, "preamp-bright", Value::Bool(bright)));
                    }
                    ui.end_row();
                });
        });
        show_preamp_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(
                "Indicative tone stack — the real amp models interact differently.",
            )
            .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-preamp-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Bass");
                param_drag(ui, slot, "preamp-bass", typed, connected, actions);
                ui.label("Middle");
                param_drag(ui, slot, "preamp-middle", typed, connected, actions);
                ui.end_row();
                ui.label("Treble");
                param_drag(ui, slot, "preamp-treble", typed, connected, actions);
                ui.label("Presence")
                    .on_hover_text("Brilliance / top-end edge above the treble band.");
                param_drag(ui, slot, "preamp-presence", typed, connected, actions);
                ui.end_row();
                ui.label("Volume")
                    .on_hover_text("Preamp volume — the main drive into the amp model.");
                param_drag(ui, slot, "preamp-volume", typed, connected, actions);
                ui.label("Master")
                    .on_hover_text("Preamp master output level (after the tone stack).");
                param_drag(ui, slot, "preamp-master", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Delay's custom UI: enable + mode, a 3-tap diagram (centre echoes with
    /// feedback, plus the L/R taps), then the timing (ms or tempo-synced), the
    /// per-tap levels, feedback, tone (high-damp / hi-cut / smooth), and wet/dry.
    fn show_delay_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Delay);
        let tempo = matches!(typed.get("delay-mode"), Some(Value::Enum(1)));
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Delay enabled").changed() {
                actions.push(Action::SetParam(slot, "delay-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Mode").on_hover_text(
                    "Normal = centre time set in ms; Tempo = synced to a BPM + note value.",
                );
                param_combo(ui, slot, "delay-mode", typed, connected, actions);
            });
        });
        show_delay_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Tap diagram — centre echoes (blue), Left (green), Right (red).")
                .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-delay-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                if tempo {
                    ui.label("Tempo").on_hover_text("Delay tempo, 50–300 BPM.");
                    param_drag(ui, slot, "delay-tempo", typed, connected, actions);
                    ui.label("Interval")
                        .on_hover_text("Note value of the centre tap, synced to the tempo.");
                    param_combo(ui, slot, "delay-interval-c", typed, connected, actions);
                } else {
                    ui.label("Time (C)")
                        .on_hover_text("Centre delay time, in ms.");
                    param_drag(ui, slot, "delay-time-c", typed, connected, actions);
                    ui.label("");
                    ui.label("");
                }
                ui.end_row();
                ui.label("Time L")
                    .on_hover_text("Left tap time, as a % of the centre time.");
                param_drag(ui, slot, "delay-time-l", typed, connected, actions);
                ui.label("Time R")
                    .on_hover_text("Right tap time, as a % of the centre time.");
                param_drag(ui, slot, "delay-time-r", typed, connected, actions);
                ui.end_row();
                ui.label("Level C");
                param_drag(ui, slot, "delay-level-c", typed, connected, actions);
                ui.label("Feedback")
                    .on_hover_text("How much the centre tap is fed back — the number of repeats.");
                param_drag(ui, slot, "delay-feedback", typed, connected, actions);
                ui.end_row();
                ui.label("Level L");
                param_drag(ui, slot, "delay-level-l", typed, connected, actions);
                ui.label("Level R");
                param_drag(ui, slot, "delay-level-r", typed, connected, actions);
                ui.end_row();
                ui.label("High damp")
                    .on_hover_text("Rolls off the highs as the repeats decay (−50…0 dB).");
                param_drag(ui, slot, "delay-high-damp", typed, connected, actions);
                ui.label("Hi cut")
                    .on_hover_text("Low-pass on the delayed signal.");
                param_combo(ui, slot, "delay-hi-cut", typed, connected, actions);
                ui.end_row();
                ui.label("Smooth")
                    .on_hover_text("Smooths pitch glitches when the delay time is changed.");
                let mut smooth = matches!(typed.get("delay-smooth"), Some(Value::Bool(true)));
                ui.add_enabled_ui(connected, |ui| {
                    if ui.checkbox(&mut smooth, "").changed() {
                        actions.push(Action::SetParam(slot, "delay-smooth", Value::Bool(smooth)));
                    }
                });
                ui.label("");
                ui.label("");
                ui.end_row();
                ui.label("Effect (wet)")
                    .on_hover_text("Level of the delayed signal in the mix.");
                param_drag(ui, slot, "delay-effect-level", typed, connected, actions);
                ui.label("Direct (dry)")
                    .on_hover_text("Level of the un-delayed signal in the mix.");
                param_drag(ui, slot, "delay-direct-level", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Speaker Sim's custom UI: enable + cabinet model and mic setting, a
    /// generic cabinet response curve (the mic setting tilts the top end), then the
    /// mic (wet) / direct (dry) level mix.
    /// The Modulation's custom UI: enable + Type, then only the selected type's
    /// controls (Flanger / Phaser / Pitch Shifter / Harmonist / Vibrato / Ring
    /// Modulator / Humanizer), grouped per the manual.
    fn show_mod_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Modulation);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Modulation enabled").changed() {
                actions.push(Action::SetParam(slot, "mod-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Type").on_hover_text(
                    "The modulation effect — only this type's settings are shown below.",
                );
                param_combo(ui, slot, "mod-type", typed, connected, actions);
            });
        });
        ui.separator();
        // mod-type: 0 Flanger, 1 Phaser, 2 Pitch Shifter, 3 Harmonist, 4 Vibrato,
        // 5 Ring Modulator, 6 Humanizer.
        match typed.get("mod-type") {
            Some(Value::Enum(1)) => mod_phaser(ui, slot, typed, connected, actions),
            Some(Value::Enum(2)) => mod_pitch_shifter(ui, slot, typed, connected, actions),
            Some(Value::Enum(3)) => mod_harmonist(ui, slot, typed, connected, actions),
            Some(Value::Enum(4)) => mod_vibrato(ui, slot, typed, connected, actions),
            Some(Value::Enum(5)) => mod_ring(ui, slot, typed, connected, actions),
            Some(Value::Enum(6)) => mod_humanizer(ui, slot, typed, connected, actions),
            _ => mod_flanger(ui, slot, typed, connected, actions),
        }
    }

    /// The Tremolo/Pan's custom UI: enable + mode, an LFO-waveform view (volume for
    /// Tremolo, anti-phase L/R for Pan; triangle or square), then Rate / Depth /
    /// Balance.
    fn show_trem_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::TremoloPan);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Tremolo/Pan enabled").changed() {
                actions.push(Action::SetParam(slot, "tremolo-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Mode").on_hover_text(
                    "Tremolo modulates volume; Pan sweeps L↔R. Tri = smooth, Sqr = on/off chop.",
                );
                param_combo(ui, slot, "tremolo-mode", typed, connected, actions);
            });
        });
        show_trem_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("LFO — Tremolo: one volume trace; Pan: L (green) / R (red).")
                .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-trem-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Rate").on_hover_text("LFO speed.");
                param_drag(ui, slot, "tremolo-rate", typed, connected, actions);
                ui.label("Depth").on_hover_text("Modulation depth.");
                param_drag(ui, slot, "tremolo-depth", typed, connected, actions);
                ui.end_row();
                ui.label("Balance")
                    .on_hover_text("Stereo balance / centre point (L100:R0 … L0:R100).");
                param_drag(ui, slot, "tremolo-balance", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Chorus's custom UI: enable + mode, an LFO-waveform view (Rate / Depth /
    /// Mod-wave shape, with an anti-phase trace in Stereo), then the rate/depth,
    /// pre-delay, tone cuts, mod-wave and effect level controls.
    fn show_chorus_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Chorus);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Chorus enabled").changed() {
                actions.push(Action::SetParam(slot, "chorus-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Mode").on_hover_text(
                    "Mono = same chorus to L+R; Stereo = different chorus on L and R.",
                );
                param_combo(ui, slot, "chorus-mode", typed, connected, actions);
            });
        });
        show_chorus_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("LFO — Rate is the speed, Depth the amount (0 = doubling).").weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-chorus-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Rate").on_hover_text("LFO speed of the chorus.");
                param_drag(ui, slot, "chorus-rate", typed, connected, actions);
                ui.label("Depth")
                    .on_hover_text("Modulation depth — set to 0 for a doubling effect.");
                param_drag(ui, slot, "chorus-depth", typed, connected, actions);
                ui.end_row();
                ui.label("Pre-delay").on_hover_text(
                    "Delay before the chorus voice (0–50 ms); larger = more doubling.",
                );
                param_drag(ui, slot, "chorus-pre-delay", typed, connected, actions);
                // Mod wave is a Triangle↔Sine blend (0..10) — a slider between the two
                // ends reads far better than a numeric "Tri10:Sin0".
                ui.label("Mod wave").on_hover_text(
                    "LFO shape — blend from Triangle to Sine. Chorus is usually Triangle.",
                );
                ui.add_enabled_ui(connected, |ui| {
                    let mut wave = match typed.get("chorus-mod-wave") {
                        Some(Value::Int(v)) => v,
                        _ => 0,
                    };
                    ui.horizontal(|ui| {
                        ui.label("Tri");
                        let slider = egui::Slider::new(&mut wave, 0..=10).show_value(false);
                        if ui
                            .add_sized([90.0, ui.spacing().interact_size.y], slider)
                            .changed()
                        {
                            actions.push(Action::SetParam(
                                slot,
                                "chorus-mod-wave",
                                Value::Int(wave),
                            ));
                        }
                        ui.label("Sin");
                    });
                });
                ui.end_row();
                ui.label("Low cut")
                    .on_hover_text("Roll off the wet signal below this frequency.");
                param_combo(ui, slot, "chorus-low-cut", typed, connected, actions);
                ui.label("Hi cut")
                    .on_hover_text("Roll off the wet signal above this frequency.");
                param_combo(ui, slot, "chorus-hi-cut", typed, connected, actions);
                ui.end_row();
                ui.label("Effect level")
                    .on_hover_text("Chorus (wet) level in the mix.");
                param_drag(ui, slot, "chorus-effect-level", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Loop's custom UI: enable + mode with a routing diagram (Series passes the
    /// whole signal through the external loop; Parallel mixes the return alongside
    /// the dry signal), then the Send / Return levels.
    fn show_loop_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Loop);
        let parallel = matches!(typed.get("loop-mode"), Some(Value::Enum(1)));
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Loop enabled").changed() {
                actions.push(Action::SetParam(slot, "loop-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Mode").on_hover_text(
                    "Series inserts the external loop in the chain; Parallel mixes its \
                     return alongside the dry signal.",
                );
                param_combo(ui, slot, "loop-mode", typed, connected, actions);
            });
        });
        ui.add_space(6.0);
        let diagram = if parallel {
            "in ─┬─────────── dry ───────────→ out\n    └→ SEND → [ external FX ] → RETURN ┘"
        } else {
            "in → SEND → [ external FX ] → RETURN → out"
        };
        ui.label(egui::RichText::new(diagram).monospace());
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(if parallel {
                "Parallel — the dry signal continues; the loop return is mixed back in."
            } else {
                "Series — the whole signal passes out through the external loop."
            })
            .weak(),
        );
        ui.add_space(6.0);
        egui::Grid::new("gx700-loop-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Send level")
                    .on_hover_text("Output level at the SEND jack.");
                param_drag(ui, slot, "loop-send-level", typed, connected, actions);
                ui.label("Return level").on_hover_text(
                    "Input level at the RETURN jack (in Parallel, the wet-mix amount).",
                );
                param_drag(ui, slot, "loop-return-level", typed, connected, actions);
                ui.end_row();
            });
    }

    /// The Wah's custom UI: enable + mode, a resonant-peak filter curve, then the
    /// mode-relevant controls — pedal sweep (Frequency / Peak / pedal source / min /
    /// max) for the pedal modes, or envelope+LFO controls for Auto Wah.
    fn show_wah_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::Wah);
        let auto = matches!(typed.get("wah-mode"), Some(Value::Enum(2)));
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Wah enabled").changed() {
                actions.push(Action::SetParam(slot, "wah-enable", Value::Bool(on)));
            }
            ui.horizontal(|ui| {
                ui.label("Mode").on_hover_text(
                    "Pedal Wah / SW-Pedal Wah follow a pedal; Auto Wah sweeps from the \
                     envelope or an LFO.",
                );
                param_combo(ui, slot, "wah-mode", typed, connected, actions);
            });
        });
        show_wah_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Resonant peak — Peak sets the width; faint = pedal sweep range.")
                .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-wah-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                if auto {
                    ui.label("Polarity")
                        .on_hover_text("Sweep direction driven by the envelope (Up or Down).");
                    param_combo(ui, slot, "wah-auto-polarity", typed, connected, actions);
                    ui.label("Sensitivity")
                        .on_hover_text("How strongly the input level drives the sweep.");
                    param_drag(ui, slot, "wah-auto-sens", typed, connected, actions);
                    ui.end_row();
                    ui.label("Manual")
                        .on_hover_text("Centre frequency the auto-wah sweeps around.");
                    param_drag(ui, slot, "wah-auto-manual", typed, connected, actions);
                    ui.label("Peak").on_hover_text(
                        "Resonance / width — 50 ≈ standard; higher is narrower and more vocal.",
                    );
                    param_drag(ui, slot, "wah-peak", typed, connected, actions);
                    ui.end_row();
                    ui.label("Rate")
                        .on_hover_text("LFO rate of the cyclic sweep.");
                    param_drag(ui, slot, "wah-auto-rate", typed, connected, actions);
                    ui.label("Depth")
                        .on_hover_text("LFO depth of the cyclic sweep.");
                    param_drag(ui, slot, "wah-auto-depth", typed, connected, actions);
                    ui.end_row();
                } else {
                    ui.label("Frequency")
                        .on_hover_text("Centre frequency / pedal position of the wah.");
                    param_drag(ui, slot, "wah-pedal-freq", typed, connected, actions);
                    ui.label("Peak").on_hover_text(
                        "Resonance / width — 50 ≈ standard; higher is narrower and more vocal.",
                    );
                    param_drag(ui, slot, "wah-peak", typed, connected, actions);
                    ui.end_row();
                    ui.label("Pedal").on_hover_text(
                        "Pedal source: 0 = Fixed, 1 = Exp pedal, 2 = FC-200, 3+ = MIDI CC.",
                    );
                    param_drag(ui, slot, "wah-pedal-source", typed, connected, actions);
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                    ui.label("Pedal min")
                        .on_hover_text("Frequency at the pedal's heel (sweep low end).");
                    param_drag(ui, slot, "wah-pedal-min", typed, connected, actions);
                    ui.label("Pedal max")
                        .on_hover_text("Frequency at the pedal's toe (sweep high end).");
                    param_drag(ui, slot, "wah-pedal-max", typed, connected, actions);
                    ui.end_row();
                }
                ui.label("Level").on_hover_text("Wah output level.");
                param_drag(ui, slot, "wah-level", typed, connected, actions);
                ui.end_row();
            });
    }

    fn show_speaker_editor(
        &self,
        ui: &mut egui::Ui,
        slot: u16,
        typed: &TypedPatch,
        actions: &mut Vec<Action>,
    ) {
        let connected = self.edit_enabled();
        let enabled = block_enabled(typed, Block::SpeakerSim);
        ui.add_enabled_ui(connected, |ui| {
            let mut on = enabled;
            if ui.checkbox(&mut on, "Speaker Sim enabled").changed() {
                actions.push(Action::SetParam(slot, "speaker-enable", Value::Bool(on)));
            }
            egui::Grid::new("gx700-speaker-head")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Cabinet")
                        .on_hover_text("Speaker / cabinet model.");
                    param_combo(ui, slot, "speaker-type", typed, connected, actions);
                    ui.end_row();
                    ui.label("Mic setting").on_hover_text(
                        "Mic placement: 1 = centre of the speaker cone (brightest); \
                         higher moves the mic progressively further away (mellower).",
                    );
                    param_drag(ui, slot, "speaker-mic-setting", typed, connected, actions);
                    ui.end_row();
                });
            // Describe the selected cabinet (enclosure, speakers, mic, best pairing).
            let cab = match typed.get("speaker-type") {
                Some(Value::Enum(v)) => v,
                _ => 0,
            };
            ui.label(egui::RichText::new(speaker_cab_desc(cab)).weak());
        });
        show_speaker_curve(ui, typed);
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(
                "Generic cab response — the 12 models differ; Mic setting tilts the top.",
            )
            .weak(),
        );
        ui.add_space(4.0);
        egui::Grid::new("gx700-speaker-grid")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Mic (wet)")
                    .on_hover_text("Level of the mic'd, cabinet-simulated signal.");
                param_drag(ui, slot, "speaker-mic-level", typed, connected, actions);
                ui.label("Direct (dry)")
                    .on_hover_text("Level of the direct, un-simulated signal.");
                param_drag(ui, slot, "speaker-direct-level", typed, connected, actions);
                ui.end_row();
            });
    }

    fn show_patch_list(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.label(
            egui::RichText::new(
                "Click a slot to audition · drag the ↕ handle onto another slot to re-order the bank.",
            )
            .weak(),
        );
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("patches")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        for row in &self.rows {
                            let playing = self.now_playing == Some(row.slot);
                            // Column 1: per-row action buttons (left-aligned, like the
                            // library lists).
                            self.patch_row_buttons(ui, row, actions);
                            // Column 2: the slot id, click to audition. A slot whose read
                            // was skipped is marked with a warning glyph + tint.
                            let label = if row.failed {
                                egui::RichText::new(format!("⚠ U{:03}", row.slot))
                                    .color(egui::Color32::from_rgb(0xE0, 0xA0, 0x30))
                            } else {
                                egui::RichText::new(format!("U{:03}", row.slot))
                            };
                            // A separate drag handle (↕) is the re-order grip; the slot
                            // id stays a plain click-to-audition label. They occupy
                            // different rects, so the drag-sense interaction can't steal
                            // the label's click (which it does if they share a rect).
                            let cell = ui.horizontal(|ui| {
                                let drag_id = egui::Id::new(("patch-drag", row.slot));
                                ui.add_enabled_ui(self.editable(), |ui| {
                                    ui.dnd_drag_source(drag_id, row.slot, |ui| {
                                        ui.label(egui::RichText::new("↕").weak());
                                    })
                                    .response
                                    .on_hover_text("drag onto another slot to re-order the bank");
                                });
                                let r = ui.add_enabled(
                                    self.editable(),
                                    egui::SelectableLabel::new(playing, label),
                                );
                                let r = if row.failed {
                                    r.on_hover_text(
                                        "read failed — value may be stale; Refresh to retry",
                                    )
                                } else {
                                    r
                                };
                                if r.clicked() {
                                    actions.push(Action::Audition(row.slot));
                                }
                            });
                            // Re-interact the cell rect as a hover-sensing drop target, so
                            // a drop released over its SelectableLabel still registers (a
                            // bare layout response misses drops over interactive children).
                            let drop = ui.interact(
                                cell.response.rect,
                                egui::Id::new(("patch-drop", row.slot)),
                                egui::Sense::hover(),
                            );
                            if self.editable()
                                && let Some(from) = drop.dnd_release_payload::<u16>()
                            {
                                actions.push(Action::ReorderPatch(*from, row.slot));
                            }

                            // Column 3: editable patch name (egui keeps the cursor by
                            // widget id, so a per-frame clone of the buffer is fine). Use
                            // a fixed allocation: inside a Grid, TextEdit::desired_width
                            // gets clamped to the (initially tiny) available width and
                            // the column sticks at a sliver, so add_sized it instead.
                            let mut name = row.name_edit.clone();
                            let edit = egui::TextEdit::singleline(&mut name)
                                .hint_text("—")
                                .char_limit(NAME_LEN);
                            let name_size = [180.0, ui.spacing().interact_size.y];
                            let name_changed = ui
                                .add_enabled_ui(self.editable(), |ui| {
                                    ui.add_sized(name_size, edit).changed()
                                })
                                .inner;
                            if name_changed {
                                actions.push(Action::SetName(row.slot, name));
                            }

                            // Column 4: output level as a compact drag-value (a slider
                            // here grows to fill the row and bloats the list).
                            let mut level =
                                i32::from(row.pending_level.unwrap_or(row.stored_level));
                            let drag = egui::DragValue::new(&mut level).range(0..=100).suffix("%");
                            let changed = ui
                                .add_enabled_ui(self.editable(), |ui| {
                                    let r = ui.add(drag).changed();
                                    // Keep the value clear of the vertical scrollbar.
                                    ui.add_space(18.0);
                                    r
                                })
                                .inner;
                            if changed {
                                let level = u8::try_from(level.clamp(0, 100)).unwrap_or(0);
                                actions.push(Action::SetLevel(row.slot, level));
                            }
                            ui.end_row();
                        }
                    });
            });
    }

    /// A patch row's action buttons (the leftmost column): Edit, Save/Revert (enabled
    /// only when the row has an unsaved edit — their state is the "modified"
    /// indicator), and Copy/Paste/Clear (Paste also needs something on the clipboard).
    fn patch_row_buttons(&self, ui: &mut egui::Ui, row: &PatchRow, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(self.editable(), |ui| {
                if action_button(ui, icon::EDIT, ActionKind::Read)
                    .on_hover_text("edit this patch's effects (auditions it live)")
                    .clicked()
                {
                    actions.push(Action::EditDevicePatch(row.slot));
                }
            });
            ui.add_enabled_ui(self.editable() && row.dirty(), |ui| {
                let save = action_button(ui, icon::SAVE, ActionKind::Commit).on_hover_text(
                    "store this patch (name + level) to the unit (needs BULK LOAD mode)",
                );
                if save.clicked() {
                    actions.push(Action::SaveRow(row.slot));
                }
                let revert = action_button(ui, icon::REVERT, ActionKind::Caution)
                    .on_hover_text("discard edits, back to the values stored on the unit");
                if revert.clicked() {
                    actions.push(Action::RevertRow(row.slot));
                }
            });
            ui.add_enabled_ui(self.editable(), |ui| {
                if action_button(ui, icon::COPY, ActionKind::Read)
                    .on_hover_text("copy this patch to the clipboard")
                    .clicked()
                {
                    actions.push(Action::CopyRow(row.slot));
                }
            });
            ui.add_enabled_ui(self.editable() && self.clipboard.is_some(), |ui| {
                let hover = match &self.clipboard {
                    Some((from, p)) => {
                        format!("paste {} {:?} here (then Save)", slot_label(*from), p.name)
                    }
                    None => "Copy a patch first".to_owned(),
                };
                if action_button(ui, icon::PASTE, ActionKind::Neutral)
                    .on_hover_text(hover)
                    .clicked()
                {
                    actions.push(Action::PasteRow(row.slot));
                }
            });
            ui.add_enabled_ui(self.editable(), |ui| {
                if action_button(ui, icon::CLEAR, ActionKind::Destructive)
                    .on_hover_text(
                        "blank this patch to Empty (name \"Empty\", level 0, effects off); \
                         staged — Revert restores it, Save overwrites the stored patch",
                    )
                    .clicked()
                {
                    actions.push(Action::ClearRow(row.slot));
                }
            });
            // Divider between the action icons and the slot's reorder handle (next
            // grid column), matching the composer.
            ui.separator();
        });
    }

    /// The Presets tab: the factory presets (P001..P100). Clicking a preset loads
    /// it into the active sound (the temporary buffer) so it can be heard and used
    /// immediately — this works in Play mode, so no BULK LOAD switch is needed.
    fn show_preset_list(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.label(
            egui::RichText::new(
                "Click a preset to load it into the active sound (no BULK LOAD needed). \
                 Copy grabs it; Paste it onto a user slot on the Patches tab.",
            )
            .weak(),
        );
        if self.preset_loader.is_some() {
            let frac = f32::from(self.preset_progress) / f32::from(PRESET_SLOTS);
            ui.add(
                egui::ProgressBar::new(frac)
                    .desired_width(220.0)
                    .text(format!("reading {}/{PRESET_SLOTS}", self.preset_progress)),
            );
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("presets")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        for row in &self.presets {
                            let playing = self.now_playing == Some(row.slot);
                            // Action button first (left-aligned, like every other list).
                            ui.add_enabled_ui(self.editable(), |ui| {
                                if action_button(ui, icon::COPY, ActionKind::Read)
                                    .on_hover_text(
                                        "copy this preset to the clipboard, then Paste it onto a \
                                     user slot on the Patches tab",
                                    )
                                    .clicked()
                                {
                                    actions.push(Action::CopyRow(row.slot));
                                }
                            });
                            let label = if row.failed {
                                egui::RichText::new(format!("⚠ {}", slot_label(row.slot)))
                                    .color(egui::Color32::from_rgb(0xE0, 0xA0, 0x30))
                            } else {
                                egui::RichText::new(slot_label(row.slot))
                            };
                            let resp = ui.add_enabled(
                                self.editable(),
                                egui::SelectableLabel::new(playing, label),
                            );
                            if resp.clicked() {
                                actions.push(Action::Audition(row.slot));
                            }
                            let name = if row.name.is_empty() {
                                "—".to_owned()
                            } else {
                                row.name.clone()
                            };
                            ui.label(name);
                            ui.horizontal(|ui| {
                                ui.label(format!("{}%", row.stored_level));
                                // Keep the value clear of the vertical scrollbar.
                                ui.add_space(18.0);
                            });
                            ui.end_row();
                        }
                    });
            });
    }

    /// The Library tab: save the current patch / effect block to disk, and load or
    /// delete saved ones. Loading stages into the now-playing slot (Write in BULK
    /// LOAD to persist), so files combine with the on-unit editing.
    fn show_library(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.heading("Library");
        ui.label(egui::RichText::new("Patches you've saved on this computer.").weak());
        ui.separator();
        let has_slot = self.now_playing.is_some();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Patches");
                if !has_slot {
                    ui.label(
                        egui::RichText::new("Audition a patch to save it or load into its slot.")
                            .weak(),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label("Save current as:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.lib_patch_name)
                            .hint_text("name")
                            .desired_width(160.0),
                    );
                    let named = !self.lib_patch_name.trim().is_empty();
                    ui.add_enabled_ui(has_slot && named, |ui| {
                        if action_button(ui, "Insert", ActionKind::Commit)
                            .on_hover_text("save the now-playing patch as a new library patch")
                            .clicked()
                        {
                            actions.push(Action::SavePatchLib);
                        }
                    });
                    ui.add_enabled_ui(named && self.clipboard.is_some(), |ui| {
                        if action_button(ui, "Paste", ActionKind::Neutral)
                            .on_hover_text("save the clipboard patch as a new library patch")
                            .clicked()
                        {
                            actions.push(Action::PastePatchLib);
                        }
                    });
                });
                lib_list(
                    ui,
                    &config::json_stems(config::patches_dir()),
                    "No saved patches yet.",
                    has_slot,
                    "load into the now-playing slot (staged)",
                    config::patches_dir().as_deref(),
                    Action::LoadPatchLib,
                    Action::SavePatchOver,
                    Action::CopyPatchLib,
                    Some(Action::EditLibraryPatch),
                    actions,
                );

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Scenes (whole-bank snapshots) live in the Scene tab. Per-effect-block \
                     presets live in the Edit tab — open a block's “Library”.",
                    )
                    .weak(),
                );
            });
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Audition(slot) => self.audition(slot),
            Action::SetLevel(slot, level) => self.set_level(slot, level),
            Action::SetParam(slot, key, value) => self.set_param(slot, key, value),
            Action::SelectTab(tab) => self.tab = tab,
            Action::SelectBlock(block) => self.selected_block = block,
            Action::SelectAssign(idx) => self.selected_assign = idx,
            Action::CopyBlock(block) => self.copy_block(block),
            Action::PasteBlock(slot, block) => self.paste_block(slot, block),
            Action::RevertBlock(slot, block) => self.revert_block(slot, block),
            Action::ReorderChain(slot, from, to) => self.reorder_chain(slot, from, to),
            Action::ReorderPatch(from, to) => self.reorder_patches(from, to),
            Action::SetName(slot, name) => self.set_name_edit(slot, name),
            Action::SaveRow(slot) => self.save_row(slot),
            Action::RevertRow(slot) => self.revert_row(slot),
            Action::CopyRow(slot) => self.copy_row(slot),
            Action::PasteRow(slot) => self.paste_row(slot),
            Action::ClearRow(slot) => self.clear_row(slot),
            Action::SavePatchLib => self.save_patch_lib(),
            Action::LoadPatchLib(name) => self.load_patch_lib(&name),
            Action::CopyPatchLib(name) => self.copy_patch_lib(&name),
            Action::SavePatchOver(name) => self.save_patch_over(&name),
            Action::PastePatchLib => self.paste_patch_lib(),
            Action::CopyScene(name) => self.copy_scene(&name),
            Action::PasteScene => self.paste_scene(),
            Action::ComposeNew => self.compose_new(),
            Action::ComposeCapture => self.compose_capture(),
            Action::ComposeLoad(name) => self.compose_load(&name),
            Action::ComposeAssign(idx, name) => self.compose_assign(idx, &name),
            Action::ComposeClear(idx) => self.compose_clear(idx),
            Action::ComposeReorder(from, to) => self.compose_reorder(from, to),
            Action::ComposeAssignBank(idx, slot) => self.compose_assign_bank(idx, slot),
            Action::ComposeCopy(idx) => self.compose_copy(idx),
            Action::ComposePaste(idx) => self.compose_paste(idx),
            Action::ComposeRevert(idx) => self.compose_revert(idx),
            Action::ComposeSave => self.compose_save(),
            Action::ComposeSaveOver(name) => self.compose_save_over(&name),
            Action::ComposeApply => self.compose_apply(),
            Action::EditComposerSlot(idx) => self.edit_composer_slot(idx),
            Action::EditLibraryPatch(name) => self.edit_library_patch(&name),
            Action::EditDevicePatch(slot) => self.edit_device_patch(slot),
            Action::SaveOfflineEdit => self.save_offline_edit(),
            Action::CloseOfflineEdit => self.close_offline_edit(),
            Action::ToggleBlockLib => self.block_lib_open = !self.block_lib_open,
            Action::SaveBlockPreset(name) => self.save_block_preset(&name),
            Action::LoadBlockPreset(name) => self.load_block_preset(&name),
            Action::CopyBlockPreset(name) => self.copy_block_preset(&name),
            Action::PasteBlockPreset(name) => self.paste_block_preset(&name),
            Action::RequestDelete(path) => self.pending_delete = Some(path),
            Action::ConfirmDelete => self.confirm_delete(),
            Action::CancelDelete => self.pending_delete = None,
            Action::Refresh => {
                // Re-reading replaces every row's stored patch with what the unit
                // holds; staged edits would be silently lost, so warn first.
                if self.dirty_count() > 0 {
                    self.confirm_refresh = true;
                } else {
                    self.start_load();
                }
            }
            Action::ConfirmRefresh => {
                self.confirm_refresh = false;
                self.discard_edits();
                self.start_load();
            }
            Action::CancelRefresh => self.confirm_refresh = false,
            Action::Retry => self.retry(),
            Action::OpenBulkPrompt => self.bulk_prompt = true,
            Action::CloseBulkPrompt => self.bulk_prompt = false,
            Action::WriteAll => {
                self.bulk_prompt = false;
                self.start_write();
            }
            Action::ProbeBulk => self.request_probe(),
        }
    }

    fn drain_loader(&mut self) {
        let Some(loader) = &self.loader else {
            return;
        };
        let mut done = false;
        let mut aborted: Option<String> = None;
        for ev in loader.drain() {
            match ev {
                // The user bank is read deep: keep the whole patch so scenes can be
                // saved and auditions are instant, and derive the header fields.
                Loaded::Patch(slot, raw) => {
                    self.progress = self.progress.saturating_add(1);
                    let typed = TypedPatch::from_raw(&raw);
                    if let Some(row) = self.row_mut(slot) {
                        let untouched = row.name_edit == row.name;
                        row.name.clone_from(&typed.name);
                        if untouched {
                            row.name_edit.clone_from(&row.name);
                        }
                        row.stored_level = typed.output_level;
                        row.chain = typed.chain.to_vec();
                        row.full = Some(typed);
                        row.failed = false;
                    }
                }
                Loaded::Header(slot, header) => {
                    self.progress = self.progress.saturating_add(1);
                    if let Some(row) = self.row_mut(slot) {
                        // Keep the edit buffer in sync unless the user is mid-edit.
                        let untouched = row.name_edit == row.name;
                        row.name = header.name;
                        if untouched {
                            row.name_edit.clone_from(&row.name);
                        }
                        row.stored_level = header.output_level;
                        row.chain = header.chain;
                        row.failed = false;
                    }
                }
                Loaded::Failed(slot, msg) => {
                    self.progress = self.progress.saturating_add(1);
                    if let Some(row) = self.row_mut(slot) {
                        row.failed = true;
                    }
                    self.status = format!("U{slot:03}: {msg}");
                }
                Loaded::Aborted(msg) => aborted = Some(msg),
                Loaded::Done => done = true,
            }
        }
        if let Some(msg) = aborted {
            // Device-wide failure, not a handful of bad slots: drop the per-slot
            // marks (the shown values are the last good cache) and stop.
            self.loader = None;
            for row in &mut self.rows {
                row.failed = false;
            }
            self.status = msg;
        } else if done {
            self.loader = None;
            "bank loaded".clone_into(&mut self.status);
            self.save_cache();
        }
    }

    /// Start the one-shot factory-preset read the first time the Presets tab is
    /// shown (presets are static, so this runs at most once per session — and not
    /// at all if a cached preset list was loaded). Waits for the user-bank load to
    /// finish so the two reads don't contend for the device.
    fn maybe_load_presets(&mut self) {
        if self.tab == Tab::Presets
            && self.connected
            && !self.presets_loaded
            && self.preset_loader.is_none()
            && self.loader.is_none()
        {
            self.preset_progress = 0;
            // Shallow (headers only) — presets are read-only; full content loads on
            // demand when one is auditioned.
            self.preset_loader = Some(Loader::spawn_range(
                Arc::clone(&self.device),
                PRESET_START,
                PRESET_END,
                false,
            ));
            "reading factory presets…".clone_into(&mut self.status);
        }
    }

    /// Drain the background preset read, filling the preset rows as headers arrive.
    fn drain_preset_loader(&mut self) {
        let Some(loader) = &self.preset_loader else {
            return;
        };
        let mut done = false;
        let mut aborted: Option<String> = None;
        for ev in loader.drain() {
            match ev {
                Loaded::Header(slot, header) => {
                    self.preset_progress = self.preset_progress.saturating_add(1);
                    if let Some(row) = self.row_mut(slot) {
                        row.name.clone_from(&header.name);
                        row.name_edit = header.name;
                        row.stored_level = header.output_level;
                        row.chain = header.chain;
                        row.failed = false;
                    }
                }
                Loaded::Failed(slot, msg) => {
                    self.preset_progress = self.preset_progress.saturating_add(1);
                    if let Some(row) = self.row_mut(slot) {
                        row.failed = true;
                    }
                    self.status = format!("{}: {msg}", slot_label(slot));
                }
                Loaded::Patch(..) => {} // presets are read shallow
                Loaded::Aborted(msg) => aborted = Some(msg),
                Loaded::Done => done = true,
            }
        }
        if let Some(msg) = aborted {
            self.preset_loader = None;
            self.status = msg;
        } else if done {
            self.preset_loader = None;
            self.presets_loaded = true;
            "factory presets loaded".clone_into(&mut self.status);
            self.save_cache();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_background(ctx);
        // Capture view state for persistence on exit. Use `screen_rect` (always
        // available) rather than the viewport's `inner_rect`, which is `None` on
        // some platforms (e.g. Wayland) and left the window size unsaved. screen_rect
        // is in egui points (scaled by zoom); a window inner size is logical points.
        self.zoom = ctx.zoom_factor();
        let size = ctx.screen_rect().size() * self.zoom;
        self.window = Some([size.x, size.y]);

        let mut actions: Vec<Action> = Vec::new();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("GX-700");
                ui.separator();
                if ui
                    .selectable_label(self.tab == Tab::Patches, "Device Patches")
                    .clicked()
                {
                    actions.push(Action::SelectTab(Tab::Patches));
                }
                if ui
                    .selectable_label(self.tab == Tab::Presets, "Presets")
                    .clicked()
                {
                    actions.push(Action::SelectTab(Tab::Presets));
                }
                if ui
                    .selectable_label(self.tab == Tab::Library, "Library")
                    .clicked()
                {
                    actions.push(Action::SelectTab(Tab::Library));
                }
                if ui
                    .selectable_label(self.tab == Tab::Scene, "Scene")
                    .clicked()
                {
                    actions.push(Action::SelectTab(Tab::Scene));
                }
                ui.separator();
                if self.connected {
                    if let Some(writer) = &self.writer {
                        // A batch write (scene / Write changes) is running.
                        let total = writer.total().max(1);
                        let progress = self.write_progress.min(writer.total());
                        let frac = f32::from(u16::try_from(progress).unwrap_or(0))
                            / f32::from(u16::try_from(total).unwrap_or(1));
                        ui.add(
                            egui::ProgressBar::new(frac)
                                .desired_width(220.0)
                                .text(format!("writing {progress}/{}", writer.total())),
                        );
                    } else {
                        if self.loader.is_some() {
                            let frac = f32::from(self.progress) / f32::from(USER_SLOTS);
                            ui.add(
                                egui::ProgressBar::new(frac)
                                    .desired_width(160.0)
                                    .text(format!("reading {}/{USER_SLOTS}", self.progress)),
                            );
                        } else if action_button(ui, "Refresh", ActionKind::Read).clicked() {
                            actions.push(Action::Refresh);
                        }
                        let pending = self.dirty_count();
                        ui.add_enabled_ui(self.editable() && pending > 0, |ui| {
                            if action_button(
                                ui,
                                format!("Write changes ({pending})"),
                                ActionKind::Commit,
                            )
                            .on_hover_text("store all pending changes to the unit (BULK LOAD)")
                            .clicked()
                            {
                                actions.push(Action::OpenBulkPrompt);
                            }
                        });
                    }
                } else if self.offline {
                    ui.label(egui::RichText::new("offline").weak());
                    if action_button(ui, "Connect", ActionKind::Read)
                        .on_hover_text("connect to the unit to go online")
                        .clicked()
                    {
                        actions.push(Action::Retry);
                    }
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "not connected");
                    if action_button(ui, "Retry", ActionKind::Read).clicked() {
                        actions.push(Action::Retry);
                    }
                }
                // Exit in the far-right corner (config is saved on close via Drop).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if action_button(ui, "Exit", ActionKind::Neutral)
                        .on_hover_text("close the editor (pending changes are kept until written)")
                        .clicked()
                    {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });

        self.show_central(ctx, &mut actions);

        self.show_bulk_modals(ctx, &mut actions);
        self.show_delete_modal(ctx, &mut actions);
        self.show_refresh_modal(ctx, &mut actions);

        for action in actions {
            self.apply(action);
        }
    }
}

impl App {
    /// The BULK-LOAD modals: the blocking "enter BULK LOAD" gate shown until the
    /// unit is in the mode (the session stays in it, so no switching), and the
    /// The central panel (and the Edit tab's side panel) for the active tab.
    /// The Scene tab's left panel: the patch library as drag sources. Drag a name
    /// onto a composer slot to place that patch there.
    fn show_scene_palette(&self, ui: &mut egui::Ui) {
        ui.heading("Patch library");
        ui.label(egui::RichText::new("Drag a patch onto a slot to place it.").weak());
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // The live device bank: drag a current unit patch straight into a slot.
                ui.label(egui::RichText::new("Device bank").strong());
                let mut any_bank = false;
                for row in &self.rows {
                    // Only loaded rows have content to place.
                    if row.full.is_none() {
                        continue;
                    }
                    any_bank = true;
                    let name = if row.name.trim().is_empty() {
                        "(empty)"
                    } else {
                        row.name.as_str()
                    };
                    let text = format!("{}  {name}", slot_label(row.slot));
                    let id = egui::Id::new(("scene-bank", row.slot));
                    ui.dnd_drag_source(id, SceneDrag::Bank(row.slot), |ui| {
                        ui.add(egui::Label::new(text).truncate().sense(egui::Sense::drag()));
                    });
                }
                if !any_bank {
                    ui.label(egui::RichText::new("bank not read yet").weak());
                }

                ui.add_space(6.0);
                // Saved-to-disk patch files (the Library tab's patches).
                ui.label(egui::RichText::new("Saved patches").strong());
                let names = config::json_stems(config::patches_dir());
                if names.is_empty() {
                    ui.label(
                        egui::RichText::new("none saved — use the Library tab to save some").weak(),
                    );
                }
                for name in &names {
                    let id = egui::Id::new(("scene-palette", name));
                    ui.dnd_drag_source(id, SceneDrag::Lib(name.clone()), |ui| {
                        ui.add(egui::Label::new(name).truncate().sense(egui::Sense::drag()));
                    });
                }
            });
    }

    /// The Scene tab's main panel: scene controls and the 100-slot grid. Each slot is
    /// a drop target for a library patch (assign) or another slot (re-order).
    fn show_scene(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.heading("Scene composer");
        ui.label(
            egui::RichText::new(
                "Build a whole-bank scene offline from the patch library, then Apply it \
                 to the unit (in BULK LOAD) or Save it to the scene library.",
            )
            .weak(),
        );
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut self.compose_name)
                    .hint_text("scene name")
                    .desired_width(160.0),
            );
            if action_button(ui, "New", ActionKind::Caution)
                .on_hover_text("reset every slot to INIT")
                .clicked()
            {
                actions.push(Action::ComposeNew);
            }
            // Capture reads the already-loaded rows, not the device, so gate on the
            // bank being loaded (works from cache offline) rather than on connection.
            ui.add_enabled_ui(self.bank_loaded(), |ui| {
                if action_button(ui, "Capture bank", ActionKind::Read)
                    .on_hover_text("copy the current bank into the composer")
                    .clicked()
                {
                    actions.push(Action::ComposeCapture);
                }
            });
            let named = !self.compose_name.trim().is_empty();
            ui.add_enabled_ui(named, |ui| {
                if action_button(ui, "Save as new", ActionKind::Commit)
                    .on_hover_text("save the composer as a new scene file")
                    .clicked()
                {
                    actions.push(Action::ComposeSave);
                }
            });
            ui.add_enabled_ui(named && self.scene_clip.is_some(), |ui| {
                if action_button(ui, "Paste", ActionKind::Neutral)
                    .on_hover_text("save the copied scene under this name (duplicate)")
                    .clicked()
                {
                    actions.push(Action::PasteScene);
                }
            });
            ui.add_enabled_ui(self.editable(), |ui| {
                if action_button(ui, "Apply to unit", ActionKind::Commit)
                    .on_hover_text("write the whole scene to the unit (replaces the bank)")
                    .clicked()
                {
                    actions.push(Action::ComposeApply);
                }
            });
        });
        ui.separator();
        ui.label(egui::RichText::new("Saved scenes").strong());
        egui::ScrollArea::vertical()
            .id_salt("scenes-lib")
            .max_height(130.0)
            // Fill width (scrollbar at the box's right edge) but shrink to content
            // height so the box isn't padded to max_height when there are few scenes.
            .auto_shrink([false, true])
            .show(ui, |ui| {
                lib_list(
                    ui,
                    &config::json_stems(config::scenes_dir()),
                    "No saved scenes yet.",
                    true,
                    "load this scene into the composer",
                    config::scenes_dir().as_deref(),
                    Action::ComposeLoad,
                    Action::ComposeSaveOver,
                    Action::CopyScene,
                    None,
                    actions,
                );
            });
        ui.separator();
        ui.label(
            egui::RichText::new(
                "Drag a patch from the left onto a slot, drag the ↕ handle to re-order, \
                 or use Copy / Paste.",
            )
            .weak(),
        );
        self.show_scene_rows(ui, actions);
    }

    /// The composer's 100-slot grid: each slot a drop zone (assign / re-order) with
    /// Edit / Revert / Copy / Paste / Clear.
    fn show_scene_rows(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let can_paste = self.clipboard.is_some();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (idx, patch) in self.compose.iter().enumerate() {
                    let slot = idx + 1;
                    let label = if patch.name.trim().is_empty() {
                        "(INIT)".to_owned()
                    } else {
                        patch.name.clone()
                    };
                    // Whether this slot differs from its baseline (enables Revert).
                    let changed = self.compose_base.get(idx) != Some(patch);
                    let inner = ui.horizontal(|ui| {
                        // Canonical action order (shared by every list): Edit, Revert,
                        // Copy, Paste, Clear.
                        if action_button(ui, icon::EDIT, ActionKind::Read)
                            .on_hover_text("edit this slot's patch offline (no device)")
                            .clicked()
                        {
                            actions.push(Action::EditComposerSlot(idx));
                        }
                        ui.add_enabled_ui(changed, |ui| {
                            if action_button(ui, icon::REVERT, ActionKind::Caution)
                                .on_hover_text("restore this slot to its last saved/loaded state")
                                .clicked()
                            {
                                actions.push(Action::ComposeRevert(idx));
                            }
                        });
                        if action_button(ui, icon::COPY, ActionKind::Read)
                            .on_hover_text("copy this slot's patch")
                            .clicked()
                        {
                            actions.push(Action::ComposeCopy(idx));
                        }
                        ui.add_enabled_ui(can_paste, |ui| {
                            if action_button(ui, icon::PASTE, ActionKind::Neutral)
                                .on_hover_text("paste the copied patch into this slot")
                                .clicked()
                            {
                                actions.push(Action::ComposePaste(idx));
                            }
                        });
                        if action_button(ui, icon::CLEAR, ActionKind::Destructive)
                            .on_hover_text("reset this slot to INIT")
                            .clicked()
                        {
                            actions.push(Action::ComposeClear(idx));
                        }
                        // Divider between the action icons and the reorder handle.
                        ui.separator();
                        let drag_id = egui::Id::new(("scene-slot", slot));
                        ui.dnd_drag_source(drag_id, SceneDrag::Slot(idx), |ui| {
                            ui.label(egui::RichText::new("↕").weak());
                        })
                        .response
                        .on_hover_text("drag onto another slot to re-order");
                        ui.label(egui::RichText::new(format!("U{slot:03}")).monospace());
                        // Fixed-width but left-aligned (add_sized would centre it).
                        ui.allocate_ui_with_layout(
                            egui::vec2(220.0, 18.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(egui::Label::new(label).truncate());
                            },
                        );
                    });
                    // Re-interact the whole row rect as a hover-sensing drop target, so a
                    // released drag registers anywhere on the row (including over its
                    // buttons) — the layout response alone misses drops there.
                    let drop = ui.interact(
                        inner.response.rect,
                        egui::Id::new(("scene-drop", idx)),
                        egui::Sense::hover(),
                    );
                    if let Some(p) = drop.dnd_release_payload::<SceneDrag>() {
                        match &*p {
                            SceneDrag::Lib(name) => {
                                actions.push(Action::ComposeAssign(idx, name.clone()));
                            }
                            SceneDrag::Bank(slot) => {
                                actions.push(Action::ComposeAssignBank(idx, *slot));
                            }
                            SceneDrag::Slot(from) => {
                                actions.push(Action::ComposeReorder(*from, idx));
                            }
                        }
                    }
                }
            });
    }

    fn show_central(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        match self.tab {
            Tab::Patches => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_patch_list(ui, actions);
                });
            }
            Tab::Edit => {
                egui::SidePanel::left("blocks")
                    .resizable(true)
                    .default_width(180.0)
                    .show(ctx, |ui| {
                        self.show_block_list(ui, actions);
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_block_params(ui, actions);
                });
            }
            Tab::Presets => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_preset_list(ui, actions);
                });
            }
            Tab::Library => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_library(ui, actions);
                });
            }
            Tab::Scene => {
                egui::SidePanel::left("scene-library")
                    .resizable(true)
                    .default_width(200.0)
                    .show(ctx, |ui| {
                        self.show_scene_palette(ui);
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_scene(ui, actions);
                });
            }
        }
    }

    /// The library delete-confirmation modal, shown whenever a delete is pending
    /// (from any tab — the global Library or a per-block library).
    fn show_delete_modal(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(path) = self.pending_delete.as_ref() else {
            return;
        };
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        egui::Window::new("Delete from library")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Delete \u{201c}{name}\u{201d} from the library? This can't be undone."
                ));
                ui.horizontal(|ui| {
                    if action_button(ui, "Delete", ActionKind::Destructive).clicked() {
                        actions.push(Action::ConfirmDelete);
                    }
                    if action_button(ui, "Cancel", ActionKind::Neutral).clicked() {
                        actions.push(Action::CancelDelete);
                    }
                });
            });
    }

    /// Warn before a Refresh that would discard staged edits (re-reading the bank
    /// replaces every row's stored patch with what the unit holds).
    fn show_refresh_modal(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if !self.confirm_refresh {
            return;
        }
        let n = self.dirty_count();
        egui::Window::new("Discard unsaved changes?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Refreshing re-reads the bank from the unit and will discard {n} \
                     unsaved change{}.",
                    if n == 1 { "" } else { "s" }
                ));
                ui.horizontal(|ui| {
                    if action_button(ui, "Discard & refresh", ActionKind::Destructive).clicked() {
                        actions.push(Action::ConfirmRefresh);
                    }
                    if action_button(ui, "Cancel", ActionKind::Neutral).clicked() {
                        actions.push(Action::CancelRefresh);
                    }
                });
            });
    }

    /// pre-write confirm for the batch store.
    fn show_bulk_modals(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.bulk_ok == Some(false) {
            egui::Window::new("Put the GX-700 in BULK LOAD mode")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(
                        "This app reads and stores patches over MIDI, which the GX-700 \
                         only allows in BULK LOAD mode.",
                    );
                    ui.label(
                        "On the unit: press TUNER/UTILITY, select \"MIDI BULK LOAD\" \
                         (the display shows \"Waiting…\"). Stay in this mode for the \
                         whole session — auditioning and writing both work here, so \
                         you never have to switch back to Play.",
                    );
                    ui.separator();
                    ui.horizontal(|ui| {
                        if action_button(ui, "Check now", ActionKind::Read).clicked() {
                            actions.push(Action::ProbeBulk);
                        }
                        ui.label(egui::RichText::new("Checking automatically…").weak());
                    });
                });
        }

        if self.bulk_prompt {
            egui::Window::new("Write changes to the unit")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Store {} patch change(s) to the unit's memory? \
                         The GX-700 is in BULK LOAD mode, so this writes immediately.",
                        self.dirty_count()
                    ));
                    ui.horizontal(|ui| {
                        if action_button(ui, "Write", ActionKind::Commit).clicked() {
                            actions.push(Action::WriteAll);
                        }
                        if action_button(ui, "Cancel", ActionKind::Neutral).clicked() {
                            actions.push(Action::CloseBulkPrompt);
                        }
                    });
                });
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        config::save(&GuiConfig {
            zoom: self.zoom,
            window: self.window,
            tab: self.tab.as_key().map(str::to_owned),
            port: self.port.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use rackctl_gx700::typed::BlockData;

    /// Wrap a payload JSON string in a library envelope for `device`.
    fn envelope_for(device: &str, payload: &str) -> String {
        format!(r#"{{"version":1,"device":"{device}","payload":{payload}}}"#)
    }

    /// Wrap a payload in a well-formed envelope for this device.
    fn envelope(payload: &str) -> String {
        envelope_for("gx700", payload)
    }

    #[test]
    fn parse_patch_reads_bare_and_enveloped_typed() {
        let patch = TypedPatch::default();
        let bare = serde_json::to_string(&patch).unwrap();
        assert_eq!(parse_patch_text(&bare).unwrap(), patch);
        assert_eq!(parse_patch_text(&envelope(&bare)).unwrap(), patch);
    }

    #[test]
    fn parse_patch_falls_back_to_a_raw_patch() {
        // The CLI's / older form is a bare RawPatch, converted to typed on load.
        let raw = TypedPatch::default().to_raw();
        let text = serde_json::to_string(&raw).unwrap();
        assert!(parse_patch_text(&text).is_ok());
    }

    #[test]
    fn parse_patch_rejects_a_foreign_device() {
        let bare = serde_json::to_string(&TypedPatch::default()).unwrap();
        assert!(parse_patch_text(&envelope_for("us16x08", &bare)).is_err());
    }

    #[test]
    fn parse_patch_rejects_garbage() {
        assert!(parse_patch_text("not a patch file").is_err());
    }

    #[test]
    fn parse_block_reads_bare_and_enveloped() {
        let block = BlockData::from_patch(&TypedPatch::default(), Block::Reverb).unwrap();
        let bare = serde_json::to_string(&block).unwrap();
        assert_eq!(parse_block_text(&bare).unwrap(), block);
        assert_eq!(parse_block_text(&envelope(&bare)).unwrap(), block);
    }

    #[test]
    fn parse_block_rejects_a_foreign_device() {
        let block = BlockData::from_patch(&TypedPatch::default(), Block::Reverb).unwrap();
        let bare = serde_json::to_string(&block).unwrap();
        assert!(parse_block_text(&envelope_for("us16x08", &bare)).is_err());
    }

    #[test]
    fn parse_block_rejects_garbage() {
        assert!(parse_block_text("not a block file").is_err());
    }

    #[test]
    fn parse_scene_reads_bare_and_enveloped() {
        let bank = vec![TypedPatch::default(), TypedPatch::default()];
        let bare = serde_json::to_string(&bank).unwrap();
        assert_eq!(parse_scene_text(&bare).unwrap().len(), 2);
        assert_eq!(parse_scene_text(&envelope(&bare)).unwrap().len(), 2);
    }

    #[test]
    fn parse_scene_rejects_garbage() {
        assert!(parse_scene_text("not a scene file").is_err());
    }

    #[test]
    fn move_within_drags_an_item_down() {
        let mut v = vec![0, 1, 2, 3, 4];
        move_within(&mut v, 1, 3);
        assert_eq!(v, vec![0, 2, 3, 1, 4]);
    }

    #[test]
    fn move_within_drags_an_item_up() {
        let mut v = vec![0, 1, 2, 3, 4];
        move_within(&mut v, 3, 1);
        assert_eq!(v, vec![0, 3, 1, 2, 4]);
    }

    #[test]
    fn move_within_ignores_noop_and_out_of_range() {
        let mut v = vec![0, 1, 2];
        move_within(&mut v, 1, 1);
        assert_eq!(v, vec![0, 1, 2]);
        move_within(&mut v, 0, 9);
        assert_eq!(v, vec![0, 1, 2]);
        move_within(&mut v, 9, 0);
        assert_eq!(v, vec![0, 1, 2]);
    }

    fn test_app() -> App {
        App::new(
            crate::device::placeholder(),
            false,
            Box::new(|| crate::device::open(true, None)),
            false,
            None,
        )
    }

    #[test]
    fn offline_edit_of_a_composer_slot_saves_back() {
        let mut app = test_app();
        app.edit_composer_slot(2);
        assert_eq!(app.edit_slot, Some(SCRATCH));

        // A pure offline param edit changes only the scratch, not the composer.
        app.set_param(SCRATCH, "comp-level", Value::Int(77));
        assert_eq!(app.edit_scratch.get("comp-level"), Some(Value::Int(77)));
        assert_ne!(
            app.compose.get(2).and_then(|p| p.get("comp-level")),
            Some(Value::Int(77))
        );
        assert!(app.edit_scratch != app.edit_base, "edit should be dirty");

        // Save writes the scratch back to the slot and re-syncs the baseline.
        app.save_offline_edit();
        assert_eq!(
            app.compose.get(2).and_then(|p| p.get("comp-level")),
            Some(Value::Int(77))
        );
        assert_eq!(app.edit_base, app.edit_scratch, "baseline should re-sync");

        app.close_offline_edit();
        assert_eq!(app.edit_slot, None);
    }

    #[test]
    fn offline_param_edit_does_not_touch_the_bank_rows() {
        let mut app = test_app();
        app.edit_composer_slot(0);
        app.set_param(SCRATCH, "comp-level", Value::Int(13));
        // The scratch is separate from the (empty) bank rows.
        assert!(app.rows.iter().all(|r| r.pending_patch.is_none()));
    }

    #[test]
    fn edit_device_patch_targets_the_slot_and_opens_the_editor() {
        let dev = crate::device::open(true, None).expect("mock device");
        let mut app = App::new(
            dev,
            true,
            Box::new(|| crate::device::open(true, None)),
            false,
            None,
        );
        app.edit_device_patch(1);
        assert_eq!(app.edit_slot, Some(1));
        assert!(app.tab == Tab::Edit);
    }

    #[test]
    fn tab_keys_round_trip_and_edit_is_transient() {
        for t in [Tab::Patches, Tab::Presets, Tab::Library, Tab::Scene] {
            let key = t.as_key().expect("main tab has a key");
            assert!(Tab::from_key(key) == Some(t));
        }
        // Edit isn't a persistable destination.
        assert!(Tab::Edit.as_key().is_none());
        assert!(Tab::from_key("edit").is_none());
        assert!(Tab::from_key("bogus").is_none());
    }

    #[test]
    fn offline_mode_starts_disconnected_on_the_scene_tab() {
        let app = App::new(
            crate::device::placeholder(),
            false,
            Box::new(|| crate::device::open(true, None)),
            true,
            None,
        );
        assert!(app.offline);
        assert!(!app.connected);
        assert!(app.tab == Tab::Scene);
        assert!(!app.editable());
    }

    #[test]
    fn compose_revert_restores_a_slot_to_its_baseline() {
        let mut app = test_app();
        // Baseline starts as all-INIT; dirty a slot.
        app.compose.get_mut(3).expect("slot").name = "EDITED".to_owned();
        assert_ne!(app.compose.get(3), app.compose_base.get(3));
        app.compose_revert(3);
        assert_eq!(app.compose.get(3), app.compose_base.get(3));
    }

    #[test]
    fn compose_assign_bank_copies_the_loaded_patch_into_the_slot() {
        let dev = crate::device::open(true, None).expect("mock device");
        let mut app = App::new(
            dev,
            true,
            Box::new(|| crate::device::open(true, None)),
            false,
            None,
        );
        app.ensure_loaded(1); // deep-read bank slot 1 from the mock
        let bank = app.effective_patch(1).expect("loaded");
        app.compose_assign_bank(4, 1);
        assert_eq!(app.compose.get(4), Some(&bank));
    }
}
