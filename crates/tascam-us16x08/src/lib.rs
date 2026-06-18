//! Control-surface library for the **Tascam US-16x08** USB audio interface.
//!
//! The US-16x08's DSP mixer (faders, EQ, compressor, routing, metering) is
//! exposed by the Linux `snd-usb-audio` driver as ~280 ALSA control elements.
//! This crate wraps that surface in a typed API. It is the Rust port of the
//! hardware layer (`OAlsa`) of the original `tascamgtk` C++ application; there
//! is no PCM streaming here, only control I/O.
//!
//! # Backends
//!
//! All access goes through the [`Backend`] trait:
//! - [`AlsaBackend`] (feature `alsa`, on by default) talks to real hardware via
//!   ALSA HCTL.
//! - [`MockBackend`] is an in-memory stand-in needing no card or `libasound`,
//!   for development and tests.
//!
//! # Example
//!
//! ```
//! use tascam_us16x08::{Control, MockBackend, Us16x08, Value};
//!
//! let mut dev = Us16x08::new(MockBackend::new());
//!
//! // Mute input channel 3.
//! dev.set(Control::MuteSwitch, 3, Value::Bool(true))?;
//! assert_eq!(dev.get(Control::MuteSwitch, 3)?, Value::Bool(true));
//!
//! // Out-of-range values and indices are rejected.
//! assert!(dev.set(Control::EqLowVolume, 0, Value::Int(999)).is_err());
//! assert!(dev.get(Control::MuteSwitch, 99).is_err());
//! # Ok::<(), tascam_us16x08::Error>(())
//! ```
//!
//! On real hardware, swap the backend:
//!
//! ```no_run
//! # #[cfg(feature = "alsa")]
//! # fn main() -> Result<(), tascam_us16x08::Error> {
//! use tascam_us16x08::Us16x08;
//! let dev = Us16x08::open()?; // finds the "US16x08" card
//! let meters = dev.meters()?;
//! println!("master = {:?}", meters.master_db());
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "alsa"))] fn main() {}
//! ```

mod backend;
mod control;
mod device;
mod error;
mod event;
mod meter;
mod preset;

pub mod convert;
pub mod units;

#[cfg(feature = "alsa")]
pub use backend::AlsaBackend;
pub use backend::{Backend, MockBackend};
pub use control::{
    COMP_RATIO_VALUES, Control, Kind, NUM_CHANNELS, NUM_OUTPUTS, ROUTE_VALUES, Scope, Value,
};
pub use device::Us16x08;
pub use error::{Error, Result};
pub use event::{ControlChange, Watcher};
pub use meter::{METER_COUNT, Meters};
pub use preset::{ApplyReport, PRESET_VERSION, Preset};
