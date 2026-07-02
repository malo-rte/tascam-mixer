//! Background bank reader: reads the Eleven Rack's patch bank off the UI thread so
//! the window keeps answering the compositor. The name directory is read fast
//! (one `0x04` block per slot, no Program Change); a *deep* capture (whole
//! [`PatchBackup`] per slot, for scenes/backup) selects each slot and reads it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rackctl_eleven::PatchBackup;
use rackctl_eleven_lib::manage;

use crate::device::{SharedDevice, lock};

/// Number of User patch slots (`0x00..=0x67`, confirmed from a full-bank backup).
pub(crate) use rackctl_eleven::backup::USER_SLOTS;

/// Gap between slot reads, to avoid overrunning the USB-MIDI interface.
const READ_PACE: Duration = Duration::from_millis(15);

/// If this many slots fail back-to-back the device is clearly not answering (off,
/// unplugged, wrong port, or the port is in use), so the load aborts with one clear
/// message instead of grinding through every slot.
const ABORT_AFTER_CONSECUTIVE: u8 = 6;

/// A result from the loader.
pub(crate) enum Loaded {
    /// One slot's stored name (directory read).
    Name(u8, String),
    /// One slot's whole captured patch (deep read — for scene capture / backup).
    Patch(u8, PatchBackup),
    /// A slot read failed; the load continues with the next.
    Failed(u8),
    /// The load gave up early because the device stopped answering (message).
    Aborted(String),
    /// The whole run finished (or was cancelled).
    Done,
}

/// A running bank load. Dropping it cancels the read and joins the thread.
pub(crate) struct Loader {
    cancel: Arc<AtomicBool>,
    rx: Receiver<Loaded>,
    handle: Option<JoinHandle<()>>,
}

/// What a load run reads.
enum Mode {
    /// Every User slot's stored name (fast directory read).
    Directory,
    /// Every Factory slot's name (select + read — slow, and auditions each).
    Factory,
    /// A deep capture (whole [`PatchBackup`]) of the given User slots.
    Capture(Vec<u8>),
}

impl Loader {
    /// Spawn a fast directory read of all User slot names, streamed slot-by-slot.
    pub(crate) fn spawn_directory(device: SharedDevice) -> Self {
        Self::spawn(device, Mode::Directory)
    }

    /// Spawn a read of all Factory slot names (selects each — for the preset browser).
    pub(crate) fn spawn_factory(device: SharedDevice) -> Self {
        Self::spawn(device, Mode::Factory)
    }

    /// Spawn a deep capture of `slots` (whole patches), streamed as they arrive.
    pub(crate) fn spawn_capture(device: SharedDevice, slots: Vec<u8>) -> Self {
        Self::spawn(device, Mode::Capture(slots))
    }

    fn spawn(device: SharedDevice, mode: Mode) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let handle = {
            let cancel = Arc::clone(&cancel);
            thread::spawn(move || run(&device, &cancel, &tx, &mode))
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

fn run(device: &SharedDevice, cancel: &AtomicBool, tx: &Sender<Loaded>, mode: &Mode) {
    let slots: Vec<u8> = match mode {
        Mode::Capture(s) => s.clone(),
        Mode::Directory | Mode::Factory => (0..USER_SLOTS).collect(),
    };
    let mut consecutive_fail = 0u8;
    for slot in slots {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let result = read_slot(device, slot, mode);
        let failed = matches!(result, Loaded::Failed(..));
        if tx.send(result).is_err() {
            return; // UI gone
        }
        if failed {
            consecutive_fail += 1;
            if consecutive_fail >= ABORT_AFTER_CONSECUTIVE {
                let _ = tx.send(Loaded::Aborted(
                    "device not responding — check it is powered on, on the right port, \
                     and that no other program is using the MIDI port, then Refresh."
                        .to_owned(),
                ));
                return;
            }
        } else {
            consecutive_fail = 0;
        }
        thread::sleep(READ_PACE);
    }
    let _ = tx.send(Loaded::Done);
}

fn read_slot(device: &SharedDevice, slot: u8, mode: &Mode) -> Loaded {
    match mode {
        Mode::Capture(_) => match manage::capture(&mut **lock(device), Some(slot)) {
            Ok(p) => Loaded::Patch(slot, p),
            Err(_) => Loaded::Failed(slot),
        },
        Mode::Directory => match manage::slot_name(&mut **lock(device), slot) {
            Some(name) => Loaded::Name(slot, name),
            None => Loaded::Failed(slot),
        },
        Mode::Factory => match manage::factory_name(&mut **lock(device), slot) {
            Some(name) => Loaded::Name(slot, name),
            None => Loaded::Failed(slot),
        },
    }
}
