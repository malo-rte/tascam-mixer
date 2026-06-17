//! The eframe application shell: device ownership, control-state cache, layout.

use std::collections::HashMap;
use std::time::Duration;

use eframe::egui;
use tascam_us16x08::{Backend, Control, Meters, Us16x08, Value, Watcher};

use crate::{bridge, channel};

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

    /// Write a control to the device and update the cache, recording any error.
    pub(crate) fn set(&mut self, control: Control, index: u32, value: Value) {
        match self.device.set(control, index, value) {
            Ok(()) => {
                self.cache.insert((control, index), value);
            }
            Err(e) => self.status = format!("write error ({}): {e}", control.cli_key()),
        }
    }

    /// The latest meter snapshot.
    pub(crate) fn meters(&self) -> &Meters {
        &self.meters
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(m) = self.device.meters() {
            self.meters = m;
        }
        let now = ctx.input(|i| i.time);
        if now >= self.next_watch {
            self.sync_controls();
            self.next_watch = now + WATCH_INTERVAL_SECS;
        }

        egui::TopBottomPanel::top("bridge").show(ctx, |ui| bridge::show(self, ui));

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("device: {}", self.source));
                if !self.status.is_empty() {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, &self.status);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| channel::show(self, ui));
        });

        ctx.request_repaint_after(METER_INTERVAL);
    }
}
