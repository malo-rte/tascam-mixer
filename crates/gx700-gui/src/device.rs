//! The shared GX-700 device, behind a boxed `Send` transport so it can move to
//! the background bank-loader thread.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::Result;
use rackctl_gx700::{Gx700, MockTransport, Transport};

#[cfg(feature = "alsa")]
use rackctl_gx700::RawMidi;

/// The device, shared between the UI thread and the loader. Each access locks it
/// briefly.
pub(crate) type Device = Gx700<Box<dyn Transport + Send>>;
pub(crate) type SharedDevice = Arc<Mutex<Device>>;

/// Open the GX-700: the in-memory mock, or real ALSA rawmidi at `port`.
pub(crate) fn open(mock: bool, port: Option<&str>) -> Result<Device> {
    if mock {
        return Ok(Gx700::new(Box::new(MockTransport::new())));
    }
    #[cfg(feature = "alsa")]
    {
        let port = port.ok_or_else(|| {
            anyhow::anyhow!("no --port given (run `rackctl-gx700 ports`, or use --mock)")
        })?;
        Ok(Gx700::new(Box::new(RawMidi::open(port)?)))
    }
    #[cfg(not(feature = "alsa"))]
    {
        let _ = port;
        anyhow::bail!("built without ALSA support; re-run with --mock")
    }
}

/// A never-read placeholder device for the disconnected state, so the app always
/// holds a [`SharedDevice`] and can retry the real open.
pub(crate) fn placeholder() -> Device {
    Gx700::new(Box::new(MockTransport::new()) as Box<dyn Transport + Send>)
}

/// Lock the shared device, recovering from a poisoned mutex rather than panicking.
pub(crate) fn lock(device: &SharedDevice) -> MutexGuard<'_, Device> {
    device.lock().unwrap_or_else(PoisonError::into_inner)
}
