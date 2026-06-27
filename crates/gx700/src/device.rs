//! The typed device facade over a [`Transport`].

use crate::backend::Transport;
use crate::error::{Error, Result};
use crate::param::{Kind, Param, Value};

/// A BOSS GX-700, addressed through typed [`Param`]s.
///
/// Generic over the [`Transport`], so the same API drives real hardware
/// ([`crate::RawMidi`]) or an in-memory [`crate::MockTransport`].
#[derive(Debug)]
pub struct Gx700<T: Transport> {
    transport: T,
}

impl<T: Transport> Gx700<T> {
    /// Wrap a transport.
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Read a parameter's current value.
    ///
    /// # Errors
    /// Propagates transport errors, and [`Error::Sysex`]/[`Error::Timeout`] from
    /// a hardware transport if the reply is malformed or absent.
    pub fn get(&mut self, param: Param) -> Result<Value> {
        let bytes = self.transport.request(&param.address(), param.width())?;
        let raw = param.encoding().decode(&bytes);
        Ok(match param.kind() {
            Kind::Bool => Value::Bool(raw != 0),
            Kind::Int { .. } => Value::Int(raw),
            Kind::Enum { .. } => Value::Enum(raw),
        })
    }

    /// Write a parameter's value, validating its kind and range.
    ///
    /// # Errors
    /// - [`Error::TypeMismatch`] if `value`'s kind does not match `param`.
    /// - [`Error::ValueOutOfRange`] if an int/enum value is out of range.
    /// - Transport errors otherwise.
    pub fn set(&mut self, param: Param, value: Value) -> Result<()> {
        let raw = encode(param, value)?;
        let mut buf = [0u8; 2];
        let n = param.encoding().encode(raw, &mut buf);
        let data: &[u8] = &buf;
        self.transport
            .send(&param.address(), data.get(..n).unwrap_or(data))
    }

    /// Select a patch memory by sending a MIDI Program Change.
    ///
    /// # Errors
    /// Propagates transport errors.
    pub fn select_patch(&mut self, n: u8) -> Result<()> {
        self.transport.program_change(n)
    }

    /// Borrow the underlying transport.
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Mutably borrow the underlying transport.
    #[must_use]
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Consume the device and return the transport.
    #[must_use]
    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg(feature = "alsa")]
impl Gx700<crate::backend::RawMidi> {
    /// Open a GX-700 on the rawmidi `port` (`hw:CARD,DEV`) and wrap it.
    ///
    /// # Errors
    /// [`Error::PortNotFound`]/[`Error::Transport`] if the port cannot be opened.
    pub fn open(port: &str) -> Result<Self> {
        Ok(Self::new(crate::backend::RawMidi::open(port)?))
    }
}

/// Validate `value` against `param` and return the raw device value. The byte
/// layout (single byte, MIDI-14-bit, or nibblized) is applied by the caller via
/// [`rackctl_gx700::Encoding::encode`](crate::param::Encoding::encode).
fn encode(param: Param, value: Value) -> Result<i32> {
    let key = param.key();
    match (param.kind(), value) {
        (Kind::Bool, Value::Bool(b)) => Ok(i32::from(b)),
        (Kind::Int { min, max, .. }, Value::Int(v)) => {
            range_check(key, v, min, max)?;
            Ok(v)
        }
        (Kind::Enum { values, .. }, Value::Enum(v)) => {
            let max = i32::try_from(values.len().saturating_sub(1)).unwrap_or(0);
            range_check(key, v, 0, max)?;
            Ok(v)
        }
        (kind, _) => Err(Error::TypeMismatch {
            param: key,
            expected: kind_name(kind),
        }),
    }
}

/// Reject a value outside `min..=max`.
fn range_check(param: &'static str, value: i32, min: i32, max: i32) -> Result<()> {
    if value < min || value > max {
        return Err(Error::ValueOutOfRange {
            param,
            value,
            min,
            max,
        });
    }
    Ok(())
}

/// Human-readable name for a [`Kind`], for error messages.
const fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Bool => "boolean",
        Kind::Int { .. } => "integer",
        Kind::Enum { .. } => "enum",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::backend::MockTransport;
    use crate::param;

    fn dev() -> Gx700<MockTransport> {
        Gx700::new(MockTransport::new())
    }

    fn p(key: &str) -> Param {
        Param::from_key(key).unwrap()
    }

    #[test]
    fn set_get_round_trips() {
        let mut d = dev();
        d.set(p("preamp-volume"), Value::Int(80)).unwrap();
        assert_eq!(d.get(p("preamp-volume")).unwrap(), Value::Int(80));

        d.set(p("comp-enable"), Value::Bool(true)).unwrap();
        assert_eq!(d.get(p("comp-enable")).unwrap(), Value::Bool(true));

        d.set(p("dist-type"), Value::Enum(2)).unwrap();
        assert_eq!(d.get(p("dist-type")).unwrap(), Value::Enum(2));
    }

    #[test]
    fn out_of_range_is_rejected() {
        let mut d = dev();
        assert!(matches!(
            d.set(p("preamp-volume"), Value::Int(999)),
            Err(Error::ValueOutOfRange { .. })
        ));
    }

    #[test]
    fn kind_mismatch_is_rejected() {
        let mut d = dev();
        assert!(matches!(
            d.set(p("comp-enable"), Value::Int(1)),
            Err(Error::TypeMismatch { .. })
        ));
    }

    #[test]
    fn enum_out_of_range_is_rejected() {
        let mut d = dev();
        let count = param::DIST_TYPE_VALUES.len();
        assert!(matches!(
            d.set(p("dist-type"), Value::Enum(i32::try_from(count).unwrap())),
            Err(Error::ValueOutOfRange { .. })
        ));
    }

    #[test]
    fn select_patch_records_program() {
        let mut d = dev();
        d.select_patch(42).unwrap();
        assert_eq!(d.transport().program(), 42);
    }
}
