//! The Eleven Rack patch-librarian app: one [`App`] holding the bank state, the
//! background loader/writer, and the tab views. Mirrors the GX-700 GUI's
//! action-collected-then-applied loop — render fns only push [`Action`]s; `update`
//! drains them through [`App::apply`], so the borrow of `self.rows` during render
//! never fights a mutation.

use std::sync::{Arc, Mutex};

use egui::{Color32, DragValue, RichText, ScrollArea, TextEdit};
use rackctl_eleven::PatchBackup;
use rackctl_eleven_lib::{Scene, manage};
use rackctl_ui::{ActionKind, action_button, icon};

use crate::config::{self, CachedRow, GuiConfig};
use crate::device::{Device, SharedDevice, lock};
use crate::loader::{Loaded, Loader, USER_SLOTS};
use crate::writer::{WriteJob, Writer, Written};

/// A closure that (re)opens the device, for Retry / Connect.
pub(crate) type Reopen = Box<dyn FnMut() -> anyhow::Result<Device>>;

/// The tabs across the top of the window.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    /// The User bank: browse, audition, rename, copy, write.
    Patches,
    /// The read-only Factory presets: audition, copy to a User slot.
    Presets,
    /// On-disk library: device backups, `.tfx` imports, scenes.
    Library,
    /// Whole-bank scene composer.
    Scene,
    /// Read-only patch inspector + MIDI-CC quick-controls.
    Inspect,
}

impl Tab {
    fn as_key(self) -> &'static str {
        match self {
            Self::Patches => "patches",
            Self::Presets => "presets",
            Self::Library => "library",
            Self::Scene => "scene",
            Self::Inspect => "inspect",
        }
    }
    fn from_key(k: &str) -> Option<Self> {
        Some(match k {
            "patches" => Self::Patches,
            "presets" => Self::Presets,
            "library" => Self::Library,
            "scene" => Self::Scene,
            "inspect" => Self::Inspect,
            _ => return None,
        })
    }
}

/// One User-bank row.
struct PatchRow {
    slot: u8,
    /// Committed name (as stored on the unit / last written).
    name: String,
    /// Rename buffer, edited in the list.
    name_edit: String,
    /// Whole captured patch, read on demand (Copy / write source).
    full: Option<PatchBackup>,
    /// Staged whole-patch content (from Paste), written on Save / Write changes.
    pending: Option<PatchBackup>,
    /// This slot's last read failed.
    failed: bool,
}

impl PatchRow {
    fn new(slot: u8) -> Self {
        Self {
            slot,
            name: String::new(),
            name_edit: String::new(),
            full: None,
            pending: None,
            failed: false,
        }
    }
    /// A row is dirty when its name was edited or a patch is staged.
    fn dirty(&self) -> bool {
        self.name != self.name_edit || self.pending.is_some()
    }
}

/// Intents pushed by the render pass, applied after the frame.
enum Action {
    SelectTab(Tab),
    Audition(u8),
    SetName(u8, String),
    SaveRow(u8),
    RevertRow(u8),
    CopyRow(u8),
    PasteRow(u8),
    ClearRow(u8),
    WriteAll,
    Refresh,
    Retry,
    // Presets
    LoadPresets,
    AuditionFactory(u8),
    CopyFactoryToUser(u8),
    SetTargetSlot(u8),
    // Library
    SetSaveName(String),
    SaveCurrentAs,
    SetImportPath(String),
    ImportTfx,
    LoadBackup(String),
    DeleteBackup(String),
    ViewImport(String),
    DeleteImport(String),
    CloseView,
    RefreshLibrary,
    // Scene
    SetSceneName(String),
    SceneNew,
    SceneCapture,
    SceneSave,
    SceneApply,
    SceneLoad(String),
    SceneDelete(String),
    // Inspect
    InspectCapture,
    SendCc(&'static str, Option<&'static str>, u8),
}

/// The application.
pub(crate) struct App {
    device: SharedDevice,
    connected: bool,
    reopen: Reopen,
    port: Option<String>,

    rows: Vec<PatchRow>,
    loader: Option<Loader>,
    /// How many slot names have arrived (progress).
    progress: usize,
    writer: Option<Writer>,
    write_ok: usize,
    write_failed: usize,

    now_playing: Option<u8>,
    /// The last Copy'd patch, for Paste onto another slot.
    clipboard: Option<PatchBackup>,

    // Presets tab: factory slot names (read on demand — a slow select+read scan).
    presets: Vec<(u8, String)>,
    preset_loader: Option<Loader>,
    preset_progress: usize,

    // Library tab: on-disk lists + entry buffers.
    lib_backups: Vec<String>,
    lib_imports: Vec<String>,
    save_name: String,
    import_path: String,
    /// Target User slot for "load a backup / copy a preset to the unit".
    target_slot: u8,
    /// JSON of an imported `.tfx` patch being viewed.
    view_text: Option<String>,

