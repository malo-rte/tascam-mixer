//! The eframe application shell: shared device, control-state cache, layout.
//!
//! The continuous device reads (meters, surface watch) run on a background
//! [`crate::poller`] thread, so the UI thread (the Wayland event loop) never
//! blocks on USB for them. User-initiated writes and loads run here, locking the
//! shared device briefly; reconnect is handled here too.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use rackctl_us16x08::{Control, Kind, LoadTiming, Meters, NUM_CHANNELS, Preset, Scope, Value};

use crate::config::{self, GuiConfig};
use crate::poller::{self, Poller, Report, SharedDevice, lock};
use crate::{bridge, channel, output, preset_tab, routing};

/// Which editor the central panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Channel,
    Routing,
    Scenes,
    Strips,
    Eq,
    Comp,
}

/// The kinds of named, file-backed preset the GUI manages in a list tab. A
/// *scene* is the whole mixer; the others are applied to the focused channel: a
/// *strip* is the whole channel, while *EQ* and *compressor* are just that
/// section. They differ only in their directory and what they capture/apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PresetKind {
    /// A whole-mixer snapshot (the Scenes tab).
    Scene,
    /// One channel's strip, applied to the focused channel (the Strips tab).
    Strip,
    /// One channel's EQ section (the EQ presets tab).
    Eq,
    /// One channel's compressor section (the Comp presets tab).
    Comp,
}

impl PresetKind {
    /// The copy/paste clipboard a preset of this kind can be copied into, so it
    /// can be pasted onto a channel. `None` for a scene (a whole mixer is not a
    /// single channel).
    pub(crate) fn clipboard_group(self) -> Option<Group> {
        match self {
            PresetKind::Strip => Some(Group::Channel),
            PresetKind::Eq => Some(Group::Eq),
            PresetKind::Comp => Some(Group::Comp),
            PresetKind::Scene => None,
        }
    }
}

/// A group of per-channel controls that can be copied from one channel and
/// pasted onto another, via the in-app clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    /// The whole channel strip (phase, fader, pan, mute, EQ, compressor).
    Channel,
    /// The EQ section only (enable plus the four bands).
    Eq,
    /// The compressor section only (enable plus the parameters).
    Comp,
}

impl Group {
    /// A short noun for status messages.
    fn label(self) -> &'static str {
        match self {
            Group::Channel => "channel",
            Group::Eq => "EQ",
            Group::Comp => "compressor",
        }
    }
}

