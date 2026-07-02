//! Background batch writer: applies staged patch-bank changes to the unit
//! slot-by-slot off the UI thread (each verified by read-back), reporting progress
//! so a whole scene writes behind a progress bar instead of freezing the window.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rackctl_eleven::PatchBackup;
use rackctl_eleven_lib::manage;

use crate::device::{SharedDevice, lock};

/// Gap between writes, to avoid overrunning the USB-MIDI interface.
const WRITE_PACE: Duration = Duration::from_millis(20);

/// One staged change to apply to a User slot.
pub(crate) enum WriteJob {
    /// Rename the slot in place (content unchanged).
    Rename(u8, String),
    /// Overwrite the slot with a whole captured patch (verified).
    Restore(u8, PatchBackup),
}

impl WriteJob {
    fn slot(&self) -> u8 {
        match self {
            Self::Rename(s, _) | Self::Restore(s, _) => *s,
        }
    }
}

/// A result from the background writer.
pub(crate) enum Written {
    /// A slot was written and verified.
    Ok(u8),
    /// A slot failed to write / verify (slot, message).
    Failed(u8, String),
    /// The batch finished (or was cancelled).
    Done,
}

/// A running batch write. Dropping it cancels and joins the thread.
pub(crate) struct Writer {
    cancel: Arc<AtomicBool>,
    rx: Receiver<Written>,
    handle: Option<JoinHandle<()>>,
    total: usize,
}

impl Writer {
    /// Spawn a write of every job to the device, verifying each. Locks the device
    /// per job so the write stays cooperative with UI actions.
    pub(crate) fn spawn(device: SharedDevice, jobs: Vec<WriteJob>) -> Self {
        let total = jobs.len();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let handle = {
            let cancel = Arc::clone(&cancel);
            thread::spawn(move || run(&device, &cancel, &tx, jobs))
        };
        Self {
            cancel,
            rx,
            handle: Some(handle),
            total,
        }
    }

    /// How many jobs the batch will write.
    pub(crate) fn total(&self) -> usize {
        self.total
    }

    /// Take every result produced since the last call.
    pub(crate) fn drain(&self) -> Vec<Written> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run(device: &SharedDevice, cancel: &AtomicBool, tx: &Sender<Written>, jobs: Vec<WriteJob>) {
    for job in jobs {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let slot = job.slot();
        let result = match apply(device, &job) {
            Ok(()) => Written::Ok(slot),
            Err(e) => Written::Failed(slot, e),
        };
        if tx.send(result).is_err() {
            return; // UI gone
        }
        thread::sleep(WRITE_PACE);
    }
    let _ = tx.send(Written::Done);
}

/// Apply one job, verifying. `Err` carries a message on write/verify failure.
fn apply(device: &SharedDevice, job: &WriteJob) -> Result<(), String> {
    match job {
        WriteJob::Rename(slot, name) => manage::rename(&mut **lock(device), *slot, name),
        WriteJob::Restore(slot, patch) => {
            let report = manage::restore(&mut **lock(device), *slot, patch)?;
            if report.ok() {
                Ok(())
            } else {
                Err(format!("verify failed: {report}"))
            }
        }
    }
}