    // Scene tab: a composed whole-bank snapshot.
    compose: Vec<Option<PatchBackup>>,
    compose_name: String,
    scene_capture: Option<Loader>,
    scene_progress: usize,
    lib_scenes: Vec<String>,

    // Inspect tab: a captured patch + the CC channel.
    inspect: Option<PatchBackup>,
    cc_channel: u8,

    tab: Tab,
    status: String,
    zoom: f32,
    window: Option<[f32; 2]>,
}

impl App {
    /// Build the app, seeding the list from the on-disk name cache and starting a
    /// background directory read if connected.
    pub(crate) fn new(
        dev: Device,
        connected: bool,
        reopen: Reopen,
        offline: bool,
        port: Option<String>,
    ) -> Self {
        let cfg = config::load();
        let mut rows: Vec<PatchRow> = (0..USER_SLOTS).map(PatchRow::new).collect();
        for c in config::load_cache() {
            if let Some(r) = rows.get_mut(usize::from(c.slot)) {
                r.name.clone_from(&c.name);
                r.name_edit.clone_from(&c.name);
            }
        }
        let tab = cfg
            .tab
            .as_deref()
            .and_then(Tab::from_key)
            .unwrap_or(Tab::Patches);
        let mut app = Self {
            device: Arc::new(Mutex::new(dev)),
            connected,
            reopen,
            port,
            rows,
            loader: None,
            progress: 0,
            writer: None,
            write_ok: 0,
            write_failed: 0,
            now_playing: None,
            clipboard: None,
            presets: (0..USER_SLOTS).map(|s| (s, String::new())).collect(),
            preset_loader: None,
            preset_progress: 0,
            lib_backups: config::json_stems(config::backups_dir()),
            lib_imports: config::json_stems(config::patches_dir()),
            save_name: String::new(),
            import_path: String::new(),
            target_slot: 0,
            view_text: None,
            compose: vec![None; usize::from(USER_SLOTS)],
            compose_name: String::new(),
            scene_capture: None,
            scene_progress: 0,
            lib_scenes: config::json_stems(config::scenes_dir()),
            inspect: None,
            cc_channel: 1,
            tab,
            status: if connected {
                "Reading the bank…".to_owned()
            } else if offline {
                "Offline — connect to browse the unit.".to_owned()
            } else {
                "Not connected. Press Retry.".to_owned()
            },
            zoom: cfg.zoom,
            window: cfg.window,
        };
        if connected {
            app.start_directory_load();
        }
        app
    }

    /// The saved zoom factor (applied at startup by `main`).
    pub(crate) fn zoom(&self) -> f32 {
        self.zoom
    }

    fn shared(&self) -> SharedDevice {
        Arc::clone(&self.device)
    }

    /// Start (or restart) a background read of the User-bank names.
    fn start_directory_load(&mut self) {
        self.progress = 0;
        for r in &mut self.rows {
            r.failed = false;
        }
        self.loader = Some(Loader::spawn_directory(self.shared()));
        "Reading the bank…".clone_into(&mut self.status);
    }

