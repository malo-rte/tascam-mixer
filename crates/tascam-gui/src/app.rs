//! The eframe application shell: device ownership, control-state cache, layout.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use eframe::egui;
use tascam_us16x08::{
    Backend, Control, Kind, Meters, NUM_CHANNELS, Preset, Scope, Us16x08, Value, Watcher,
};

use crate::config::{self, GuiConfig};
use crate::{bridge, channel, output, routing};

/// Which editor the central panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Channel,
    Routing,
}

/// Meter repaint cadence (~30 Hz).
const METER_INTERVAL: Duration = Duration::from_millis(33);
/// How often to re-read controls so external changes (front panel, another
/// client) show up.
const WATCH_INTERVAL_SECS: f64 = 0.5;

/// The running mixer application. Owns the device on the UI thread.
pub(crate) struct App {
    device: Us16x08<Box<dyn Backend>>,
    source: &'static str,
    /// Last-known control values, fed by the watcher and by our own writes.
    cache: HashMap<(Control, u32), Value>,
    watcher: Watcher,
    meters: Meters,
    next_watch: f64,
    /// The channel shown in the editor.
    pub(crate) selected: u8,
    tab: Tab,
    /// Stereo-link state for the eight channel pairs (GUI-only).
    links: [bool; 8],
    status: String,
}

impl App {
    /// Build the app around an opened device and seed the control cache.
    pub(crate) fn new(device: Us16x08<Box<dyn Backend>>, mock: bool) -> Self {
        let mut app = Self {
            device,
            source: if mock {
                "mock device"
            } else {
                "US-16x08 (ALSA)"
            },
            cache: HashMap::new(),
            watcher: Watcher::new(),
            meters: Meters::default(),
            next_watch: 0.0,
            selected: 0,
            tab: Tab::Channel,
            links: config::load().links,
            status: String::new(),
        };
        app.sync_controls();
        app
    }

    /// Poll the watcher and fold any changes into the cache. The first call (an
    /// un-primed watcher) reports the whole control surface, seeding the cache.
    fn sync_controls(&mut self) {
        match self.watcher.poll(&self.device) {
            Ok(changes) => {
                for change in changes {
                    self.cache
                        .insert((change.control, change.index), change.value);
                }
            }
            Err(e) => self.status = format!("read error: {e}"),
        }
    }

    /// Cached boolean value (false if unknown).
    pub(crate) fn cached_bool(&self, control: Control, index: u32) -> bool {
        matches!(self.cache.get(&(control, index)), Some(Value::Bool(true)))
    }

    /// Cached integer/enum value (0 if unknown).
    pub(crate) fn cached_int(&self, control: Control, index: u32) -> i32 {
        match self.cache.get(&(control, index)) {
            Some(Value::Int(v) | Value::Enum(v)) => *v,
            _ => 0,
        }
    }

    /// Write a control to the device and update the cache. Per-channel controls
    /// on a linked pair are written to both channels.
    pub(crate) fn set(&mut self, control: Control, index: u32, value: Value) {
        self.write_one(control, index, value);
        if matches!(control.scope(), Scope::Channel) && self.linked(index) {
            self.write_one(control, index ^ 1, value);
        }
    }

    fn write_one(&mut self, control: Control, index: u32, value: Value) {
        match self.device.set(control, index, value) {
            Ok(()) => {
                self.cache.insert((control, index), value);
            }
            Err(e) => self.status = format!("write error ({}): {e}", control.cli_key()),
        }
    }