/// EQ controls a copy/paste carries (enable plus the four bands).
const EQ_GROUP: [Control; 11] = [
    Control::EqSwitch,
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

/// Compressor controls a copy/paste carries (enable plus the parameters).
const COMP_GROUP: [Control; 6] = [
    Control::CompSwitch,
    Control::CompThreshold,
    Control::CompRatio,
    Control::CompAttack,
    Control::CompRelease,
    Control::CompGain,
];

/// Meter repaint cadence (~30 Hz).
const METER_INTERVAL: Duration = Duration::from_millis(33);
/// How often to try reopening the device after it has gone away (e.g. the USB
/// interface was unplugged). Slow enough not to spin while it is absent.
const RECONNECT_INTERVAL_SECS: f64 = 1.0;
/// Timing for a full-mixer load. `pace` spaces the per-control writes so the
/// device's USB control channel is not overrun (sending ~280 back-to-back can
/// drop some). `rounds`/`settle` restart the sequence if a write errors; the
/// write-readiness gate means the first attempt usually succeeds.
const LOAD_TIMING: LoadTiming = LoadTiming {
    pace: Duration::from_millis(2),
    rounds: 6,
    settle: Duration::from_millis(50),
};
/// Pace between consecutive control writes in an in-editor bulk operation (solo,
/// paste, reset channel, stereo-link sync), so the burst does not outrun the
/// device's USB control channel and get silently dropped. Short -- these write
/// at most ~20 controls -- to stay responsive.
const WRITE_PACE: Duration = Duration::from_millis(2);

/// Sleep one [`WRITE_PACE`] between consecutive writes of a bulk operation.
fn pace_write() {
    if !WRITE_PACE.is_zero() {
        std::thread::sleep(WRITE_PACE);
    }
}

/// Reopens the device (real hardware or mock). Called to recover after the
/// device disappears, so the closure can outlive any one connection.
pub(crate) type Reopen = Box<dyn Fn() -> anyhow::Result<poller::Device>>;

/// The running mixer application. Shares the device with the background poller.
pub(crate) struct App {
    /// The device, shared with the poller thread; locked briefly for each access.
    device: SharedDevice,
    /// Background reader of meters and the control surface (off the UI thread).
    poller: Poller,
    source: &'static str,
    /// Reopen the backend after a disconnect (USB replug); see [`Reopen`].
    reopen: Reopen,
    /// Whether the device is currently reachable. When false, the app polls
    /// [`Self::try_reconnect`] instead of the device.
    connected: bool,
    /// Whether we have ever read a real surface into the cache. False until the
    /// first successful connection; it decides whether a (re)connect restores
    /// the cached mix or applies the startup defaults.
    primed: bool,
    /// Next time (seconds, eframe clock) to attempt a reconnect.
    next_reconnect: f64,
    /// Last-known control values, fed by the poller and by our own writes.
    cache: HashMap<(Control, u32), Value>,
    meters: Meters,
    /// The channel shown in the editor.
    pub(crate) selected: u8,
    tab: Tab,
    /// Stereo-link state for the eight channel pairs (GUI-only).
    links: [bool; 8],
    /// User-given names for the 16 input channels (GUI-only).
    names: [String; 16],
    /// Solo state for the 16 channels (GUI-only); while any is soloed, the
    /// non-soloed channels are muted.
    solo: [bool; 16],
    /// The channel mutes snapshotted when solo first engaged, restored when solo
    /// is cleared. `None` when no channel is soloed.
    pre_solo: Option<[bool; 16]>,
    /// Per-meter peak-hold / clip state (16 channels + master L/R).
    peaks: [bridge::PeakHold; bridge::NUM_METERS],
    /// Clock time of the last peak-hold update, for the decay step.
    meter_time: f64,
    /// Persisted interface zoom factor, saved with the default preset.
    zoom: f32,
    /// Persisted window inner size (logical points), saved with the default.
    window: Option<[f32; 2]>,
    /// Name being typed for a new preset, per preset tab.
    preset_names: HashMap<PresetKind, String>,
    /// A preset awaiting delete confirmation in a preset tab.
    pub(crate) pending_delete: Option<PathBuf>,
    /// Single copy/paste clipboard of per-channel control values. Each Copy
    /// replaces the part it handles (whole channel, or just the EQ / compressor
    /// section); each Paste applies the part it handles. So a copied channel can
    /// be tweaked section by section, then pasted onto many channels.
    clipboard: HashMap<Control, Value>,
    status: String,
}

impl App {
    /// Build the app around a device. `connected` says whether the card was
    /// actually open at startup (false means it was absent and `device` is a
    /// placeholder); `reopen` reconnects the same backend after a USB replug.
    pub(crate) fn new(device: poller::Device, mock: bool, connected: bool, reopen: Reopen) -> Self {
        let cfg = config::load();
        let device: SharedDevice = Arc::new(Mutex::new(device));
        let poller = Poller::spawn(Arc::clone(&device), connected);
        let mut app = Self {
            device,
            poller,
            source: if mock {
                "mock device"
            } else {
                "US-16x08 (ALSA)"
            },
            reopen,
            connected,
            primed: false,
            next_reconnect: 0.0,
            cache: HashMap::new(),
            meters: Meters::default(),
            selected: 0,
            tab: Tab::Channel,
            links: cfg.links,
            names: cfg.names,
            solo: [false; 16],
            pre_solo: None,
            peaks: [bridge::PeakHold::default(); bridge::NUM_METERS],
            meter_time: 0.0,
            zoom: cfg.zoom,
            window: cfg.window,
            preset_names: HashMap::new(),
            pending_delete: None,
            clipboard: HashMap::new(),
            status: String::new(),
        };
        if connected {
            // Card already running: read its current settings and use them. (The
            // poller keeps it current from here.)
            app.sync_controls();
            app.primed = true;
        } else {
            "device disconnected; waiting for it to return…".clone_into(&mut app.status);
        }
        app
    }

    /// Read the whole present control surface into the cache, locking per control
    /// so the poller is not blocked for the whole sweep. Used to seed the cache
    /// at startup and after applying a preset.
    fn sync_controls(&mut self) {
        for &control in Control::ALL {
            if matches!(control.kind(), Kind::Meter) {
                continue;
            }
            if !lock(&self.device).is_present(control) {
                continue;
            }
            for index in 0..control.scope().count() {
                if let Ok(value) = lock(&self.device).get(control, index) {
                    self.cache.insert((control, index), value);
                }
            }
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
        let result = lock(&self.device).set(control, index, value);
        match result {
            Ok(()) => {
                self.cache.insert((control, index), value);
            }
            Err(e) => self.status = format!("write error ({}): {e}", control.cli_key()),
        }
    }

    /// The common fader level and balance of a linked pair, recovered from the
    /// two channels' fader (`line-volume`) values. The pair stays panned hard
    /// L/R; balance is a level attenuation of one side, not a pan.
    pub(crate) fn pair_levels(&self, low: u32) -> (i32, i32) {
        let left = self.cached_int(Control::LineVolume, low);
        let right = self.cached_int(Control::LineVolume, low + 1);
        levels_to_balance(left, right)
    }

    /// Apply a common fader level and balance to a linked pair: the favoured
    /// side sits at `common`, the other is attenuated toward silence, so the
    /// stereo image (pan) is preserved and never needs gain above the fader top.
    pub(crate) fn set_pair_levels(&mut self, low: u32, common: i32, balance: i32) {
        let (left, right) = balance_to_levels(common, balance);
        self.write_one(Control::LineVolume, low, Value::Int(left));
        self.write_one(Control::LineVolume, low + 1, Value::Int(right));
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
        if now_linked {
            self.sync_pair(channel & !1);
        }
        // Persist after sync_pair, which may also share the name across the pair.
        self.save_config();
    }

    /// Persist the GUI-only state (stereo links, channel names, zoom, and window
    /// size).
    fn save_config(&self) {
        config::save(&GuiConfig {
            links: self.links,
            zoom: self.zoom,
            window: self.window,
            names: self.names.clone(),
        });
    }

    /// The user-given name for an input channel, or `""` if unset.
    pub(crate) fn channel_name(&self, channel: u32) -> &str {
        self.names.get(channel as usize).map_or("", String::as_str)
    }

    /// Set (and persist) the user-given name for an input channel. A linked pair
    /// shares one name, so the partner is updated too.
    pub(crate) fn set_channel_name(&mut self, channel: u32, name: &str) {
        self.assign_name(channel, name);
        if self.linked(channel) {
            self.assign_name(channel ^ 1, name);
        }
        self.save_config();
    }

    /// Store a channel name in memory (no persistence); see [`Self::save_config`].
    fn assign_name(&mut self, channel: u32, name: &str) {
        if let Some(slot) = self.names.get_mut(channel as usize) {
            name.clone_into(slot);
        }
    }

    /// Whether `channel` is soloed.
    pub(crate) fn soloed(&self, channel: u32) -> bool {
        self.solo.get(channel as usize).copied().unwrap_or(false)
    }

    /// Toggle solo on `channel` (and its linked partner). Engaging solo mutes the
    /// non-soloed channels; clearing the last solo restores the mutes that were
    /// in place when solo first engaged.
    pub(crate) fn toggle_solo(&mut self, channel: u32) {
        let on = !self.soloed(channel);
        self.set_solo(channel, on);
        if self.linked(channel) {
            self.set_solo(channel ^ 1, on);
        }
        self.apply_solo();
    }

    fn set_solo(&mut self, channel: u32, on: bool) {
        if let Some(slot) = self.solo.get_mut(channel as usize) {
            *slot = on;
        }
    }

    /// Push the mutes that the current solo state implies: when any channel is
    /// soloed, mute the rest (soloed channels keep their snapshotted mute); when
    /// no channel is soloed, restore the snapshot taken before solo engaged.
    fn apply_solo(&mut self) {
        let target = if self.solo.iter().any(|&s| s) {
            if self.pre_solo.is_none() {
                let snap: [bool; 16] = std::array::from_fn(|i| {
                    self.cached_bool(Control::MuteSwitch, u32::try_from(i).unwrap_or(0))
                });
                self.pre_solo = Some(snap);
            }
            match self.pre_solo {
                Some(pre) => solo_mutes(self.solo, pre),
                None => return,
            }
        } else {
            match self.pre_solo.take() {
                Some(pre) => pre,
                None => return,
            }
        };
        self.push_mutes(&target);
    }

    /// Write `target` to the channel mutes, but only where it differs from the
    /// current value, and paced like a load so the burst (up to 16 writes) does
    /// not outrun the device and get silently dropped.
    fn push_mutes(&mut self, target: &[bool; 16]) {
        for ch in 0..NUM_CHANNELS {
            let Some(&want) = target.get(ch as usize) else {
                continue;
            };
            if self.cached_bool(Control::MuteSwitch, ch) != want {
                self.write_one(Control::MuteSwitch, ch, Value::Bool(want));
                pace_write();
            }
        }
    }

    /// The persisted interface zoom factor, applied at startup.
    pub(crate) fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Copy every per-channel control from `low` to its partner `low + 1`, then
    /// pan the pair hard L/R for a stereo image. Pan is excluded from the copy:
    /// a linked pair is panned hard-opposite, not to the same position. The
    /// faders are copied (equal), so balance starts centred.
    fn sync_pair(&mut self, low: u32) {
        for &control in Control::ALL {
            if matches!(control.scope(), Scope::Channel)
                && !matches!(control.kind(), Kind::Meter)
                && control != Control::Pan
                && let Some(&value) = self.cache.get(&(control, low))
            {
                self.write_one(control, low + 1, value);
                pace_write();
            }
        }
        self.hard_pan_pair(low);
        // Share the lower channel's name across the newly linked pair.
        let name = self.channel_name(low).to_owned();
        self.assign_name(low + 1, &name);
    }

    /// Pan a linked pair hard left/right (lower channel left, upper right), the
    /// stereo image a link maintains. Re-asserted whenever something might have
    /// written a single pan onto both halves (e.g. pasting or loading a strip).
    fn hard_pan_pair(&mut self, low: u32) {
        self.write_one(Control::Pan, low, Value::Int(0));
        self.write_one(Control::Pan, low + 1, Value::Int(254));
    }

    /// The latest meter snapshot.
    pub(crate) fn meters(&self) -> &Meters {
        &self.meters
    }

    /// The peak-hold / clip state for a meter (channels `0..16`, master L `16`,
    /// master R `17`).
    pub(crate) fn peak(&self, index: u32) -> bridge::PeakHold {
        self.peaks.get(index as usize).copied().unwrap_or_default()
    }

    /// Fold the poller's reports into the UI state: meter snapshots (with the
    /// peak-hold step), surface changes (into the cache), and a lost device.
    fn drain_poller(&mut self, now: f64) {
        for report in self.poller.drain() {
            match report {
                Report::Meters(meters) => {
                    self.meters = meters;
                    bridge::observe_meters(
                        &mut self.peaks,
                        &self.meters,
                        now,
                        &mut self.meter_time,
                    );
                }
                Report::Changes(changes) => {
                    for (control, index, value) in changes {
                        self.cache.insert((control, index), value);
                    }
                }
                Report::Lost => self.mark_disconnected("device read failed"),
            }
        }
    }

    /// Record that the device has gone away and schedule an immediate reconnect
    /// attempt. The cached control values are kept so the mix can be restored.
    fn mark_disconnected(&mut self, err: &str) {
        self.connected = false;
        self.poller.set_enabled(false);
        self.next_reconnect = 0.0;
        self.meters = Meters::default();
        self.status = format!("device disconnected ({err}); reconnecting…");
    }

    /// Try to reopen the device. On success, resume polling and bring the card
    /// up to the right state: restore the cached mix if we had one (a replug
    /// while running), or apply the startup defaults if this is the first
    /// connection (the app started with no card).
    fn try_reconnect(&mut self) {
        let Ok(device) = (self.reopen)() else {
            "device disconnected; waiting for it to return…".clone_into(&mut self.status);
            return;
        };
        *lock(&self.device) = device;
        // Trust the handle only once a control *write* round-trips. A card that
        // has just re-enumerated answers reads while still silently dropping
        // writes; restoring the mix then does nothing and the device sits muted.
        // Stay disconnected and retry until writes actually land.
        if !lock(&self.device).accepts_writes() {
            self.connected = false;
            "device returning…".clone_into(&mut self.status);
            return;
        }
        self.connected = true;
        if self.primed {
            self.restore_mix();
        } else {
            self.apply_startup_defaults();
            self.primed = true;
            "device reconnected".clone_into(&mut self.status);
        }
        // Resume background polling now that the device is back and seeded.
        self.poller.set_enabled(true);
    }

    /// Push the cached mix back to a just-reconnected device as one transaction:
    /// master muted throughout, every write verified, restarting from the top on
    /// any failure. Re-seed the cache afterwards so it matches the device exactly.
    fn restore_mix(&mut self) {
        let values: Vec<(Control, u32, Value)> =
            self.cache.iter().map(|(&(c, i), &v)| (c, i, v)).collect();
        let ok = lock(&self.device).restore_values_muted(&values, LOAD_TIMING);
        self.sync_controls();
        self.status = if ok {
            "device reconnected".to_owned()
        } else {
            "reconnected, but the mix could not be fully restored".to_owned()
        };
    }

    /// First connection when the app started with no card: there is nothing to
    /// restore, so apply the shared default preset (if one is saved) and then
    /// seed the cache from whatever the device now reports. With no default
    /// preset saved, just read the device's current settings.
    fn apply_startup_defaults(&mut self) {
        if let Some(path) = config::default_preset_path()
            && path.exists()
        {
            self.load_preset(&path, None);
        }
        // Always seed the cache from the device, whether a default was applied,
        // missing, or failed to parse, so later restores have real values.
        self.sync_controls();
    }

    /// Capture the whole mixer (or one channel's strip) to a JSON file.
    fn save_preset(&mut self, path: &Path, channel: Option<u32>) {
        let result = self
            .build_preset_json(channel)
            .and_then(|json| std::fs::write(path, json).map_err(|e| e.to_string()));
        self.status = match result {
            Ok(()) => format!("saved {}", path.display()),
            Err(e) => format!("save failed: {e}"),
        };
    }

    /// Capture a preset and serialise it. A whole-mixer preset also carries the
    /// GUI-only stereo-link grouping as an extra `links` field, so loading it
    /// restores the grouping. The library and CLI do not use that field (serde
    /// ignores it), keeping the file a valid hardware preset.
    fn build_preset_json(&self, channel: Option<u32>) -> Result<String, String> {
        let preset = match channel {
            Some(ch) => lock(&self.device).capture_strip(ch),
            None => lock(&self.device).capture_mixer(),
        }
        .map_err(|e| e.to_string())?;
        let mut value = serde_json::to_value(&preset).map_err(|e| e.to_string())?;
        if channel.is_none()
            && let Some(object) = value.as_object_mut()
        {
            let links = serde_json::to_value(self.links).map_err(|e| e.to_string())?;
            object.insert("links".to_owned(), links);
        }
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }

    /// Load a JSON preset. A strip preset needs a target `channel`; a mixer
    /// preset must be loaded with `None`.
    fn load_preset(&mut self, path: &Path, channel: Option<u32>) {
        let parsed = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                serde_json::from_str::<serde_json::Value>(&text).map_err(|e| e.to_string())
            });
        let value = match parsed {
            Ok(value) => value,
            Err(e) => {
                self.status = format!("load failed: {e}");
                return;
            }
        };
        // Restore the stereo-link grouping a whole-mixer preset carries, before
        // applying, so the UI interprets the restored faders/pans the same way.
        if channel.is_none()
            && let Some(links) = extract_links(&value)
        {
            self.links = links;
            self.save_config();
        }
        let preset: Preset = match serde_json::from_value(value) {
            Ok(preset) => preset,
            Err(e) => {
                self.status = format!("load failed: {e}");
                return;
            }
        };
        // A per-channel preset loaded onto a linked channel applies to both
        // halves of the pair, so the link stays consistent.
        let pair_low = match channel {
            Some(ch) if self.linked(ch) => Some(ch & !1),
            _ => None,
        };
        let targets: Vec<Option<u32>> = match pair_low {
            Some(low) => vec![Some(low), Some(low + 1)],
            None => vec![channel],
        };
        // Mute the master and verify each write, so a full load is silent while
        // it applies and survives a device that is still settling.
        let mut applied = 0;
        let mut skipped = 0;
        let mut error = None;
        for target in targets {
            let outcome = lock(&self.device).apply_muted(&preset, target, LOAD_TIMING);
            match outcome {
                Ok(report) => {
                    applied = report.applied;
                    skipped = report.skipped.len();
                }
                Err(e) => {
                    error = Some(e.to_string());
                    break;
                }
            }
        }
        if let Some(e) = error {
            self.status = format!("load failed: {e}");
            return;
        }
        // Keep a linked pair hard-panned: a strip preset carries one channel's
        // pan, which would otherwise collapse the stereo image onto one side.
        if let Some(low) = pair_low {
            self.hard_pan_pair(low);
        }
        self.sync_controls();
        self.status = format!(
            "loaded {} ({} applied, {} skipped)",
            path.display(),
            applied,
            skipped
        );
    }

    /// Save the whole mixer as the shared default preset (read back by the CLI
    /// `default` command and by `Load default`), and remember the current zoom
    /// and window size as part of the saved setup.
    pub(crate) fn save_default(&mut self, zoom: f32, window: [f32; 2]) {
        self.zoom = zoom;
        self.window = Some(window);
        self.save_config();
        match config::default_preset_path() {
            Some(path) => self.save_preset(&path, None),
            None => "save failed: no config directory".clone_into(&mut self.status),
        }
    }

    /// Load the shared default preset into the whole mixer and return the
    /// persisted zoom factor and window size for the caller to apply.
    pub(crate) fn load_default(&mut self) -> (f32, Option<[f32; 2]>) {
        let cfg = config::load();
        self.zoom = cfg.zoom;
        self.window = cfg.window;
        // The stereo-link grouping is part of the saved setup too, so the UI
        // interprets the restored faders/pans the same way they were saved.
        self.links = cfg.links;
        self.names = cfg.names;
        match config::default_preset_path() {
            Some(path) => self.load_preset(&path, None),
            None => "load failed: no config directory".clone_into(&mut self.status),
        }
        (self.zoom, self.window)
    }

    /// The text buffer for the new-preset name field of `kind`'s tab.
    pub(crate) fn preset_name_mut(&mut self, kind: PresetKind) -> &mut String {
        self.preset_names.entry(kind).or_default()
    }

    /// The directory holding `kind`'s preset files.
    pub(crate) fn preset_dir(kind: PresetKind) -> Option<PathBuf> {
        match kind {
            PresetKind::Scene => config::scenes_dir(),
            PresetKind::Strip => config::strips_dir(),
            PresetKind::Eq => config::eq_dir(),
            PresetKind::Comp => config::comp_dir(),
        }
    }

    /// Which channel a `kind` preset applies to: the whole mixer (`None`) for a
    /// scene, or the focused channel for the per-channel kinds.
    fn preset_channel(&self, kind: PresetKind) -> Option<u32> {
        match kind {
            PresetKind::Scene => None,
            PresetKind::Strip | PresetKind::Eq | PresetKind::Comp => Some(u32::from(self.selected)),
        }
    }

    /// Save the current state as a preset named `name` in `kind`'s directory. The
    /// name is sanitised into a file name; an existing preset of that name is
    /// overwritten.
    pub(crate) fn save_named_preset(&mut self, kind: PresetKind, name: &str) {
        let Some(dir) = Self::preset_dir(kind) else {
            "save failed: no config directory".clone_into(&mut self.status);
            return;
        };
        let file = sanitize_name(name);
        if file.is_empty() {
            "save failed: empty preset name".clone_into(&mut self.status);
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.status = format!("save failed: {e}");
            return;
        }
        let path = dir.join(format!("{file}.json"));
        self.write_named(kind, &path);
    }

    /// Load a saved preset of `kind` from `path` (a scene, full strip, or a
    /// partial strip holding only the EQ or compressor controls).
    pub(crate) fn load_named_preset(&mut self, kind: PresetKind, path: &Path) {
        self.load_preset(path, self.preset_channel(kind));
    }

    /// Overwrite an existing preset file of `kind` with the current state.
    pub(crate) fn update_named_preset(&mut self, kind: PresetKind, path: &Path) {
        self.write_named(kind, path);
    }

    /// Copy a saved preset of `kind` into the copy/paste clipboard, so it can be
    /// pasted onto channels. A channel/strip preset replaces the clipboard; an EQ
    /// or compressor preset overlays just that section. No-op for a scene.
    pub(crate) fn copy_preset(&mut self, kind: PresetKind, path: &Path) {
        let Some(group) = kind.clipboard_group() else {
            return;
        };
        let values = match read_strip_values(path) {
            Ok(values) => values,
            Err(e) => {
                self.status = format!("copy failed: {e}");
                return;
            }
        };
        let allowed = group_controls(group);
        if matches!(group, Group::Channel) {
            self.clipboard.clear();
        }
        for (control, value) in values {
            if allowed.contains(&control) {
                self.clipboard.insert(control, value);
            }
        }
        self.status = format!("copied {} from {}", group.label(), preset_label(path));
    }

    /// Write a preset of `kind` to `path`: a scene or full strip via
    /// [`Self::save_preset`]; an EQ or compressor preset as a strip holding only
    /// that section's controls.
    fn write_named(&mut self, kind: PresetKind, path: &Path) {
        match kind {
            PresetKind::Scene | PresetKind::Strip => {
                self.save_preset(path, self.preset_channel(kind));
            }
            PresetKind::Eq => self.save_group_preset(path, Group::Eq),
            PresetKind::Comp => self.save_group_preset(path, Group::Comp),
        }
    }

    /// Capture the focused channel's strip, keep only `group`'s controls, and
    /// write it as a (partial) strip preset. Loading it applies just that section.
    fn save_group_preset(&mut self, path: &Path, group: Group) {
        let ch = u32::from(self.selected);
        let keys: std::collections::HashSet<&str> =
            group_controls(group).iter().map(|c| c.cli_key()).collect();
        let result = lock(&self.device)
            .capture_strip(ch)
            .map_err(|e| e.to_string())
            .and_then(|preset| {
                let Preset::Strip { version, controls } = preset else {
                    return Err("expected a strip preset".to_owned());
                };
                let controls = controls
                    .into_iter()
                    .filter(|(key, _)| keys.contains(key.as_str()))
                    .collect();
                let filtered = Preset::Strip { version, controls };
                serde_json::to_string_pretty(&filtered).map_err(|e| e.to_string())
            })
            .and_then(|json| std::fs::write(path, json).map_err(|e| e.to_string()));
        self.status = match result {
            Ok(()) => format!("saved {}", path.display()),
            Err(e) => format!("save failed: {e}"),
        };
    }

    /// Delete a saved preset file.
    pub(crate) fn delete_preset(&mut self, path: &Path) {
        self.status = match std::fs::remove_file(path) {
            Ok(()) => format!("deleted {}", preset_label(path)),
            Err(e) => format!("delete failed: {e}"),
        };
    }

    /// Whether the clipboard holds anything `group` can paste -- i.e. at least
    /// one of `group`'s controls was copied. The Paste button is disabled
    /// otherwise, so a never-copied section is never pasted.
    pub(crate) fn has_clip(&self, group: Group) -> bool {
        group_controls(group)
            .iter()
            .any(|control| self.clipboard.contains_key(control))
    }

    /// Copy `group`'s controls from the focused channel into the clipboard.
    /// Copying the whole channel replaces the clipboard; copying a section
    /// overlays just that part, leaving the rest of a previous copy intact.
    pub(crate) fn copy_group(&mut self, group: Group) {
        let ch = u32::from(self.selected);
        if matches!(group, Group::Channel) {
            self.clipboard.clear();
        }
        for control in group_controls(group) {
            if lock(&self.device).is_present(control)
                && let Some(&value) = self.cache.get(&(control, ch))
            {
                self.clipboard.insert(control, value);
            }
        }
        self.status = format!("copied {} from channel {}", group.label(), ch + 1);
    }

    /// Paste the part of the clipboard `group` handles onto the focused channel
    /// (and its pair, when linked, via [`Self::set`]). Only controls actually in
    /// the clipboard are written -- never-copied controls are left untouched, so
    /// nothing uninitialised is pasted.
    pub(crate) fn paste_group(&mut self, group: Group) {
        let ch = u32::from(self.selected);
        let values: Vec<(Control, Value)> = group_controls(group)
            .into_iter()
            .filter_map(|control| self.clipboard.get(&control).map(|&value| (control, value)))
            .collect();
        if values.is_empty() {
            return;
        }
        for (control, value) in values {
            self.set(control, ch, value);
            pace_write();
        }
        // `set` mirrors each control onto the linked partner; re-assert the
        // hard-pan so a pasted channel's pan does not collapse the pair.
        if self.linked(ch) {
            self.hard_pan_pair(ch & !1);
        }
        self.status = format!("pasted {} to channel {}", group.label(), ch + 1);
    }

    /// Reset the whole focused channel to a neutral default: every per-channel
    /// control to its catalog default, the switches (phase, mute, EQ and
    /// compressor enables) off, and the fader fully down (-127 dB) so a reset
    /// channel starts silent rather than at the default level. Flat EQ, no
    /// compression, centre pan. Applies to the linked pair too, via [`Self::set`].
    pub(crate) fn reset_channel(&mut self) {
        let ch = u32::from(self.selected);
        for &control in Control::ALL {
            if control.scope() != Scope::Channel || !lock(&self.device).is_present(control) {
                continue;
            }
            let value = match control.kind() {
                // The fader resets fully down, not to its default level.
                Kind::Int { min, .. } if control == Control::LineVolume => Value::Int(min),
                Kind::Int { default, .. } => Value::Int(default),
                Kind::Enum { default, .. } => Value::Enum(default),
                Kind::Bool => Value::Bool(false),
                // Meters are not settable, and any future kind is skipped.
                _ => continue,
            };
            self.set(control, ch, value);
            pace_write();
        }
        self.status = format!("reset channel {}", ch + 1);
    }

    /// Tab selector. (Presets live in the Scenes and Channel-presets tabs; the
    /// global DSP switches live in the OUTPUT panel.)
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Channel, "Channel");
            ui.selectable_value(&mut self.tab, Tab::Routing, "Routing");
            ui.selectable_value(&mut self.tab, Tab::Scenes, "Scenes");
            ui.selectable_value(&mut self.tab, Tab::Strips, "Channel presets");
            ui.selectable_value(&mut self.tab, Tab::Eq, "EQ presets");
            ui.selectable_value(&mut self.tab, Tab::Comp, "Comp presets");
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        // The poller (background thread) does the continuous device reads; fold
        // its reports in. Reconnect, when disconnected, is handled here.
        self.drain_poller(now);
        if !self.connected && now >= self.next_reconnect {
            self.try_reconnect();
            self.next_reconnect = now + RECONNECT_INTERVAL_SECS;
        }

        // Keyboard shortcuts, only when no widget (e.g. a slider) holds keyboard
        // focus, so editing a value never triggers them. Esc/Q quit; the arrow
        // keys step the focused channel; `m` mutes the selected channel and `M`
        // (Shift+m) mutes the master.
        if ctx.memory(egui::Memory::focused).is_none() {
            let (mut prev, mut next, mut quit) = (false, false, false);
            let (mut mute_channel, mut mute_master, mut solo_channel) = (false, false, false);
            ctx.input(|i| {
                prev = i.key_pressed(egui::Key::ArrowLeft);
                next = i.key_pressed(egui::Key::ArrowRight);
                quit = i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Q);
                if i.key_pressed(egui::Key::M) {
                    if i.modifiers.shift {
                        mute_master = true;
                    } else {
                        mute_channel = true;
                    }
                }
                solo_channel = i.key_pressed(egui::Key::S);
            });
            if quit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if next {
                self.nav(true);
            } else if prev {
                self.nav(false);
            }
            // Mute shortcuts write to the device, so ignore them while it is
            // gone; quit and channel navigation stay available.
            if mute_channel && self.connected {
                let ch = u32::from(self.selected);
                let muted = self.cached_bool(Control::MuteSwitch, ch);
                self.set(Control::MuteSwitch, ch, Value::Bool(!muted));
            }
            if mute_master && self.connected {
                let muted = self.cached_bool(Control::MasterMute, 0);
                self.set(Control::MasterMute, 0, Value::Bool(!muted));
            }
            if solo_channel && self.connected {
                self.toggle_solo(u32::from(self.selected));
            }
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("device: {}", self.source));
                if !self.status.is_empty() {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, &self.status);
                }
            });
        });

        if self.connected {
            // The mixer surface: bridge, toolbar, output, and the active tab.
            egui::TopBottomPanel::top("bridge").show(ctx, |ui| bridge::show(self, ui));
            egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
            egui::SidePanel::right("output").show(ctx, |ui| output::show(self, ui));
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::both().show(ui, |ui| match self.tab {
                    Tab::Channel => channel::show(self, ui),
                    Tab::Routing => routing::show(self, ui),
                    Tab::Scenes => preset_tab::show(self, ui, PresetKind::Scene),
                    Tab::Strips => preset_tab::show(self, ui, PresetKind::Strip),
                    Tab::Eq => preset_tab::show(self, ui, PresetKind::Eq),
                    Tab::Comp => preset_tab::show(self, ui, PresetKind::Comp),
                });
            });
        } else {
            // Hide the controls (they would drive a dead handle) and show only a
            // centred notice until the device comes back.
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("Tascam US-16x08 is disconnected")
                            .size(28.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });
        }

        ctx.request_repaint_after(METER_INTERVAL);
    }
}