    /// Drive the background threads once per frame; request a repaint while busy.
    fn pump_background(&mut self, ctx: &egui::Context) {
        // Bitwise-or so every source drains each frame (no short-circuit); any busy
        // source keeps the window repainting.
        let busy = self.drain_directory()
            | self.drain_presets()
            | self.drain_scene()
            | self.drain_writer();
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    /// Drain the User-bank directory/name reader into `rows`. Returns busy.
    fn drain_directory(&mut self) -> bool {
        let Some(loader) = &self.loader else {
            return false;
        };
        let mut done = false;
        for msg in loader.drain() {
            match msg {
                Loaded::Name(slot, name) => {
                    if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                        r.name.clone_from(&name);
                        r.name_edit.clone_from(&name);
                        r.failed = false;
                    }
                    self.progress += 1;
                }
                Loaded::Patch(slot, patch) => {
                    if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                        r.full = Some(patch);
                    }
                }
                Loaded::Failed(slot) => {
                    if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                        r.failed = true;
                    }
                }
                Loaded::Aborted(msg) => {
                    self.status = msg;
                    done = true;
                }
                Loaded::Done => done = true,
            }
        }
        if done {
            self.loader = None;
            self.save_name_cache();
            "Bank loaded.".clone_into(&mut self.status);
            false
        } else {
            true
        }
    }

    /// Drain the Factory-name reader into `presets`. Returns busy.
    fn drain_presets(&mut self) -> bool {
        let Some(loader) = &self.preset_loader else {
            return false;
        };
        let mut done = false;
        for msg in loader.drain() {
            match msg {
                Loaded::Name(slot, name) => {
                    if let Some(p) = self.presets.get_mut(usize::from(slot)) {
                        p.1 = name;
                    }
                    self.preset_progress += 1;
                }
                Loaded::Aborted(m) => {
                    self.status = m;
                    done = true;
                }
                Loaded::Done => done = true,
                _ => {}
            }
        }
        if done {
            self.preset_loader = None;
            "Factory presets loaded.".clone_into(&mut self.status);
            false
        } else {
            true
        }
    }

    /// Drain the scene bank-capture into `compose`. Returns busy.
    fn drain_scene(&mut self) -> bool {
        let Some(loader) = &self.scene_capture else {
            return false;
        };
        let mut done = false;
        for msg in loader.drain() {
            match msg {
                Loaded::Patch(slot, patch) => {
                    if let Some(c) = self.compose.get_mut(usize::from(slot)) {
                        *c = Some(patch);
                    }
                    self.scene_progress += 1;
                }
                Loaded::Aborted(m) => {
                    self.status = m;
                    done = true;
                }
                Loaded::Done => done = true,
                _ => {}
            }
        }
        if done {
            self.scene_capture = None;
            let n = self.compose.iter().filter(|c| c.is_some()).count();
            self.status = format!("Captured {n} patches into the scene.");
            false
        } else {
            true
        }
    }

    /// Drain the background writer, committing each verified slot. Returns busy.
    fn drain_writer(&mut self) -> bool {
        let Some(writer) = &self.writer else {
            return false;
        };
        let total = writer.total();
        let mut done = false;
        for msg in writer.drain() {
            match msg {
                Written::Ok(slot) => {
                    self.write_ok += 1;
                    if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                        if let Some(p) = r.pending.take() {
                            r.full = Some(p);
                        }
                        r.name.clone_from(&r.name_edit);
                    }
                }
                Written::Failed(slot, msg) => {
                    self.write_failed += 1;
                    self.status = format!("{} write failed: {msg}", slot_label(slot));
                }
                Written::Done => done = true,
            }
        }
        if done {
            self.writer = None;
            self.status = format!(
                "Wrote {} of {}, {} failed.",
                self.write_ok, total, self.write_failed
            );
            self.save_name_cache();
            false
        } else {
            true
        }
    }

    fn save_name_cache(&self) {
        let rows: Vec<CachedRow> = self
            .rows
            .iter()
            .filter(|r| !r.name.is_empty())
            .map(|r| CachedRow {
                slot: r.slot,
                name: r.name.clone(),
            })
            .collect();
        config::save_cache(&rows);
    }

    fn dirty_count(&self) -> usize {
        self.rows.iter().filter(|r| r.dirty()).count()
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::SelectTab(t) => self.tab = t,
            Action::Audition(slot) => self.audition(slot),
            Action::SetName(slot, name) => {
                if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                    r.name_edit = name;
                }
            }
            Action::SaveRow(slot) => self.write_rows(vec![slot]),
            Action::RevertRow(slot) => {
                if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                    r.name_edit.clone_from(&r.name);
                    r.pending = None;
                }
            }
            Action::CopyRow(slot) => self.copy_row(slot),
            Action::PasteRow(slot) => {
                if let Some(p) = self.clipboard.clone()
                    && let Some(r) = self.rows.get_mut(usize::from(slot))
                {
                    r.pending = Some(p);
                }
            }
            Action::ClearRow(slot) => {
                if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                    r.pending = None;
                }
            }
            Action::WriteAll => {
                let dirty: Vec<u8> = self
                    .rows
                    .iter()
                    .filter(|r| r.dirty())
                    .map(|r| r.slot)
                    .collect();
                self.write_rows(dirty);
            }
            Action::Refresh => self.start_directory_load(),
            Action::Retry => self.retry(),
            Action::LoadPresets => self.load_presets(),
            Action::AuditionFactory(slot) => self.audition_factory(slot),
            Action::CopyFactoryToUser(from) => self.copy_factory(from),
            Action::SetTargetSlot(s) => self.target_slot = s,
            Action::SetSaveName(n) => self.save_name = n,
            Action::SaveCurrentAs => self.save_current(),
            Action::SetImportPath(p) => self.import_path = p,
            Action::ImportTfx => self.import_tfx(),
            Action::LoadBackup(name) => self.load_backup(&name),
            Action::DeleteBackup(name) => {
                let _ = config::delete_named(config::backups_dir(), &name);
                self.lib_backups = config::json_stems(config::backups_dir());
            }
            Action::ViewImport(name) => self.view_import(&name),
            Action::DeleteImport(name) => {
                let _ = config::delete_named(config::patches_dir(), &name);
                self.lib_imports = config::json_stems(config::patches_dir());
            }
            Action::CloseView => self.view_text = None,
            Action::RefreshLibrary => self.refresh_library(),
            Action::SetSceneName(n) => self.compose_name = n,
            Action::SceneNew => {
                self.compose = vec![None; usize::from(USER_SLOTS)];
                "New empty scene.".clone_into(&mut self.status);
            }
            Action::SceneCapture => self.scene_capture(),
            Action::SceneSave => self.scene_save(),
            Action::SceneApply => self.scene_apply(),
            Action::SceneLoad(name) => self.scene_load(&name),
            Action::SceneDelete(name) => {
                let _ = config::delete_named(config::scenes_dir(), &name);
                self.lib_scenes = config::json_stems(config::scenes_dir());
            }
            Action::InspectCapture => self.inspect_capture(),
            Action::SendCc(name, fx, value) => self.send_cc(name, fx, value),
        }
    }

    /// Select a slot on the unit so its sound plays (audition).
    fn audition(&mut self, slot: u8) {
        if !self.connected {
            return;
        }
        match lock(&self.device).select_rig(0, slot) {
            Ok(()) => {
                self.now_playing = Some(slot);
                self.status = format!("Auditioning {}.", slot_label(slot));
            }
            Err(e) => self.status = format!("select failed: {e}"),
        }
    }

    /// Capture a slot's whole patch into the clipboard (and the row's cache).
    fn copy_row(&mut self, slot: u8) {
        if !self.connected {
            return;
        }
        match manage::capture(&mut **lock(&self.device), Some(slot)) {
            Ok(patch) => {
                if let Some(r) = self.rows.get_mut(usize::from(slot)) {
                    r.full = Some(patch.clone());
                }
                self.clipboard = Some(patch);
                self.status = format!("Copied {}.", slot_label(slot));
            }
            Err(e) => self.status = format!("copy failed: {e}"),
        }
    }

    /// Kick off a background write of the given dirty rows.
    fn write_rows(&mut self, slots: Vec<u8>) {
        if !self.connected || slots.is_empty() {
            return;
        }
        let mut jobs = Vec::new();
        for slot in slots {
            let Some(r) = self.rows.get(usize::from(slot)) else {
                continue;
            };
            if let Some(patch) = &r.pending {
                jobs.push(WriteJob::Restore(slot, patch.clone()));
            } else if r.name != r.name_edit {
                jobs.push(WriteJob::Rename(slot, r.name_edit.clone()));
            }
        }
        if jobs.is_empty() {
            return;
        }
        self.write_ok = 0;
        self.write_failed = 0;
        self.writer = Some(Writer::spawn(self.shared(), jobs));
        "Writing changes…".clone_into(&mut self.status);
    }

    fn retry(&mut self) {
        match (self.reopen)() {
            Ok(dev) => {
                self.device = Arc::new(Mutex::new(dev));
                self.connected = true;
                self.start_directory_load();
            }
            Err(e) => self.status = format!("connect failed: {e}"),
        }
    }

    fn busy(&self) -> bool {
        self.loader.is_some()
            || self.writer.is_some()
            || self.preset_loader.is_some()
            || self.scene_capture.is_some()
    }

    // ---- Presets ----

    fn load_presets(&mut self) {
        if !self.connected {
            return;
        }
        self.preset_progress = 0;
        self.preset_loader = Some(Loader::spawn_factory(self.shared()));
        "Reading factory presets (this also auditions each)…".clone_into(&mut self.status);
    }

    fn audition_factory(&mut self, slot: u8) {
        if self.connected {
            let _ = lock(&self.device).select_rig(1, slot);
            self.status = format!("Auditioning factory {slot}.");
        }
    }

    fn copy_factory(&mut self, from: u8) {
        if !self.connected {
            return;
        }
        let to = self.target_slot;
        match manage::copy_slot(&mut **lock(&self.device), manage::FACTORY_BANK, from, to) {
            Ok(_) => {
                if let Some((_, name)) = self.presets.get(usize::from(from)) {
                    let name = name.clone();
                    if let Some(r) = self.rows.get_mut(usize::from(to)) {
                        r.name.clone_from(&name);
                        r.name_edit = name;
                        r.full = None;
                    }
                }
                self.status = format!("Copied factory {from} → {}.", slot_label(to));
            }
            Err(e) => self.status = format!("copy failed: {e}"),
        }
    }

    // ---- Library ----

    fn refresh_library(&mut self) {
        self.lib_backups = config::json_stems(config::backups_dir());
        self.lib_imports = config::json_stems(config::patches_dir());
        self.lib_scenes = config::json_stems(config::scenes_dir());
    }

    fn save_current(&mut self) {
        if !self.connected || self.save_name.is_empty() {
            return;
        }
        let name = self.save_name.clone();
        match manage::capture_to_library(&mut **lock(&self.device), &name, None) {
            Ok(_) => {
                self.save_name.clear();
                self.lib_backups = config::json_stems(config::backups_dir());
                self.status = format!("Saved current sound as backup {name:?}.");
            }
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }

    fn import_tfx(&mut self) {
        let file = std::path::PathBuf::from(self.import_path.trim());
        if file.as_os_str().is_empty() {
            return;
        }
        match rackctl_eleven_lib::import_tfx(&file) {
            Ok(patch) => {
                let name = if patch.name.is_empty() {
                    file.file_stem()
                        .map_or("imported", |s| s.to_str().unwrap_or("imported"))
                        .to_owned()
                } else {
                    patch.name.clone()
                };
                match rackctl_eleven_lib::save_patch(&name, &patch) {
                    Ok(_) => {
                        self.import_path.clear();
                        self.lib_imports = config::json_stems(config::patches_dir());
                        self.status = format!(
                            "Imported {name:?} (view-only — .tfx can't be sent to the unit)."
                        );
                    }
                    Err(e) => self.status = format!("save import failed: {e}"),
                }
            }
            Err(e) => self.status = format!("import failed: {e}"),
        }
    }

    fn load_backup(&mut self, name: &str) {
        if !self.connected {
            return;
        }
        let to = self.target_slot;
        match manage::restore_from_library(&mut **lock(&self.device), name, to) {
            Ok((patch, report)) => {
                if let Some(r) = self.rows.get_mut(usize::from(to)) {
                    r.name.clone_from(&patch.name);
                    r.name_edit.clone_from(&patch.name);
                    r.full = Some(patch);
                }
                self.status = format!("Loaded {name:?} → {} ({report}).", slot_label(to));
            }
            Err(e) => self.status = format!("load failed: {e}"),
        }
    }

    fn view_import(&mut self, name: &str) {
        match rackctl_eleven_lib::load_patch(name) {
            Ok(patch) => {
                self.view_text = rackctl_eleven_lib::patch_to_json(&patch).ok();
                self.status = format!("Viewing {name:?}.");
            }
            Err(e) => self.status = format!("open failed: {e}"),
        }
    }

    // ---- Scene ----

    fn scene_capture(&mut self) {
        if !self.connected {
            return;
        }
        self.compose = vec![None; usize::from(USER_SLOTS)];
        self.scene_progress = 0;
        self.scene_capture = Some(Loader::spawn_capture(
            self.shared(),
            (0..USER_SLOTS).collect(),
        ));
        "Capturing the whole bank into a scene…".clone_into(&mut self.status);
    }

    fn scene_save(&mut self) {
        if self.compose_name.is_empty() {
            "Name the scene first.".clone_into(&mut self.status);
            return;
        }
        let mut scene = Scene::new(&self.compose_name);
        for (slot, p) in self.compose.iter().enumerate() {
            if let (Some(patch), Ok(s)) = (p, u8::try_from(slot)) {
                scene.patches.insert(s, patch.clone());
            }
        }
        match rackctl_eleven_lib::save_scene(&scene) {
            Ok(_) => {
                self.lib_scenes = config::json_stems(config::scenes_dir());
                self.status = format!(
                    "Saved scene {:?} ({} patches).",
                    scene.name,
                    scene.patches.len()
                );
            }
            Err(e) => self.status = format!("scene save failed: {e}"),
        }
    }

    fn scene_apply(&mut self) {
        if !self.connected {
            return;
        }
        let jobs: Vec<WriteJob> = self
            .compose
            .iter()
            .enumerate()
            .filter_map(|(slot, p)| {
                let patch = p.as_ref()?;
                let s = u8::try_from(slot).ok()?;
                Some(WriteJob::Restore(s, patch.clone()))
            })
            .collect();
        if jobs.is_empty() {
            "Scene is empty.".clone_into(&mut self.status);
            return;
        }
        self.write_ok = 0;
        self.write_failed = 0;
        self.writer = Some(Writer::spawn(self.shared(), jobs));
        "Applying scene to the unit…".clone_into(&mut self.status);
    }

    fn scene_load(&mut self, name: &str) {
        match rackctl_eleven_lib::load_scene(name) {
            Ok(scene) => {
                self.compose = vec![None; usize::from(USER_SLOTS)];
                for (slot, patch) in scene.patches {
                    if let Some(c) = self.compose.get_mut(usize::from(slot)) {
                        *c = Some(patch);
                    }
                }
                self.compose_name = scene.name;
                self.status = format!("Loaded scene {name:?}.");
            }
            Err(e) => self.status = format!("scene load failed: {e}"),
        }
    }

    // ---- Inspect ----

    fn inspect_capture(&mut self) {
        if !self.connected {
            return;
        }
        match manage::capture(&mut **lock(&self.device), None) {
            Ok(patch) => {
                self.status = format!("Captured current sound {:?}.", patch.name);
                self.inspect = Some(patch);
            }
            Err(e) => self.status = format!("capture failed: {e}"),
        }
    }

    fn send_cc(&mut self, name: &'static str, fx: Option<&'static str>, value: u8) {
        if !self.connected {
            return;
        }
        match manage::send_named_cc(
            &mut **lock(&self.device),
            name,
            value,
            None,
            fx,
            None,
            self.cc_channel,
        ) {
            Ok((cc, _)) => self.status = format!("Sent {name} = {value} (CC {cc})."),
            Err(e) => self.status = format!("CC failed: {e}"),
        }
    }
}

