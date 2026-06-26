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
use rackctl_gx700::{NAME_LEN, Param, RawPatch, Value, decode_name, encode_name};

use crate::config::{self, CachedRow, GuiConfig};
use crate::device::{self, Device, SharedDevice};
use crate::loader::{Loaded, Loader, USER_SLOTS};

/// Reopen the device on demand (the Retry button), e.g. after the port appears.
pub(crate) type Reopen = Box<dyn Fn() -> anyhow::Result<Device>>;

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
}

impl PatchRow {
    /// Whether the row has unsaved edits (a level or a name change).
    fn dirty(&self) -> bool {
        self.pending_level.is_some() || self.name_edit != self.name
    }
}

/// A UI interaction to apply after the render pass (avoids borrowing `self`
/// mutably while iterating the rows).
enum Action {
    Audition(u16),
    SetLevel(u16, u8),
    SetName(u16, String),
    SaveRow(u16),
    RevertRow(u16),
    Refresh,
    Retry,
    OpenBulkPrompt,
    CloseBulkPrompt,
    WriteAll,
}

pub(crate) struct App {
    device: SharedDevice,
    connected: bool,
    reopen: Reopen,
    loader: Option<Loader>,
    /// Slots received from the loader in the current load (for the progress bar).
    progress: u16,
    rows: Vec<PatchRow>,
    now_playing: Option<u16>,
    bulk_prompt: bool,
    status: String,
    zoom: f32,
    window: Option<[f32; 2]>,
}