/// The `line-volume` raw maximum (channel fader top); see the control catalog.
const LINE_VOLUME_MAX: i32 = 133;

/// Attenuate a fader value by a balance fraction `f` (0..=1), in the amplitude
/// domain: the gain is reduced to `(1 - f)` of `common`. The fader is roughly
/// linear in dB (1 raw step per dB), so a raw delta of `20*log10(1-f)` applies
/// that gain. This is gentle near the centre (about -6 dB at half) and only
/// reaches silence at `f == 1`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn attenuate(common: i32, f: f64) -> i32 {
    if f <= 0.0 {
        return common;
    }
    if f >= 1.0 {
        return 0;
    }
    let db = 20.0 * (1.0 - f).log10(); // <= 0
    (f64::from(common) + db)
        .round()
        .clamp(0.0, f64::from(common)) as i32
}

/// Map a common fader level (`line-volume` raw) and a balance setting (0..=254,
/// 127 = centred) to the lower and upper channel fader values. The favoured side
/// stays at `common`; the other is attenuated toward silence. Balance 0 favours
/// the left (lower) channel, 254 the right (upper); pan stays hard L/R.
fn balance_to_levels(common: i32, balance: i32) -> (i32, i32) {
    let common = common.clamp(0, LINE_VOLUME_MAX);
    let balance = balance.clamp(0, 254);
    if balance >= 127 {
        // Favour right: attenuate the lower (left) channel.
        (attenuate(common, f64::from(balance - 127) / 127.0), common)
    } else {
        // Favour left: attenuate the upper (right) channel.
        (common, attenuate(common, f64::from(127 - balance) / 127.0))
    }
}

