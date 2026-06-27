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
use egui_plot::{Line, LineStyle, Plot, PlotPoints};
use rackctl_gx700::param::{EQ_MID_FREQ_VALUES, EQ_MID_Q_VALUES};
use rackctl_gx700::typed::Patch as TypedPatch;
use rackctl_gx700::{Block, Kind, NAME_LEN, Param, RawPatch, Value, param, units};
use rackctl_ui::comp::output_db as comp_output_db;
use rackctl_ui::eq::{BandType, EqBand, eq_response_db};
use rackctl_ui::{ActionKind, action_button};

use crate::config::{self, CachedRow, GuiConfig};
use crate::device::{self, Device, SharedDevice};
use crate::loader::{Loaded, Loader, PRESET_END, PRESET_SLOTS, PRESET_START, USER_SLOTS};

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
    /// The full patch, loaded the first time the row is auditioned/edited.
    full: Option<RawPatch>,
    /// A live-edited level not yet written to memory.
    pending_level: Option<u8>,
    /// A staged whole-patch replacement (from Paste or Clear) not yet written.
    /// When set, it — not `full` — is the basis for the next store.
    pending_patch: Option<RawPatch>,
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

/// A UI interaction to apply after the render pass (avoids borrowing `self`
/// mutably while iterating the rows).
enum Action {
    Audition(u16),
    SetLevel(u16, u8),
    SetParam(u16, &'static str, Value),
    SelectTab(Tab),
    SelectBlock(Block),
    ReorderChain(u16, usize, usize),
    ReorderPatch(u16, u16),
    SetName(u16, String),
    SaveRow(u16),
    RevertRow(u16),
    CopyRow(u16),
    PasteRow(u16),
    ClearRow(u16),
    Refresh,
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
            Some(key) => param_combo(ui, slot, key, typed, enabled, actions),
            None => {
                ui.weak("—");
            }
        }
    }
    ui.end_row();
}

/// An int parameter as a compact drag-value in display units (US-16x08 style).
fn param_drag(
    ui: &mut egui::Ui,
    slot: u16,
    key: &'static str,
    typed: &TypedPatch,
    enabled: bool,
    actions: &mut Vec<Action>,
) {
    let Some(p) = Param::from_key(key) else {
        return;
    };
    let Kind::Int { min, max, .. } = p.kind() else {
        return;
    };
    let mut val = match typed.get(key) {
        Some(Value::Int(v)) => v,
        _ => 0,
    };
    ui.add_enabled_ui(enabled, |ui| {
        let widget = egui::DragValue::new(&mut val)
            .range(min..=max)
            .custom_formatter(move |n, _| display_raw(p, n));
        if ui.add(widget).changed() {
            actions.push(Action::SetParam(slot, key, Value::Int(val)));
        }
    });
}

/// An enum parameter as a dropdown of its labels.
fn param_combo(
    ui: &mut egui::Ui,
    slot: u16,
    key: &'static str,
    typed: &TypedPatch,
    enabled: bool,
    actions: &mut Vec<Action>,
) {
    let Some(p) = Param::from_key(key) else {
        return;
    };
    let Kind::Enum { values, .. } = p.kind() else {
        return;
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
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt((slot, key))
            .selected_text(cur)
            .show_ui(ui, |ui| {
                for (i, lbl) in values.iter().enumerate() {
                    let this = i32::try_from(i).unwrap_or(-1);
                    if ui.selectable_label(this == idx, *lbl).clicked() {
                        actions.push(Action::SetParam(slot, key, Value::Enum(this)));
                    }
                }
            });
    });
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
    clipboard: Option<(u16, RawPatch)>,
    /// Which screen is showing.
    tab: Tab,
    /// The effect block selected in the Edit tab.
    selected_block: Block,
    bulk_prompt: bool,
    /// Whether the unit is in BULK LOAD mode: `None` until first probed, then
    /// `Some(true/false)`. While `Some(false)` a blocking dialog asks the user to
    /// enter BULK LOAD; the whole session then stays in it, so no mode switching.
    bulk_ok: Option<bool>,
    /// `egui` input-time of the last BULK LOAD probe, to throttle re-probes.
    last_probe: f64,
    status: String,
    zoom: f32,
    window: Option<[f32; 2]>,
}

