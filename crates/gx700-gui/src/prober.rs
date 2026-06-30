//! Background BULK LOAD probe: runs a single `probe_bulk_load` off the UI thread,
//! so the "waiting for BULK LOAD" screen keeps answering the compositor between
//! probes instead of hitching on each one (a silent unit blocks until timeout).

use std::sync::mpsc::{Receiver, channel};
use std::thread::{self, JoinHandle};

use crate::device::{SharedDevice, lock};

/// The outcome of one BULK LOAD probe.
pub(crate) enum Probe {
    /// The unit answered as in BULK LOAD mode — ready to read/write the bank.
    InBulkLoad,
    /// The unit answered but isn't in BULK LOAD mode yet — keep waiting.
    Waiting,
    /// The probe couldn't run (unit silent, or nothing to read — e.g. the mock).
    /// Don't block on it; let the bank read surface any real trouble.
    CantRun,
}

/// A one-shot probe in flight. Dropping it joins the thread.
pub(crate) struct Prober {
    rx: Receiver<Probe>,
    handle: Option<JoinHandle<()>>,
}

impl Prober {
    /// Spawn a one-shot `probe_bulk_load(slot)` off the UI thread.
    pub(crate) fn spawn(device: SharedDevice, slot: u16) -> Self {
        let (tx, rx) = channel();
        let handle = thread::spawn(move || {
            let outcome = match lock(&device).probe_bulk_load(slot) {
                Ok(true) => Probe::InBulkLoad,
                Ok(false) => Probe::Waiting,
                Err(_) => Probe::CantRun,
            };
            let _ = tx.send(outcome);
        });
        Self {
            rx,
            handle: Some(handle),
        }
    }

    /// The probe result, if it has finished; `None` while still probing.
    pub(crate) fn poll(&self) -> Option<Probe> {
        self.rx.try_recv().ok()
    }
}

impl Drop for Prober {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