/// Inverse of [`balance_to_levels`]: recover `(common, balance)` from a linked
/// pair's two fader values. The louder side is the common level; the quieter
/// side's attenuation (read back in the amplitude domain) gives the balance.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn levels_to_balance(left: i32, right: i32) -> (i32, i32) {
    let common = left.max(right);
    if common <= 0 {
        return (0, 127);
    }
    let att = left.min(right);
    // Recover f from the gain ratio (1 - f) = 10^((att - common) / 20).
    let amp = 10f64.powf(f64::from(att - common) / 20.0);
    let offset = ((1.0 - amp).clamp(0.0, 1.0) * 127.0).round() as i32;
    let balance = match left.cmp(&right) {
        std::cmp::Ordering::Less => 127 + offset, // left attenuated -> favour right
        std::cmp::Ordering::Greater => 127 - offset,
        std::cmp::Ordering::Equal => 127,
    };
    (common, balance.clamp(0, 254))
}

/// The saved presets of `kind` in its directory, sorted by name. Each is a
/// `*.json` file saved from the matching tab.
pub(crate) fn preset_paths(kind: PresetKind) -> Vec<PathBuf> {
    let Some(dir) = App::preset_dir(kind) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
}

/// Read the GUI-only stereo-link grouping from a whole-mixer preset's optional
/// `links` field. `None` if the field is absent or malformed (e.g. a preset
/// saved by the CLI), in which case the current grouping is kept.
fn extract_links(value: &serde_json::Value) -> Option<[bool; 8]> {
    let array = value.get("links")?.as_array()?;
    if array.len() != 8 {
        return None;
    }
    let mut links = [false; 8];
    for (slot, item) in links.iter_mut().zip(array) {
        *slot = item.as_bool()?;
    }
    Some(links)
}