    /// Whether `channel`'s stereo pair is linked.
    pub(crate) fn linked(&self, channel: u32) -> bool {
        self.links
            .get((channel / 2) as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Move the focused channel one step, treating a linked pair as a single
    /// step and landing on the lower channel of a linked target pair.
    fn nav(&mut self, forward: bool) {
        let cur = u32::from(self.selected);
        let mut next = if forward {
            let base = if self.linked(cur) { cur | 1 } else { cur };
            (base + 1).min(NUM_CHANNELS - 1)
        } else {
            let base = if self.linked(cur) { cur & !1 } else { cur };
            base.saturating_sub(1)
        };
        if self.linked(next) {
            next &= !1;
        }
        self.selected = u8::try_from(next).unwrap_or(0);
    }

    /// Toggle the stereo link for `channel`'s pair, persisting the change. When
    /// enabling, copy the lower channel's settings to the upper one.
    pub(crate) fn toggle_link(&mut self, channel: u32) {
        let pair = (channel / 2) as usize;
        let Some(slot) = self.links.get_mut(pair) else {
            return;
        };
        *slot = !*slot;
        let now_linked = *slot;
        config::save(&GuiConfig { links: self.links });
        if now_linked {
            self.sync_pair(channel & !1);
        }
    }

    /// Copy every per-channel control from `low` to its partner `low + 1`.
    fn sync_pair(&mut self, low: u32) {
        for &control in Control::ALL {
            if matches!(control.scope(), Scope::Channel) && !matches!(control.kind(), Kind::Meter) {
                if let Some(&value) = self.cache.get(&(control, low)) {
                    self.write_one(control, low + 1, value);
                }
            }
        }
    }

    /// The latest meter snapshot.
    pub(crate) fn meters(&self) -> &Meters {
        &self.meters
    }

    /// Capture the whole mixer (or one channel's strip) to a JSON file.
    fn save_preset(&mut self, path: &Path, channel: Option<u32>) {
        let captured = match channel {
            Some(ch) => self.device.capture_strip(ch),
            None => self.device.capture_mixer(),
        };
        let result = captured
            .map_err(|e| e.to_string())
            .and_then(|preset| serde_json::to_string_pretty(&preset).map_err(|e| e.to_string()))
            .and_then(|json| std::fs::write(path, json).map_err(|e| e.to_string()));
        self.status = match result {
            Ok(()) => format!("saved {}", path.display()),
            Err(e) => format!("save failed: {e}"),
        };
    }

    /// Load a JSON preset. A strip preset needs a target `channel`; a mixer
    /// preset must be loaded with `None`.
    fn load_preset(&mut self, path: &Path, channel: Option<u32>) {
        let parsed = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Preset>(&text).map_err(|e| e.to_string()));
        match parsed {
            Ok(preset) => match self.device.apply(&preset, channel) {
                Ok(report) => {
                    self.status = format!(
                        "loaded {} ({} applied, {} skipped)",
                        path.display(),
                        report.applied,
                        report.skipped.len()
                    );
                    self.sync_controls();
                }
                Err(e) => self.status = format!("load failed: {e}"),
            },
            Err(e) => self.status = format!("load failed: {e}"),
        }
    }

    /// Tab selector and the Presets menu. (The global DSP switches live in the
    /// OUTPUT panel.)
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Channel, "Channel");
            ui.selectable_value(&mut self.tab, Tab::Routing, "Routing");
            ui.separator();
            self.presets_menu(ui);
        });
    }

    /// The Presets menu: save/load the whole mixer or the selected channel strip.
    fn presets_menu(&mut self, ui: &mut egui::Ui) {
        let channel = u32::from(self.selected);
        ui.menu_button("Presets", |ui| {
            if ui.button("Save mixer...").clicked() {
                ui.close_menu();
                if let Some(path) = save_dialog("mixer.json") {
                    self.save_preset(&path, None);
                }
            }
            if ui.button("Load mixer...").clicked() {
                ui.close_menu();
                if let Some(path) = open_dialog() {
                    self.load_preset(&path, None);
                }
            }
            ui.separator();
            if ui
                .button(format!("Save channel {} strip...", channel + 1))
                .clicked()
            {
                ui.close_menu();
                if let Some(path) = save_dialog("strip.json") {
                    self.save_preset(&path, Some(channel));
                }
            }
            if ui
                .button(format!("Load strip into channel {}...", channel + 1))
                .clicked()
            {
                ui.close_menu();
                if let Some(path) = open_dialog() {
                    self.load_preset(&path, Some(channel));
                }
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.device.meters() {
            Ok(m) => self.meters = m,
            Err(e) => self.status = format!("device error: {e}"),
        }
        let now = ctx.input(|i| i.time);
        if now >= self.next_watch {
            self.sync_controls();
            self.next_watch = now + WATCH_INTERVAL_SECS;
        }

        // Arrow keys step the focused channel, but only when no widget (e.g. a
        // slider) holds keyboard focus, so editing a value never moves channel.
        if ctx.memory(egui::Memory::focused).is_none() {
            let (mut prev, mut next) = (false, false);
            ctx.input(|i| {
                prev = i.key_pressed(egui::Key::ArrowLeft);
                next = i.key_pressed(egui::Key::ArrowRight);
            });
            if next {
                self.nav(true);
            } else if prev {
                self.nav(false);
            }
        }

        egui::TopBottomPanel::top("bridge").show(ctx, |ui| bridge::show(self, ui));
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("device: {}", self.source));
                if !self.status.is_empty() {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, &self.status);
                }
            });
        });

        egui::SidePanel::right("output").show(ctx, |ui| output::show(self, ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| match self.tab {
                Tab::Channel => channel::show(self, ui),
                Tab::Routing => routing::show(self, ui),
            });
        });

        ctx.request_repaint_after(METER_INTERVAL);
    }
}

/// Native "save file" dialog for a JSON preset.
fn save_dialog(default_name: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("JSON preset", &["json"])
        .set_file_name(default_name)
        .save_file()
}

/// Native "open file" dialog for a JSON preset.
fn open_dialog() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("JSON preset", &["json"])
        .pick_file()
}
