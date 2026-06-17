//! The typed device facade over a [`Backend`].

use std::collections::{HashMap, HashSet};

use crate::backend::Backend;
use crate::control::{Control, Kind, NUM_CHANNELS, Value};
use crate::error::{Error, Result};
use crate::meter::{METER_COUNT, Meters};

/// Resolve the ALSA name to use for `control`, preferring whichever spelling the
/// loaded card actually exposes (handles the kernel `Frequency`/`Frequence`
/// rename). Falls back to the canonical name when nothing matches.
fn resolve_name(control: Control, loaded: &[String]) -> &'static str {
    let canonical = control.alsa_name();
    if loaded.iter().any(|n| n == canonical) {
        return canonical;
    }
    for &alias in control.alsa_aliases() {
        if loaded.iter().any(|n| n == alias) {
            return alias;
        }
    }
    canonical
}

/// A US-16x08 mixer, addressed through typed [`Control`]s.
///
/// Generic over the [`Backend`], so the same API drives real hardware
/// ([`crate::AlsaBackend`]) or an in-memory [`crate::MockBackend`].
#[derive(Debug)]
pub struct Us16x08<B: Backend> {
    backend: B,
    names: HashMap<Control, &'static str>,
    present: HashSet<Control>,
}

impl<B: Backend> Us16x08<B> {
    /// Wrap a backend, resolving control-name aliases against its element list
    /// and recording which controls the device actually exposes.
    #[must_use]
    pub fn new(backend: B) -> Self {
        let loaded = backend.elem_names();
        let mut names = HashMap::new();
        let mut present = HashSet::new();
        for &control in Control::ALL {
            let name = resolve_name(control, &loaded);
            names.insert(control, name);
            if loaded.iter().any(|n| n == name) {
                present.insert(control);
            }
        }
        Self {
            backend,
            names,
            present,
        }
    }

    /// Whether the connected device exposes this control. Not every cataloged
    /// control exists on every device/kernel; callers that sweep the whole
    /// catalog (e.g. [`crate::Watcher`]) use this to skip absent ones.
    #[must_use]
    pub fn is_present(&self, control: Control) -> bool {
        self.present.contains(&control)
    }

    /// The resolved ALSA name for a control.
    fn name(&self, control: Control) -> &'static str {
        self.names
            .get(&control)
            .copied()
            .unwrap_or(control.alsa_name())
    }

    /// Validate an index against a control's scope.
    fn check_index(control: Control, index: u32) -> Result<()> {
        let count = control.scope().count();
        if index >= count {
            return Err(Error::IndexOutOfRange { index, count });
        }
        Ok(())
    }

    /// Read a control's current value.
    ///
    /// # Errors
    /// - [`Error::IndexOutOfRange`] if `index` is outside the control's scope.
    /// - [`Error::TypeMismatch`] for [`Control::LevelMeter`] (use
    ///   [`Self::meters`] instead).
    /// - [`Error::Backend`]/[`Error::UnknownControl`] from the backend.
    pub fn get(&self, control: Control, index: u32) -> Result<Value> {
        Self::check_index(control, index)?;
        let name = self.name(control);
        match control.kind() {
            Kind::Bool => Ok(Value::Bool(self.backend.get_bool(name, index)?)),
            Kind::Int { .. } => Ok(Value::Int(self.backend.get_int(name, index)?)),
            Kind::Enum { .. } => Ok(Value::Enum(self.backend.get_int(name, index)?)),
            Kind::Meter => Err(Error::TypeMismatch {
                control: control.alsa_name(),
                expected: "meter block (use meters())",
            }),
        }
    }

    /// Write a control's value, validating kind and range.
    ///
    /// # Errors
    /// - [`Error::IndexOutOfRange`] if `index` is outside the control's scope.
    /// - [`Error::TypeMismatch`] if `value`'s kind does not match the control.
    /// - [`Error::ValueOutOfRange`] if an int/enum value is out of range.
    /// - [`Error::Backend`]/[`Error::UnknownControl`] from the backend.
    pub fn set(&mut self, control: Control, index: u32, value: Value) -> Result<()> {
        Self::check_index(control, index)?;
        let name = self.name(control);
        match (control.kind(), value) {
            (Kind::Bool, Value::Bool(b)) => self.backend.set_bool(name, index, b),
            (Kind::Int { min, max, .. }, Value::Int(v)) => {
                if v < min || v > max {
                    return Err(Error::ValueOutOfRange {
                        control: control.alsa_name(),
                        value: v,
                        min,
                        max,
                    });
                }
                self.backend.set_int(name, index, v)
            }
            (Kind::Enum { values, .. }, Value::Enum(v)) => {
                let len = i32::try_from(values.len()).unwrap_or(i32::MAX);
                if v < 0 || v >= len {
                    return Err(Error::ValueOutOfRange {
                        control: control.alsa_name(),
                        value: v,
                        min: 0,
                        max: len - 1,
                    });
                }
                self.backend.set_int(name, index, v)
            }
            (kind, _) => Err(Error::TypeMismatch {
                control: control.alsa_name(),
                expected: kind_name(kind),
            }),
        }
    }

    /// Read the level-meter block.
    ///
    /// # Errors
    /// [`Error::Backend`]/[`Error::UnknownControl`] from the backend.
    pub fn meters(&self) -> Result<Meters> {
        let name = self.name(Control::LevelMeter);
        let mut raw = [0i32; METER_COUNT];
        self.backend.get_ints(name, &mut raw)?;
        Ok(Meters::from_raw(raw))
    }

    /// Select which channel's DSP the device reports in the meter stream, or
    /// `None` to deselect.
    ///
    /// Mirrors `OAlsa::on_active_button_control_changed`, which writes the
    /// `Level Meter` element (index 0) with `channel + 1`, or `0` for none.
    ///
    /// # Errors
    /// [`Error::IndexOutOfRange`] if `channel >= 16`; backend errors otherwise.
    pub fn select_dsp_channel(&mut self, channel: Option<u32>) -> Result<()> {
        let name = self.name(Control::LevelMeter);
        let val = match channel {
            Some(c) => i32::try_from(c + 1).map_err(|_| Error::IndexOutOfRange {
                index: c,
                count: NUM_CHANNELS,
            })?,
            None => 0,
        };
        self.backend.set_int(name, 0, val)
    }

    /// Borrow the underlying backend (e.g. to call [`crate::AlsaBackend::wait`]).
    #[must_use]
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutably borrow the underlying backend.
    #[must_use]
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Consume the device and return the backend.
    #[must_use]
    pub fn into_backend(self) -> B {
        self.backend
    }
}

#[cfg(feature = "alsa")]
impl Us16x08<crate::backend::AlsaBackend> {
    /// Open the US-16x08 hardware and wrap it.
    ///
    /// # Errors
    /// [`Error::CardNotFound`] if the card is absent; [`Error::Backend`] on
    /// ALSA failures.
    pub fn open() -> Result<Self> {
        Ok(Self::new(crate::backend::AlsaBackend::open()?))
    }
}

/// Human-readable name for a [`Kind`], for error messages.
const fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Bool => "boolean",
        Kind::Int { .. } => "integer",
        Kind::Enum { .. } => "enum",
        Kind::Meter => "meter",
    }
}