/// Read a strip preset file and resolve it to `(control, value)` pairs for the
/// paste clipboard. (EQ / compressor presets are partial strips.)
fn read_strip_values(path: &Path) -> Result<Vec<(Control, Value)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let preset: Preset = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(preset.strip_values())
}

/// The channel mutes implied by a solo state: a soloed channel keeps its
/// snapshotted mute `pre`, every other channel is muted. With nothing soloed the
/// result equals `pre`.
fn solo_mutes(solo: [bool; 16], pre: [bool; 16]) -> [bool; 16] {
    let any = solo.iter().any(|&s| s);
    let mut out = [false; 16];
    for ((slot, soloed), &was) in out.iter_mut().zip(solo).zip(pre.iter()) {
        *slot = if !any || soloed { was } else { true };
    }
    out
}

/// The per-channel controls a copy/paste of `group` carries.
fn group_controls(group: Group) -> Vec<Control> {
    match group {
        Group::Eq => EQ_GROUP.to_vec(),
        Group::Comp => COMP_GROUP.to_vec(),
        // The whole strip: every per-channel, non-meter control.
        Group::Channel => Control::ALL
            .iter()
            .copied()
            .filter(|c| c.scope() == Scope::Channel && !matches!(c.kind(), Kind::Meter))
            .collect(),
    }
}