/// A display label for a User slot: banks of four, `A1`..`Z4`, else `U<slot>`.
fn slot_label(slot: u8) -> String {
    let bank = slot / 4;
    let pos = slot % 4 + 1;
    if bank < 26 {
        format!("{}{}", (b'A' + bank) as char, pos)
    } else {
        format!("U{slot}")
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_background(ctx);
        self.zoom = ctx.zoom_factor();
        self.window = Some((ctx.screen_rect().size() * self.zoom).into());

        let mut actions: Vec<Action> = Vec::new();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Eleven Rack");
                ui.separator();
                for (tab, label) in [
                    (Tab::Patches, "Patches"),
                    (Tab::Presets, "Presets"),
                    (Tab::Library, "Library"),
                    (Tab::Scene, "Scene"),
                    (Tab::Inspect, "Inspect"),
                ] {
                    if ui.selectable_label(self.tab == tab, label).clicked() {
                        actions.push(Action::SelectTab(tab));
                    }
                }
                ui.separator();
                if self.busy() {
                    ui.add(egui::Spinner::new());
                    ui.label(format!("{}/{}", self.progress, USER_SLOTS));
                } else if self.connected {
                    if action_button(ui, format!("{} Refresh", icon::REVERT), ActionKind::Read)
                        .clicked()
                    {
                        actions.push(Action::Refresh);
                    }
                    let n = self.dirty_count();
                    let write = action_button(
                        ui,
                        format!("{} Write changes ({n})", icon::SAVE),
                        ActionKind::Commit,
                    );
                    if n > 0 && write.clicked() {
                        actions.push(Action::WriteAll);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Exit in the far-right corner (config is saved on close via on_exit).
                    if action_button(ui, "Exit", ActionKind::Neutral)
                        .on_hover_text("close the editor (staged changes are kept until written)")
                        .clicked()
                    {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if !self.connected && action_button(ui, "Connect", ActionKind::Read).clicked() {
                        actions.push(Action::Retry);
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Patches => self.show_patch_list(ui, &mut actions),
            Tab::Presets => self.show_presets(ui, &mut actions),
            Tab::Library => self.show_library(ui, &mut actions),
            Tab::Scene => self.show_scene(ui, &mut actions),
            Tab::Inspect => self.show_inspect(ui, &mut actions),
        });

        for action in actions {
            self.apply(action);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        config::save(&GuiConfig {
            zoom: self.zoom,
            window: self.window,
            tab: Some(self.tab.as_key().to_owned()),
            port: self.port.clone(),
        });
    }
}

impl App {
    /// The User-bank list: a row per slot with audition, name edit, and buttons.
    fn show_patch_list(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let writable = self.connected && !self.busy();
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("patches")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        for r in &self.rows {
                            self.patch_row(ui, r, writable, actions);
                            ui.end_row();
                        }
                    });
            });
    }

    fn patch_row(
        &self,
        ui: &mut egui::Ui,
        r: &PatchRow,
        writable: bool,
        actions: &mut Vec<Action>,
    ) {
        // Buttons (left), consistent order Save·Revert·Copy·Paste·Clear.
        ui.horizontal(|ui| {
            let dirty = r.dirty();
            if ui
                .add_enabled(writable && dirty, save_btn())
                .on_hover_text("Save this slot")
                .clicked()
            {
                actions.push(Action::SaveRow(r.slot));
            }
            if ui
                .add_enabled(dirty, revert_btn())
                .on_hover_text("Revert edits")
                .clicked()
            {
                actions.push(Action::RevertRow(r.slot));
            }
            if ui
                .add_enabled(writable, copy_btn())
                .on_hover_text("Copy this patch")
                .clicked()
            {
                actions.push(Action::CopyRow(r.slot));
            }
            if ui
                .add_enabled(self.clipboard.is_some(), paste_btn())
                .on_hover_text("Paste the copied patch here")
                .clicked()
            {
                actions.push(Action::PasteRow(r.slot));
            }
            if ui
                .add_enabled(r.pending.is_some(), clear_btn())
                .on_hover_text("Discard the staged patch")
                .clicked()
            {
                actions.push(Action::ClearRow(r.slot));
            }
        });

        // Slot id — click to audition.
        let mut label = RichText::new(slot_label(r.slot)).monospace();
        if self.now_playing == Some(r.slot) {
            label = label.strong().color(Color32::LIGHT_GREEN);
        }
        if r.failed {
            label = label.color(Color32::LIGHT_RED);
        }
        if ui
            .add_enabled(self.connected, egui::Button::new(label).frame(false))
            .on_hover_text("Audition")
            .clicked()
        {
            actions.push(Action::Audition(r.slot));
        }

        // Editable name (staged edit shown; a pending patch marks it modified).
        let mut name = r.name_edit.clone();
        let hint = if r.pending.is_some() {
            "‹pasted patch›"
        } else {
            ""
        };
        if ui
            .add(
                TextEdit::singleline(&mut name)
                    .desired_width(220.0)
                    .hint_text(hint),
            )
            .changed()
        {
            actions.push(Action::SetName(r.slot, name));
        }
    }
}