impl App {
    pub(crate) fn new(device: Device, connected: bool, reopen: Reopen) -> Self {
        let cfg = config::load();
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
            tab: Tab::Patches,
            selected_block: Block::Compressor,
            bulk_prompt: false,
            bulk_ok: None,
            last_probe: 0.0,
            status: if connected {
                "checking BULK LOAD mode…".to_owned()
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
        self.connected && self.loader.is_none() && self.bulk_ok != Some(false)
    }

    /// Probe the unit's BULK LOAD mode and react: start the bank read once it's in
    /// BULK LOAD (or if the probe can't run, e.g. on the mock), otherwise mark it
    /// not-yet-ready so the blocking dialog stays up. Restores the probed slot.
    fn probe_now(&mut self) {
        if !self.connected {
            return;
        }
        let result = device::lock(&self.device).probe_bulk_load(PROBE_SLOT);
        // `Ok(false)` is the only "still waiting" case. `Ok(true)` is in BULK LOAD;
        // `Err` means the probe can't run (unit silent, or no patch to read — e.g.
        // the mock), so don't block on it — let the bank read surface real trouble.
        if matches!(result, Ok(false)) {
            self.bulk_ok = Some(false);
            "waiting for BULK LOAD mode on the unit…".clone_into(&mut self.status);
        } else {
            let first = self.bulk_ok != Some(true);
            self.bulk_ok = Some(true);
            if first {
                self.start_load();
            }
        }
    }

    /// Drive the startup BULK LOAD check: probe immediately the first time, then
    /// re-probe on an interval while still waiting (so entering the mode on the
    /// unit is picked up automatically). Quiet once the unit is confirmed.
    fn drive_startup(&mut self, ctx: &egui::Context) {
        if !self.connected || self.bulk_ok == Some(true) {
            return;
        }
        let now = ctx.input(|i| i.time);
        if self.bulk_ok.is_some() && now - self.last_probe < PROBE_INTERVAL {
            return;
        }
        self.last_probe = now;
        self.probe_now();
    }

    /// Load a row's full patch if it isn't loaded yet (needed before storing,
    /// e.g. a name-only edit on a patch that was never auditioned).
    fn ensure_loaded(&mut self, slot: u16) {
        if self.row(slot).is_some_and(|r| r.full.is_none()) {
            let read = device::lock(&self.device).read_patch(slot);
            if let Ok(patch) = read
                && let Some(row) = self.row_mut(slot)
            {
                row.full = Some(patch);
            }
        }
    }

    /// The patch a store would write for `slot`: the staged whole-patch (from
    /// Paste or Clear) or the loaded patch, with the row's edited name and level
    /// overlaid. `None` if nothing is loaded for the row yet.
    fn effective_patch(&self, slot: u16) -> Option<RawPatch> {
        let row = self.row(slot)?;
        let mut patch = row.pending_patch.clone().or_else(|| row.full.clone())?;
        let level = row.pending_level.unwrap_or(row.stored_level);
        let _ = patch.set_output_level(level);
        let _ = patch.set_name(&row.name_edit);
        Some(patch)
    }

    /// After a successful store, commit the edits: the written patch becomes the
    /// row's stored state, clearing every pending change (and the dirty flag).
    fn commit_row(&mut self, slot: u16) {
        let Some(patch) = self.effective_patch(slot) else {
            return;
        };
        if let Some(row) = self.row_mut(slot) {
            row.stored_level = patch.output_level();
            row.name.clone_from(&patch.name);
            row.name_edit.clone_from(&patch.name);
            row.chain = patch.chain();
            row.full = Some(patch);
            row.pending_level = None;
            row.pending_patch = None;
        }
    }

    /// Spawn (or restart) the background bank read.
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
        self.ensure_loaded(slot);
        let Some(patch) = self.effective_patch(slot) else {
            return;
        };
        let written = device::lock(&self.device).write_current_patch(&patch);
        match written {
            Ok(_) => {
                self.now_playing = Some(slot);
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
        if self.now_playing == Some(slot)
            && let Err(e) = device::lock(&self.device).set(param, value)
        {
            self.status = format!("set {key}: {e}");
            return;
        }
        // Stage onto the row's raw base (no name/level overlay — those stay separate).
        let base = self
            .row(slot)
            .and_then(|r| r.pending_patch.clone().or_else(|| r.full.clone()));
        if let Some(base) = base {
            let mut typed = TypedPatch::from_raw(&base);
            if typed.set(key, value).is_ok()
                && let Some(row) = self.row_mut(slot)
            {
                row.pending_patch = Some(typed.to_raw());
            }
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
        let base = self
            .row(slot)
            .and_then(|r| r.pending_patch.clone().or_else(|| r.full.clone()));
        let Some(mut base) = base else {
            return;
        };
        let mut chain = base.chain();
        if from >= chain.len() || to >= chain.len() {
            return;
        }
        let id = chain.remove(from);
        chain.insert(to, id);
        if base.set_chain(&chain).is_err() {
            return;
        }
        if let Some(row) = self.row_mut(slot) {
            row.pending_patch = Some(base);
        }
        // Re-audition so the re-ordered chain is applied to the current sound.
        self.audition(slot);
    }

    /// Write one patch (its edited name + level) to its memory slot and verify by
    /// read-back. `Ok` on success; `Err(message)` if the patch isn't loaded or the
    /// unit isn't in BULK LOAD mode (the write is silently ignored there).
    fn store_one(&self, slot: u16) -> Result<(), String> {
        let Some(patch) = self.effective_patch(slot) else {
            return Err(format!("U{slot:03}: patch not loaded — audition it first"));
        };
        let write = device::lock(&self.device).write_patch(slot, &patch);
        if let Err(e) = write {
            return Err(format!("write U{slot:03}: {e}"));
        }
        let readback = device::lock(&self.device).read_patch(slot);
        match readback {
            Ok(got) if got.blocks == patch.blocks => Ok(()),
            _ => Err(format!(
                "U{slot:03} not stored — put the GX-700 in BULK LOAD mode \
                 (TUNER/UTILITY → MIDI BULK LOAD), then try again"
            )),
        }
    }

    fn set_name_edit(&mut self, slot: u16, name: String) {
        if let Some(row) = self.row_mut(slot) {
            row.name_edit = name;
        }
    }

    /// Save one patch (name + level) to the unit (per-row Save button).
    fn save_row(&mut self, slot: u16) {
        if !self.row(slot).is_some_and(PatchRow::dirty) {
            return;
        }
        self.ensure_loaded(slot);
        match self.store_one(slot) {
            Ok(()) => {
                self.commit_row(slot);
                self.status = format!("stored U{slot:03}");
                self.save_cache();
            }
            Err(msg) => self.status = msg,
        }
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
        self.ensure_loaded(slot);
        match self.effective_patch(slot) {
            Some(patch) => {
                self.status = format!("copied U{slot:03} {:?}", patch.name);
                self.clipboard = Some((slot, patch));
            }
            None => self.status = format!("U{slot:03}: nothing to copy — read the bank first"),
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
        let level = patch.output_level();
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
        if let Err(e) = patch.initialize() {
            self.status = format!("U{slot:03}: cannot clear — {e}");
            return;
        }
        let name = patch.name.clone();
        let level = patch.output_level();
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
            let level = patch.output_level();
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
    fn write_all(&mut self) -> usize {
        let dirty: Vec<u16> = self
            .rows
            .iter()
            .filter(|r| r.dirty())
            .map(|r| r.slot)
            .collect();
        if dirty.is_empty() {
            "no pending changes to store".clone_into(&mut self.status);
            return 0;
        }
        let mut stored = 0usize;
        let mut failed: Vec<u16> = Vec::new();
        let mut last_err = String::new();
        for slot in &dirty {
            self.ensure_loaded(*slot);
            match self.store_one(*slot) {
                Ok(()) => {
                    self.commit_row(*slot);
                    stored = stored.saturating_add(1);
                }
                Err(msg) => {
                    failed.push(*slot);
                    last_err = msg;
                }
            }
        }
        self.save_cache();
        if failed.is_empty() {
            self.status = format!("stored {stored} patch change(s)");
        } else {
            // A failed store almost always means the unit dropped out of BULK LOAD
            // mode: re-block on the probe so the dialog guides the user back in.
            self.bulk_ok = Some(false);
            let slots: Vec<String> = failed.iter().map(|s| slot_label(*s)).collect();
            self.status = format!(
                "stored {stored}, {} failed ({}) — {last_err}",
                failed.len(),
                slots.join(", ")
            );
        }
        stored
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
        let Some(slot) = self.now_playing else {
            ui.label("Audition a patch on the Patches tab to edit it here.");
            return;
        };
        let Some(eff) = self.effective_patch(slot) else {
            return;
        };
        ui.label(format!("U{slot:03}  {:?}", eff.name));
        ui.label(egui::RichText::new("Drag the ↕ handle to re-order the chain.").weak());
        ui.separator();
        let typed = TypedPatch::from_raw(&eff);
        // Drag-to-reorder: a separate ↕ handle per row is the drag source carrying
        // the chain index; the whole row is the drop target. Keeping the handle off
        // the name's rect means the name's click (select) isn't stolen by the drag.
        let mut reorder: Option<(usize, usize)> = None;
        for (idx, id) in eff.chain().into_iter().enumerate() {
            let Some(block) = Block::from_base(id) else {
                continue;
            };
            let enabled = block_enabled(&typed, block);
            let selected = self.selected_block == block;
            let cell = ui.horizontal(|ui| {
                let drag_id = egui::Id::new(("chain-drag", idx));
                ui.add_enabled_ui(self.editable(), |ui| {
                    ui.dnd_drag_source(drag_id, idx, |ui| {
                        ui.label(egui::RichText::new("↕").weak());
                    })
                    .response
                    .on_hover_text("drag to re-order the chain");
                });
                // A checkbox toggles the block's bypass directly; the name selects it.
                if let Some(p) = block_enable_param(block) {
                    ui.add_enabled_ui(self.editable(), |ui| {
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
            if self.editable()
                && let Some(from) = cell.response.dnd_release_payload::<usize>()
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
    fn show_block_params(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(slot) = self.now_playing else {
            ui.label("Audition a patch to edit its effects.");
            return;
        };
        let Some(eff) = self.effective_patch(slot) else {
            return;
        };
        let typed = TypedPatch::from_raw(&eff);
        let block = self.selected_block;
        ui.heading(block.label());
        egui::ScrollArea::vertical().show(ui, |ui| {
            // The Equalizer gets a custom band-table layout (curve + Gain/Freq/Q
            // grid); every other block uses the generic per-parameter list.
            if block == Block::Equalizer {
                self.show_eq_editor(ui, slot, &typed, actions);
                return;
            }
            if block == Block::Compressor {
                self.show_comp_editor(ui, slot, &typed, actions);
                return;
            }
            for &p in param::ALL {
                if p.block() != block {
                    continue;
                }
                let value = typed.get(p.key()).unwrap_or(Value::Int(0));
                param_widget(ui, slot, p, value, self.editable(), actions);
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
        let connected = self.editable();
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
        let connected = self.editable();
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

    fn show_patch_list(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.label(
            egui::RichText::new(
                "Click a slot to audition · drag the ↕ handle onto another slot to re-order the bank.",
            )
            .weak(),
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("patches")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    for row in &self.rows {
                        let playing = self.now_playing == Some(row.slot);
                        // Column 1: the slot id, click to audition. A slot whose read
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
                        // The whole cell is the drop target, so releasing anywhere on
                        // the row re-orders.
                        if self.editable()
                            && let Some(from) = cell.response.dnd_release_payload::<u16>()
                        {
                            actions.push(Action::ReorderPatch(*from, row.slot));
                        }

                        // Column 2: editable patch name (egui keeps the cursor by
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

                        // Column 3: output-level slider. Give it a fixed allocation
                        // so it does not expand to fill the row and starve the name
                        // field (an egui Slider grows to its available width).
                        let mut level = i32::from(row.pending_level.unwrap_or(row.stored_level));
                        let slider = egui::Slider::new(&mut level, 0..=100).suffix("%");
                        let size = [220.0, ui.spacing().interact_size.y];
                        let changed = ui
                            .add_enabled_ui(self.editable(), |ui| {
                                ui.add_sized(size, slider).changed()
                            })
                            .inner;
                        if changed {
                            let level = u8::try_from(level.clamp(0, 100)).unwrap_or(0);
                            actions.push(Action::SetLevel(row.slot, level));
                        }

                        // Column 4: per-row actions.
                        self.patch_row_buttons(ui, row, actions);
                        ui.end_row();
                    }
                });
        });
    }

    /// A patch row's action buttons (column 4): Save/Revert (enabled only when the
    /// row has an unsaved edit — their state is the "modified" indicator), and
    /// Copy/Paste/Clear (Paste also needs something on the clipboard).
    fn patch_row_buttons(&self, ui: &mut egui::Ui, row: &PatchRow, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(self.editable() && row.dirty(), |ui| {
                let save = action_button(ui, "Save", ActionKind::Commit).on_hover_text(
                    "store this patch (name + level) to the unit (needs BULK LOAD mode)",
                );
                if save.clicked() {
                    actions.push(Action::SaveRow(row.slot));
                }
                let revert = action_button(ui, "Revert", ActionKind::Caution)
                    .on_hover_text("discard edits, back to the values stored on the unit");
                if revert.clicked() {
                    actions.push(Action::RevertRow(row.slot));
                }
            });
            ui.separator();
            ui.add_enabled_ui(self.editable(), |ui| {
                if action_button(ui, "Copy", ActionKind::Read)
                    .on_hover_text("copy this patch to the clipboard")
                    .clicked()
                {
                    actions.push(Action::CopyRow(row.slot));
                }
            });
            ui.add_enabled_ui(self.editable() && self.clipboard.is_some(), |ui| {
                let hover = match &self.clipboard {
                    Some((from, p)) => format!("paste U{from:03} {:?} here (then Save)", p.name),
                    None => "Copy a patch first".to_owned(),
                };
                if action_button(ui, "Paste", ActionKind::Neutral)
                    .on_hover_text(hover)
                    .clicked()
                {
                    actions.push(Action::PasteRow(row.slot));
                }
            });
            ui.add_enabled_ui(self.editable(), |ui| {
                if action_button(ui, "Clear", ActionKind::Caution)
                    .on_hover_text(
                        "reset to an empty patch (name \"Empty\", level 0, effects off), then Save",
                    )
                    .clicked()
                {
                    actions.push(Action::ClearRow(row.slot));
                }
            });
            // Keep the last button clear of the vertical scrollbar, which otherwise
            // overlays (and clips) the Clear button.
            ui.add_space(18.0);
        });
    }

    /// The Presets tab: the factory presets (P001..P100). Clicking a preset loads
    /// it into the active sound (the temporary buffer) so it can be heard and used
    /// immediately — this works in Play mode, so no BULK LOAD switch is needed.
    fn show_preset_list(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.label(
            egui::RichText::new(
                "Click a preset to load it into the active sound (current buffer). \
                 Works in Play mode — no BULK LOAD needed.",
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
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("presets")
                .striped(true)
                .num_columns(3)
                .show(ui, |ui| {
                    for row in &self.presets {
                        let playing = self.now_playing == Some(row.slot);
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
                        ui.label(format!("{}%", row.stored_level));
                        ui.end_row();
                    }
                });
        });
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Audition(slot) => self.audition(slot),
            Action::SetLevel(slot, level) => self.set_level(slot, level),
            Action::SetParam(slot, key, value) => self.set_param(slot, key, value),
            Action::SelectTab(tab) => self.tab = tab,
            Action::SelectBlock(block) => self.selected_block = block,
            Action::ReorderChain(slot, from, to) => self.reorder_chain(slot, from, to),
            Action::ReorderPatch(from, to) => self.reorder_patches(from, to),
            Action::SetName(slot, name) => self.set_name_edit(slot, name),
            Action::SaveRow(slot) => self.save_row(slot),
            Action::RevertRow(slot) => self.revert_row(slot),
            Action::CopyRow(slot) => self.copy_row(slot),
            Action::PasteRow(slot) => self.paste_row(slot),
            Action::ClearRow(slot) => self.clear_row(slot),
            Action::Refresh => self.start_load(),
            Action::Retry => self.retry(),
            Action::OpenBulkPrompt => self.bulk_prompt = true,
            Action::CloseBulkPrompt => self.bulk_prompt = false,
            Action::WriteAll => {
                self.bulk_prompt = false;
                self.write_all();
            }
            Action::ProbeBulk => self.probe_now(),
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
            self.preset_loader = Some(Loader::spawn_range(
                Arc::clone(&self.device),
                PRESET_START,
                PRESET_END,
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
        self.drive_startup(ctx);
        self.drain_loader();
        self.maybe_load_presets();
        self.drain_preset_loader();
        if self.loader.is_some() || self.preset_loader.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        // Keep re-probing while waiting for the unit to enter BULK LOAD mode.
        if self.bulk_ok == Some(false) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        // Capture view state for persistence on exit.
        self.zoom = ctx.zoom_factor();
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.window = Some([rect.width(), rect.height()]);
        }

        let mut actions: Vec<Action> = Vec::new();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("GX-700");
                ui.separator();
                if ui
                    .selectable_label(self.tab == Tab::Patches, "Patches")
                    .clicked()
                {
                    actions.push(Action::SelectTab(Tab::Patches));
                }
                if ui.selectable_label(self.tab == Tab::Edit, "Edit").clicked() {
                    actions.push(Action::SelectTab(Tab::Edit));
                }
                if ui
                    .selectable_label(self.tab == Tab::Presets, "Presets")
                    .clicked()
                {
                    actions.push(Action::SelectTab(Tab::Presets));
                }
                ui.separator();
                if self.connected {
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
                            format!("Write changes to unit ({pending})"),
                            ActionKind::Commit,
                        )
                        .clicked()
                        {
                            actions.push(Action::OpenBulkPrompt);
                        }
                    });
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "not connected");
                    if action_button(ui, "Retry", ActionKind::Read).clicked() {
                        actions.push(Action::Retry);
                    }
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });

        match self.tab {
            Tab::Patches => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_patch_list(ui, &mut actions);
                });
            }
            Tab::Edit => {
                egui::SidePanel::left("blocks")
                    .resizable(true)
                    .default_width(180.0)
                    .show(ctx, |ui| {
                        self.show_block_list(ui, &mut actions);
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_block_params(ui, &mut actions);
                });
            }
            Tab::Presets => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.show_preset_list(ui, &mut actions);
                });
            }
        }

        self.show_bulk_modals(ctx, &mut actions);

        for action in actions {
            self.apply(action);
        }
    }
}

impl App {
    /// The BULK-LOAD modals: the blocking "enter BULK LOAD" gate shown until the
    /// unit is in the mode (the session stays in it, so no switching), and the
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
        });
    }
}