/// The display name of a preset file: its stem without the `.json` extension.
pub(crate) fn preset_label(path: &Path) -> String {
    path.file_stem().map_or_else(
        || path.display().to_string(),
        |s| s.to_string_lossy().into_owned(),
    )
}

/// Turn a user-typed preset name into a safe file stem: keep letters, digits,
/// spaces, dashes and underscores; replace anything else (path separators,
/// dots) with an underscore; trim surrounding whitespace.
fn sanitize_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{balance_to_levels, extract_links, levels_to_balance, solo_mutes};

    #[test]
    #[allow(clippy::indexing_slicing)]
    fn solo_mutes_silences_the_unsoloed() {
        let mut solo = [false; 16];
        solo[2] = true;
        solo[5] = true;
        // Nothing muted beforehand.
        let mutes = solo_mutes(solo, [false; 16]);
        assert!(!mutes[2] && !mutes[5], "soloed channels stay unmuted");
        assert!(
            mutes[0] && mutes[1] && mutes[3] && mutes[15],
            "others muted"
        );

        // With nothing soloed, the result is exactly the snapshot.
        let mut pre = [false; 16];
        pre[0] = true;
        assert_eq!(solo_mutes([false; 16], pre), pre);

        // A soloed channel that was muted in the snapshot stays muted.
        let mut solo2 = [false; 16];
        solo2[0] = true;
        assert!(
            solo_mutes(solo2, pre)[0],
            "soloed but pre-muted stays muted"
        );
    }

    #[test]
    fn links_round_trip_through_preset_json() {
        let links = [true, false, false, true, false, true, false, false];
        // A whole-mixer preset object with the GUI's extra `links` field.
        let value = serde_json::json!({
            "kind": "mixer",
            "version": 1,
            "master": {},
            "route": {},
            "channels": [],
            "links": links,
        });

        // The grouping is recovered, and the library still parses the preset
        // (the extra field is ignored).
        assert_eq!(extract_links(&value), Some(links));
        assert!(serde_json::from_value::<rackctl_us16x08::Preset>(value).is_ok());
    }

    #[test]
    fn missing_or_malformed_links_keep_current_grouping() {
        assert_eq!(extract_links(&serde_json::json!({"kind": "mixer"})), None);
        assert_eq!(
            extract_links(&serde_json::json!({"links": [true, false]})),
            None
        );
        assert_eq!(extract_links(&serde_json::json!({"links": "nope"})), None);
    }

    #[test]
    fn centre_balance_keeps_both_at_common() {
        assert_eq!(balance_to_levels(100, 127), (100, 100));
    }

    #[test]
    fn extreme_balance_silences_one_side() {
        // Full right favours the right (upper) channel: left fader to silence.
        assert_eq!(balance_to_levels(120, 254), (0, 120));
        // Full left silences the right (upper) channel.
        assert_eq!(balance_to_levels(120, 0), (120, 0));
    }

    #[test]
    fn half_balance_is_gentle() {
        // Half-right is about -6 dB on the left (amplitude halved), not silence;
        // the right stays at the common level.
        let (left, right) = balance_to_levels(100, 190);
        assert_eq!(right, 100);
        assert!((90..=98).contains(&left), "left = {left}");
    }

    #[test]
    fn levels_round_trip_is_stable() {
        for common in [1, 40, 100, 133] {
            for balance in 0..=254 {
                let (left, right) = balance_to_levels(common, balance);
                let (rc, rb) = levels_to_balance(left, right);
                assert_eq!(rc, common, "common {common} balance {balance}");
                // The recovered balance re-applies to within one step, so the
                // slider reading does not drift between frames.
                let (l2, r2) = balance_to_levels(rc, rb);
                assert!(
                    (l2 - left).abs() <= 1 && (r2 - right).abs() <= 1,
                    "balance {balance}: ({left},{right}) -> b{rb} -> ({l2},{r2})"
                );
            }
        }
    }
}
