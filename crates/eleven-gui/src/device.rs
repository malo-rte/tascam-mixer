//! The Eleven Rack device, behind a boxed `Send` [`ElevenDevice`] so it can move to
//! the background bank-loader/writer threads and be swapped for the in-memory mock.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::Result;
use rackctl_eleven::{ElevenDevice, MockEleven};

#[cfg(feature = "alsa")]
use rackctl_eleven::RawMidi;

/// The device, shared between the UI thread and the background threads. Each access
/// locks it briefly, so a slow bank read interleaves with UI actions.
pub(crate) type Device = Box<dyn ElevenDevice + Send>;
pub(crate) type SharedDevice = Arc<Mutex<Device>>;

/// Open the Eleven Rack: the in-memory mock, or real ALSA rawmidi at `port`. When
/// `midi_log` is set, a real connection logs every MIDI byte in/out to that file
/// (the `--midi-log` flag; ignored for the mock).
pub(crate) fn open(mock: bool, port: Option<&str>, midi_log: Option<&Path>) -> Result<Device> {
    if mock {
        return Ok(Box::new(MockEleven::new()));
    }
    #[cfg(feature = "alsa")]
    {
        let port = port.ok_or_else(|| {
            anyhow::anyhow!("no --port given (run `rackctl-eleven ports`, or use --mock)")
        })?;
        let mut dev = RawMidi::open(port).map_err(|e| anyhow::anyhow!("{e}"))?;
        if let Some(path) = midi_log {
            dev.enable_midi_log(path)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        Ok(Box::new(dev))
    }
    #[cfg(not(feature = "alsa"))]
    {
        let _ = (port, midi_log);
        anyhow::bail!("built without ALSA support; re-run with --mock")
    }
}

/// A never-touched placeholder device for the disconnected/offline state, so the
/// app always holds a [`SharedDevice`] and can retry the real open.
pub(crate) fn placeholder() -> Device {
    Box::new(MockEleven::new())
}

/// Lock the shared device, recovering from a poisoned mutex rather than panicking.
pub(crate) fn lock(device: &SharedDevice) -> MutexGuard<'_, Device> {
    device.lock().unwrap_or_else(PoisonError::into_inner)
}
