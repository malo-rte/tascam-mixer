//! Background bank reader: reads the 100 user patch headers slot-by-slot off the
//! UI thread, so the window keeps answering the compositor while the (slow) bank
//! read runs. Each `read_patch_header` makes the device stream a whole patch
//! (~0.5 s), so a full bank is ~1 minute; results stream back as they arrive.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rackctl_gx700::PatchHeader;

use crate::device::{SharedDevice, lock};

/// Number of user patch slots a bank load covers (slots `1..=100`).
pub(crate) const USER_SLOTS: u16 = 100;

/// First and last factory-preset slot (P001..P100), and how many there are.
pub(crate) const PRESET_START: u16 = 101;
pub(crate) const PRESET_END: u16 = 200;
pub(crate) const PRESET_SLOTS: u16 = PRESET_END - PRESET_START + 1;

/// Gap left between slot reads, to avoid overrunning the USB-MIDI interface (the
/// same pacing the CLI's `patches` uses).
const READ_PACE: Duration = Duration::from_millis(40);

/// How many times to read a slot before giving up and skipping it. A single
/// dropped device reply shouldn't abort the whole bank load.
const READ_ATTEMPTS: u32 = 3;

/// If this many slots fail back-to-back the device is clearly not answering at
/// all (off, unplugged, wrong port, in BULK LOAD, or the port is in use), so the
/// load aborts with one clear message instead of grinding through all 100 slots.
const ABORT_AFTER_CONSECUTIVE: u16 = 3;

/// A result from the loader.
pub(crate) enum Loaded {
    /// One slot's header arrived.
    Header(u16, PatchHeader),
    /// A slot read failed (slot, message); the load continues with the next.
    Failed(u16, String),
    /// The load gave up early because the device stopped answering (message).
    Aborted(String),
    /// The whole bank finished (or the load was cancelled).
    Done,
}

/// A running bank load. Dropping it cancels the read and joins the thread.
pub(crate) struct Loader {
    cancel: Arc<AtomicBool>,
    rx: Receiver<Loaded>,
    handle: Option<JoinHandle<()>>,
}

impl Loader {
    /// Spawn a one-shot read of all user patch headers over `device`. Locks the
    /// device per slot so UI actions (auditioning) interleave between reads.
    pub(crate) fn spawn(device: SharedDevice) -> Self {
        Self::spawn_range(device, 1, USER_SLOTS)
    }

    /// Spawn a one-shot read of the patch headers in `start..=end` (inclusive).
    /// Used both for the user bank (`1..=100`) and the factory presets
    /// (`101..=200`). Locks the device per slot so auditions interleave.
    pub(crate) fn spawn_range(device: SharedDevice, start: u16, end: u16) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let handle = {
            let cancel = Arc::clone(&cancel);
            thread::spawn(move || run(&device, &cancel, &tx, start, end))
        };
        Self {
            cancel,
            rx,
            handle: Some(handle),
        }
    }

    /// Take every result produced since the last call.
    pub(crate) fn drain(&self) -> Vec<Loaded> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run(device: &SharedDevice, cancel: &AtomicBool, tx: &Sender<Loaded>, start: u16, end: u16) {
    let mut consecutive_fail = 0u16;
    for slot in start..=end {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let result = read_slot(device, cancel, slot);
        let failed = matches!(result, Loaded::Failed(..));
        if tx.send(result).is_err() {
            return; // UI gone
        }
        if failed {
            consecutive_fail += 1;
            if consecutive_fail >= ABORT_AFTER_CONSECUTIVE {
                let _ = tx.send(Loaded::Aborted(format!(
                    "device not responding ({consecutive_fail} slots timed out in a row). \
                     Check it is powered on, connected to the right port, not in BULK LOAD \
                     mode, and that no other program is using the MIDI port — then Refresh."
                )));
                return;
            }
        } else {
            consecutive_fail = 0;
        }
        thread::sleep(READ_PACE);
    }
    let _ = tx.send(Loaded::Done);
}

/// Read one slot's header, retrying up to [`READ_ATTEMPTS`] times before giving up
/// so a single dropped reply skips just that slot rather than the whole bank.
fn read_slot(device: &SharedDevice, cancel: &AtomicBool, slot: u16) -> Loaded {
    let mut last = String::new();
    for attempt in 1..=READ_ATTEMPTS {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match lock(device).read_patch_header(slot) {
            Ok(header) => return Loaded::Header(slot, header),
            Err(e) => last = e.to_string(),
        }
        // Pace before the next try, giving the interface time to settle.
        if attempt < READ_ATTEMPTS {
            thread::sleep(READ_PACE);
        }
    }
    Loaded::Failed(
        slot,
        format!("{last} (skipped after {READ_ATTEMPTS} attempts)"),
    )
}
