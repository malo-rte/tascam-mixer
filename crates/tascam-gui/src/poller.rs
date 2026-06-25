//! Background reader for the meters and the control surface.
//!
//! All the *continuous* device I/O (meters at ~30 Hz, a periodic re-read of the
//! ~280-control surface to catch front-panel/other-client changes) runs here, on
//! its own thread, so the UI thread -- which is also the Wayland event loop --
//! never blocks on USB for it and always answers the compositor's keep-alive
//! pings. User-initiated writes/loads still run on the UI thread, which shares
//! the device through the same mutex (locked briefly).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tascam_us16x08::{Backend, Control, Kind, Meters, Us16x08, Value};

/// The device, shared between the UI thread and the poller. Each access locks it
/// briefly.
pub(crate) type Device = Us16x08<Box<dyn Backend + Send>>;
pub(crate) type SharedDevice = Arc<Mutex<Device>>;

/// Meter cadence (~30 Hz): the loop sleeps this between iterations.
const METER_INTERVAL: Duration = Duration::from_millis(33);
/// Re-read the whole surface every this many meter ticks (~0.5 s at 30 Hz).
const WATCH_EVERY_TICKS: u32 = 15;

/// What the poller reports to the UI thread.
pub(crate) enum Report {
    /// A fresh meter snapshot.
    Meters(Meters),
    /// Controls whose value changed since the last sweep (also the full surface
    /// on the first sweep after (re)enabling).
    Changes(Vec<(Control, u32, Value)>),
    /// A meter read failed: the device has gone away. The poller pauses itself;
    /// the UI thread handles reconnect and re-enables it.
    Lost,
}

/// Lock the shared device, recovering from a poisoned mutex (a panic while
/// holding it) rather than propagating the panic.
pub(crate) fn lock(device: &SharedDevice) -> MutexGuard<'_, Device> {
    device
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The background poller: owns its thread and a channel of [`Report`]s.
pub(crate) struct Poller {
    running: Arc<AtomicBool>,
    enabled: Arc<AtomicBool>,
    rx: Receiver<Report>,
    handle: Option<JoinHandle<()>>,
}

impl Poller {
    /// Spawn the poller over `device`. It polls only while enabled; pass the
    /// initial state (true when the device is connected at startup).
    pub(crate) fn spawn(device: SharedDevice, enabled_initially: bool) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let enabled = Arc::new(AtomicBool::new(enabled_initially));
        let (tx, rx) = channel();
        let handle = {
            let (running, enabled) = (Arc::clone(&running), Arc::clone(&enabled));
            thread::spawn(move || run(&device, &running, &enabled, &tx))
        };
        Self {
            running,
            enabled,
            rx,
            handle: Some(handle),
        }
    }

    /// Resume or pause polling (the UI thread enables it on (re)connect and
    /// pauses it on disconnect, so it does not fight reconnect for the device).
    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Take all reports the poller has produced since the last call.
    pub(crate) fn drain(&self) -> Vec<Report> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The poller loop: meters at ~30 Hz, a surface sweep every `WATCH_EVERY_TICKS`,
/// while enabled. Timing is tick-based (no wall clock) to keep it deterministic.
fn run(device: &SharedDevice, running: &AtomicBool, enabled: &AtomicBool, tx: &Sender<Report>) {
    let mut snapshot: HashMap<(Control, u32), Value> = HashMap::new();
    let mut ticks: u32 = 0;
    while running.load(Ordering::Relaxed) {
        if !enabled.load(Ordering::Relaxed) {
            // Paused (disconnected): drop our snapshot so the next sweep after
            // re-enabling reports the whole surface afresh.
            snapshot.clear();
            ticks = 0;
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        let Ok(meters) = lock(device).meters() else {
            enabled.store(false, Ordering::Relaxed);
            let _ = tx.send(Report::Lost);
            continue;
        };
        if tx.send(Report::Meters(meters)).is_err() {
            return; // UI gone
        }
        ticks = ticks.wrapping_add(1);
        if ticks % WATCH_EVERY_TICKS == 0 {
            let changes = watch(device, &mut snapshot);
            if !changes.is_empty() && tx.send(Report::Changes(changes)).is_err() {
                return;
            }
        }
        thread::sleep(METER_INTERVAL);
    }
}

/// Read every present, non-meter control (locking per read, so UI writes can
/// interleave) and return those whose value changed since `snapshot`.
fn watch(
    device: &SharedDevice,
    snapshot: &mut HashMap<(Control, u32), Value>,
) -> Vec<(Control, u32, Value)> {
    let mut changes = Vec::new();
    for &control in Control::ALL {
        if matches!(control.kind(), Kind::Meter) {
            continue;
        }
        if !lock(device).is_present(control) {
            continue;
        }
        for index in 0..control.scope().count() {
            let Ok(value) = lock(device).get(control, index) else {
                continue;
            };
            let key = (control, index);
            if snapshot.get(&key) != Some(&value) {
                snapshot.insert(key, value);
                changes.push((control, index, value));
            }
        }
    }
    changes
}
