//! Observing control changes.
//!
//! The original app used the HCTL event callback (`OAlsa::do_work`) to learn
//! when the hardware or another client changed a value. [`Watcher`] offers the
//! same capability in a backend-agnostic, testable way: it diffs successive
//! reads of the control surface and reports what changed.
//!
//! On real hardware, gate polling behind [`crate::AlsaBackend::wait`] so you
//! only re-read after the card signals an event rather than busy-looping.

use std::collections::HashMap;

use crate::backend::Backend;
use crate::control::{Control, Kind, Value};
use crate::device::Us16x08;
use crate::error::Result;

/// A single observed control change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ControlChange {
    /// Which control changed.
    pub control: Control,
    /// The element index within the control's scope.
    pub index: u32,
    /// The new value.
    pub value: Value,
}

/// Tracks the last-seen value of every control and reports changes.
#[derive(Debug, Default)]
pub struct Watcher {
    snapshot: HashMap<(Control, u32), Value>,
}

impl Watcher {
    /// Create an empty watcher. The first [`Self::poll`] reports every control
    /// as changed (initial sync); use [`Self::prime`] first to suppress that.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture the current value of every control as the baseline, without
    /// reporting any changes.
    ///
    /// # Errors
    /// Propagates backend read errors.
    pub fn prime<B: Backend>(&mut self, device: &Us16x08<B>) -> Result<()> {
        let _ = self.poll(device)?;
        Ok(())
    }

    /// Re-read every (non-meter) control and return those whose value differs
    /// from the previous poll.
    ///
    /// # Errors
    /// Propagates backend read errors.
    pub fn poll<B: Backend>(&mut self, device: &Us16x08<B>) -> Result<Vec<ControlChange>> {
        let mut changes = Vec::new();
        for &control in Control::ALL {
            // Skip the meter block (not a scalar) and any control this device
            // does not expose (catalogs span kernel versions; not all are
            // present on every device).
            if matches!(control.kind(), Kind::Meter) || !device.is_present(control) {
                continue;
            }
            for index in 0..control.scope().count() {
                let value = device.get(control, index)?;
                let key = (control, index);
                if self.snapshot.get(&key) != Some(&value) {
                    self.snapshot.insert(key, value);
                    changes.push(ControlChange {
                        control,
                        index,
                        value,
                    });
                }
            }
        }
        Ok(changes)
    }
}