impl App {
    /// Read-only Factory-preset browser: audition, or copy one to the target User slot.
    fn show_presets(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if action_button(ui, format!("{} Load names", icon::LOAD), ActionKind::Read)
                .on_hover_text("Reads each factory slot's name (slow — also auditions each)")
                .clicked()
            {
                actions.push(Action::LoadPresets);
            }
            ui.separator();
            ui.label("Copy target:");
            target_slot_picker(ui, self.target_slot, actions);
        });
        ui.separator();
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("presets")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        for (slot, name) in &self.presets {
                            if ui
                                .add_enabled(self.connected, copy_btn())
                                .on_hover_text("Copy to the target User slot")
                                .clicked()
                            {
                                actions.push(Action::CopyFactoryToUser(*slot));
                            }
                            if ui
                                .add_enabled(
                                    self.connected,
                                    egui::Button::new(
                                        RichText::new(format!("F{slot:03}")).monospace(),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text("Audition")
                                .clicked()
                            {
                                actions.push(Action::AuditionFactory(*slot));
                            }
                            ui.label(if name.is_empty() {
                                "—"
                            } else {
                                name.as_str()
                            });
                            ui.end_row();
                        }
                    });
            });
    }

    /// On-disk library: save the current sound, import `.tfx`, load backups to a slot.
    fn show_library(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        if let Some(text) = &self.view_text {
            ui.horizontal(|ui| {
                ui.heading("Imported patch (JSON)");
                if action_button(ui, "Close", ActionKind::Neutral).clicked() {
                    actions.push(Action::CloseView);
                }
            });
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.monospace(text);
                });
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Save current as:");
            let mut n = self.save_name.clone();
            if ui
                .add(TextEdit::singleline(&mut n).desired_width(160.0))
                .changed()
            {
                actions.push(Action::SetSaveName(n));
            }
            if action_button(ui, format!("{} Save", icon::SAVE), ActionKind::Commit).clicked() {
                actions.push(Action::SaveCurrentAs);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Import .tfx:");
            let mut p = self.import_path.clone();
            if ui
                .add(
                    TextEdit::singleline(&mut p)
                        .desired_width(300.0)
                        .hint_text("path to file"),
                )
                .changed()
            {
                actions.push(Action::SetImportPath(p));
            }
            if action_button(ui, format!("{} Import", icon::LOAD), ActionKind::Read).clicked() {
                actions.push(Action::ImportTfx);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Load target:");
            target_slot_picker(ui, self.target_slot, actions);
            ui.separator();
            if action_button(ui, "Refresh list", ActionKind::Read).clicked() {
                actions.push(Action::RefreshLibrary);
            }
        });
        ui.separator();
        ui.columns(2, |cols| {
            let [left, right] = cols else { return };
            left.label(RichText::new("Backups — loadable").strong());
            for name in &self.lib_backups {
                left.horizontal(|ui| {
                    if action_button(ui, format!("{} Load", icon::LOAD), ActionKind::Commit)
                        .on_hover_text("Restore onto the target slot")
                        .clicked()
                    {
                        actions.push(Action::LoadBackup(name.clone()));
                    }
                    if action_button(ui, icon::DELETE, ActionKind::Destructive).clicked() {
                        actions.push(Action::DeleteBackup(name.clone()));
                    }
                    ui.label(name);
                });
            }
            right.label(RichText::new(".tfx imports — view only").strong());
            for name in &self.lib_imports {
                right.horizontal(|ui| {
                    if action_button(ui, format!("{} View", icon::EDIT), ActionKind::Read).clicked()
                    {
                        actions.push(Action::ViewImport(name.clone()));
                    }
                    if action_button(ui, icon::DELETE, ActionKind::Destructive).clicked() {
                        actions.push(Action::DeleteImport(name.clone()));
                    }
                    ui.label(name);
                });
            }
        });
    }

    /// Whole-bank scene composer: capture the bank, save/load, apply to the unit.
    fn show_scene(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.label("Scene:");
            let mut n = self.compose_name.clone();
            if ui
                .add(TextEdit::singleline(&mut n).desired_width(160.0))
                .changed()
            {
                actions.push(Action::SetSceneName(n));
            }
            if action_button(ui, "New", ActionKind::Neutral).clicked() {
                actions.push(Action::SceneNew);
            }
            if action_button(ui, format!("{} Capture bank", icon::LOAD), ActionKind::Read).clicked()
            {
                actions.push(Action::SceneCapture);
            }
            if action_button(ui, format!("{} Save", icon::SAVE), ActionKind::Commit).clicked() {
                actions.push(Action::SceneSave);
            }
            if action_button(ui, "Apply to unit", ActionKind::Commit).clicked() {
                actions.push(Action::SceneApply);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Saved:");
            for name in &self.lib_scenes {
                if ui
                    .button(name)
                    .on_hover_text("Load into the composer")
                    .clicked()
                {
                    actions.push(Action::SceneLoad(name.clone()));
                }
                if action_button(ui, icon::DELETE, ActionKind::Destructive)
                    .on_hover_text("Delete scene")
                    .clicked()
                {
                    actions.push(Action::SceneDelete(name.clone()));
                }
            }
        });
        ui.separator();
        let filled = self.compose.iter().filter(|c| c.is_some()).count();
        ui.label(format!("{filled} patches in this scene:"));
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("scene")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for (slot, c) in self.compose.iter().enumerate() {
                            let s = u8::try_from(slot).unwrap_or(0);
                            ui.monospace(slot_label(s));
                            ui.label(c.as_ref().map_or("—", |p| p.name.as_str()));
                            ui.end_row();
                        }
                    });
            });
    }

    /// Read-only patch inspector + live MIDI-CC quick-controls.
    fn show_inspect(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if action_button(
                ui,
                format!("{} Capture current", icon::LOAD),
                ActionKind::Read,
            )
            .clicked()
            {
                actions.push(Action::InspectCapture);
            }
        });
        ui.separator();
        if let Some(p) = &self.inspect {
            ui.label(RichText::new(format!("{}  ({} blocks)", p.name, p.blocks.len())).strong());
            ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("blocks")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for b in &p.blocks {
                                ui.monospace(format!("0x{:02X}", b.id));
                                ui.label(format!("{} bytes", b.bytes.len()));
                                ui.end_row();
                            }
                        });
                });
        } else {
            ui.label(RichText::new("Capture the current sound to inspect its blocks.").weak());
        }
        ui.separator();
        ui.label(
            RichText::new("Live MIDI-CC controls (host-set can't edit stored params)").strong(),
        );
        let bypasses: [(&str, &'static str); 5] = [
            ("Amp", "amp-bypass"),
            ("Dist", "dist-bypass"),
            ("Mod", "mod-bypass"),
            ("Delay", "delay-bypass"),
            ("Reverb", "reverb-bypass"),
        ];
        for (label, name) in bypasses {
            ui.horizontal(|ui| {
                ui.label(format!("{label}:"));
                if action_button(ui, "On", ActionKind::Neutral).clicked() {
                    actions.push(Action::SendCc(name, None, 127));
                }
                if action_button(ui, "Off", ActionKind::Neutral).clicked() {
                    actions.push(Action::SendCc(name, None, 0));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Amp output:");
            for v in [0u8, 32, 64, 96, 127] {
                if ui.button(format!("{v}")).clicked() {
                    actions.push(Action::SendCc("amp-output", None, v));
                }
            }
        });
    }
}

/// A `0..=127` slot picker that pushes [`Action::SetTargetSlot`] and shows the label.
fn target_slot_picker(ui: &mut egui::Ui, target: u8, actions: &mut Vec<Action>) {
    let mut t = target;
    if ui.add(DragValue::new(&mut t).range(0..=127)).changed() {
        actions.push(Action::SetTargetSlot(t));
    }
    ui.label(format!("({})", slot_label(target)));
}

fn save_btn() -> egui::Button<'static> {
    egui::Button::new(icon::SAVE)
}
fn revert_btn() -> egui::Button<'static> {
    egui::Button::new(icon::REVERT)
}
fn copy_btn() -> egui::Button<'static> {
    egui::Button::new(icon::COPY)
}
fn paste_btn() -> egui::Button<'static> {
    egui::Button::new(icon::PASTE)
}
fn clear_btn() -> egui::Button<'static> {
    egui::Button::new(icon::CLEAR)
}