impl App {
    pub(crate) fn new(device: Device, connected: bool, reopen: Reopen) -> Self {
        let cfg = config::load();
        let mut rows: Vec<PatchRow> = (1..=USER_SLOTS)
            .map(|slot| PatchRow {
                slot,
                name: String::new(),
                name_edit: String::new(),
                stored_level: 0,
                chain: Vec::new(),
                full: None,
                pending_level: None,
            })
            .collect();
        // Show the cached bank instantly, before the (slow) re-read fills it in.
        for cached in config::load_cache() {
            if let Some(row) = rows.get_mut(usize::from(cached.slot.saturating_sub(1))) {
                row.name.clone_from(&cached.name);
                row.name_edit = cached.name;
                row.stored_level = cached.output_level;
                row.chain = cached.chain;
            }
        }

        let device = Arc::new(Mutex::new(device));
        let loader = connected.then(|| Loader::spawn(Arc::clone(&device)));
        Self {
            device,
            connected,
            reopen,
            loader,
            progress: 0,
            rows,
            now_playing: None,
            bulk_prompt: false,
            status: if connected {
                "reading patch bank…".to_owned()
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

    fn row(&self, slot: u16) -> Option<&PatchRow> {
        self.rows.get(usize::from(slot.saturating_sub(1)))
    }

    fn row_mut(&mut self, slot: u16) -> Option<&mut PatchRow> {
        self.rows.get_mut(usize::from(slot.saturating_sub(1)))
    }

    fn dirty_count(&self) -> usize {
        self.rows.iter().filter(|r| r.dirty()).count()
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

    /// After a successful store, commit the edits: the level and the (normalized,
    /// device-encoded) name become the stored values, clearing the dirty state.
    fn commit_row(&mut self, slot: u16) {
        if let Some(row) = self.row_mut(slot) {
            if let Some(level) = row.pending_level.take() {
                row.stored_level = level;
            }
            let normalized = decode_name(&encode_name(&row.name_edit));
            row.name.clone_from(&normalized);
            row.name_edit = normalized;
        }
    }

    /// Spawn (or restart) the background bank read.
    fn start_load(&mut self) {
        if !self.connected {
            return;
        }
        self.loader = None; // cancel + join any in-flight load first
        self.progress = 0;
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
                "connected".clone_into(&mut self.status);
                self.start_load();
            }
            Err(e) => self.status = format!("connect failed: {e}"),
        }
    }

    /// Write `slot`'s patch into the current sound so it can be heard.
    fn audition(&mut self, slot: u16) {
        if !self.connected {
            return;
        }
        if self.row(slot).is_some_and(|r| r.full.is_none()) {
            let read = device::lock(&self.device).read_patch(slot);
            match read {
                Ok(patch) => {
                    if let Some(row) = self.row_mut(slot) {
                        row.full = Some(patch);
                    }
                }
                Err(e) => {
                    self.status = format!("read U{slot:03}: {e}");
                    return;
                }
            }
        }
        let patch = self.row(slot).and_then(|r| r.full.clone());
        if let Some(patch) = patch {
            let written = device::lock(&self.device).write_current_patch(&patch);
            match written {
                Ok(_) => {
                    self.now_playing = Some(slot);
                    self.status = format!("auditioning U{slot:03} {:?}", patch.name);
                }
                Err(e) => self.status = format!("audition U{slot:03}: {e}"),
            }
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
        if let Some(row) = self.row_mut(slot) {
            row.pending_level = Some(level);
            if let Some(full) = row.full.as_mut() {
                let _ = full.set_output_level(level);
            }
        }
    }

    /// Write one patch (its edited name + level) to its memory slot and verify by
    /// read-back. `Ok` on success; `Err(message)` if the patch isn't loaded or the
    /// unit isn't in BULK LOAD mode (the write is silently ignored there).
    fn store_one(&self, slot: u16) -> Result<(), String> {
        let Some(row) = self.row(slot) else {
            return Err(format!("U{slot:03}: no such patch"));
        };
        let Some(mut patch) = row.full.clone() else {
            return Err(format!("U{slot:03}: patch not loaded — audition it first"));
        };
        let level = row.pending_level.unwrap_or(row.stored_level);
        if patch.set_output_level(level).is_err() {
            return Err(format!("U{slot:03}: patch has no level block"));
        }
        if patch.set_name(&row.name_edit).is_err() {
            return Err(format!("U{slot:03}: patch has no name block"));
        }
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

    /// Revert one patch's name and level back to the values stored on the unit
    /// (per-row Revert button), updating the cached patch and live sound if playing.
    fn revert_row(&mut self, slot: u16) {
        let Some((stored_level, stored_name)) =
            self.row(slot).map(|r| (r.stored_level, r.name.clone()))
        else {
            return;
        };
        if let Some(row) = self.row_mut(slot) {
            row.pending_level = None;
            row.name_edit = stored_name;
            if let Some(full) = row.full.as_mut() {
                let _ = full.set_output_level(stored_level);
            }
        }
        if self.now_playing == Some(slot)
            && let Some(param) = Param::from_key("output-level")
        {
            let _ = device::lock(&self.device).set(param, Value::Int(i32::from(stored_level)));
        }
        self.status = format!("reverted U{slot:03}");
    }

    /// Store every pending change (name + level) to memory in one batch (the
    /// "Write changes to unit" button). Stops at the first failure with guidance.
    fn write_all(&mut self) {
        let dirty: Vec<u16> = self
            .rows
            .iter()
            .filter(|r| r.dirty())
            .map(|r| r.slot)
            .collect();
        if dirty.is_empty() {
            "no pending changes to store".clone_into(&mut self.status);
            return;
        }
        let mut stored = 0usize;
        for slot in &dirty {
            self.ensure_loaded(*slot);
            match self.store_one(*slot) {
                Ok(()) => {
                    self.commit_row(*slot);
                    stored = stored.saturating_add(1);
                }
                Err(msg) => {
                    self.status = msg;
                    break;
                }
            }
        }
        if stored == dirty.len() {
            self.status = format!("stored {stored} patch change(s)");
        }
        self.save_cache();
    }

    fn save_cache(&self) {
        let rows: Vec<CachedRow> = self
            .rows
            .iter()
            .map(|r| CachedRow {
                slot: r.slot,
                name: r.name.clone(),
                output_level: r.stored_level,
                chain: r.chain.clone(),
            })
            .collect();
        config::save_cache(&rows);
    }

    /// Render the scrollable patch list (name, level slider, Save/Revert), pushing
    /// any interactions into `actions` to apply after the render pass.
    fn show_patch_list(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("patches")
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    for row in &self.rows {
                        let playing = self.now_playing == Some(row.slot);
                        // Column 1: the slot id, click to audition.
                        let id = egui::SelectableLabel::new(playing, format!("U{:03}", row.slot));
                        if ui.add_enabled(self.connected, id).clicked() {
                            actions.push(Action::Audition(row.slot));
                        }

                        // Column 2: editable patch name (egui keeps the cursor by
                        // widget id, so a per-frame clone of the buffer is fine).
                        let mut name = row.name_edit.clone();
                        let edit = egui::TextEdit::singleline(&mut name)
                            .hint_text("—")
                            .char_limit(NAME_LEN)
                            .desired_width(200.0);
                        if ui.add_enabled(self.connected, edit).changed() {
                            actions.push(Action::SetName(row.slot, name));
                        }

                        // Column 3: output-level slider.
                        let mut level = i32::from(row.pending_level.unwrap_or(row.stored_level));
                        let slider = egui::Slider::new(&mut level, 0..=100).suffix("%");
                        if ui.add_enabled(self.connected, slider).changed() {
                            let level = u8::try_from(level.clamp(0, 100)).unwrap_or(0);
                            actions.push(Action::SetLevel(row.slot, level));
                        }

                        // Column 4: Save/Revert, enabled only when the row has an
                        // unsaved edit (their state is the "modified" indicator).
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(self.connected && row.dirty(), |ui| {
                                let save = ui.button("Save").on_hover_text(
                                    "store this patch (name + level) to the unit (needs BULK LOAD mode)",
                                );
                                if save.clicked() {
                                    actions.push(Action::SaveRow(row.slot));
                                }
                                let revert = ui.button("Revert").on_hover_text(
                                    "discard edits, back to the values stored on the unit",
                                );
                                if revert.clicked() {
                                    actions.push(Action::RevertRow(row.slot));
                                }
                            });
                        });
                        ui.end_row();
                    }
                });
        });
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Audition(slot) => self.audition(slot),
            Action::SetLevel(slot, level) => self.set_level(slot, level),
            Action::SetName(slot, name) => self.set_name_edit(slot, name),
            Action::SaveRow(slot) => self.save_row(slot),
            Action::RevertRow(slot) => self.revert_row(slot),
            Action::Refresh => self.start_load(),
            Action::Retry => self.retry(),
            Action::OpenBulkPrompt => self.bulk_prompt = true,
            Action::CloseBulkPrompt => self.bulk_prompt = false,
            Action::WriteAll => {
                self.bulk_prompt = false;
                self.write_all();
            }
        }
    }

    fn drain_loader(&mut self) {
        let Some(loader) = &self.loader else {
            return;
        };
        let mut done = false;
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
                    }
                }
                Loaded::Failed(slot, msg) => {
                    self.progress = self.progress.saturating_add(1);
                    self.status = format!("U{slot:03}: {msg}");
                }
                Loaded::Done => done = true,
            }
        }
        if done {
            self.loader = None;
            "bank loaded".clone_into(&mut self.status);
            self.save_cache();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_loader();
        if self.loader.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        // Capture view state for persistence on exit.
        self.zoom = ctx.zoom_factor();
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.window = Some([rect.width(), rect.height()]);
        }

        let mut actions: Vec<Action> = Vec::new();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("GX-700 Patches");
                ui.separator();
                if self.connected {
                    if self.loader.is_some() {
                        let frac = f32::from(self.progress) / f32::from(USER_SLOTS);
                        ui.add(
                            egui::ProgressBar::new(frac)
                                .desired_width(160.0)
                                .text(format!("reading {}/{USER_SLOTS}", self.progress)),
                        );
                    } else if ui.button("Refresh").clicked() {
                        actions.push(Action::Refresh);
                    }
                    let pending = self.dirty_count();
                    ui.add_enabled_ui(pending > 0, |ui| {
                        if ui
                            .button(format!("Write changes to unit ({pending})"))
                            .clicked()
                        {
                            actions.push(Action::OpenBulkPrompt);
                        }
                    });
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "not connected");
                    if ui.button("Retry").clicked() {
                        actions.push(Action::Retry);
                    }
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.show_patch_list(ui, &mut actions);
        });

        if self.bulk_prompt {
            egui::Window::new("Write changes to the unit")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(format!("{} patch change(s) to store.", self.dirty_count()));
                    ui.label(
                        "On the GX-700: press TUNER/UTILITY, select \"MIDI BULK LOAD\" \
                         (the display shows \"Waiting…\"), then click Write. \
                         Press PLAY on the unit when done.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Write").clicked() {
                            actions.push(Action::WriteAll);
                        }
                        if ui.button("Cancel").clicked() {
                            actions.push(Action::CloseBulkPrompt);
                        }
                    });
                });
        }

        for action in actions {
            self.apply(action);
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
