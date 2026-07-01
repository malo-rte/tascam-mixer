//! Pure System Exclusive codec for the Eleven Rack, with no I/O.
//!
//! The Eleven Rack speaks Digidesign address-mapped `SysEx`:
//! `F0 13 0B <dev> <opcode> <addr..> <value..> F7`. This module builds those
//! frames, parses them back, and decodes the Universal Identity reply. The
//! manufacturer-independent framing ([`Framer`], `SYSEX_START`/`SYSEX_END`) lives
//! in the shared [`rackctl_sysex`] crate and is re-exported here.
//!
//! See `docs/eleven-rack-sysex-protocol.adoc` for the reverse-engineered details.

use rackctl_eleven_model::error::{Error, Result};
use rackctl_eleven_model::value::{RawValue, VALUE_LEN};
pub use rackctl_sysex::{Framer, SYSEX_END, SYSEX_START};

/// Digidesign's MIDI manufacturer id (as emitted by this unit).
pub const DIGIDESIGN_ID: u8 = 0x13;
/// The Eleven Rack's model id in the Digidesign header.
pub const ELEVEN_RACK_MODEL: u8 = 0x0B;
/// The device id the unit ships with (`F0 13 0B 0F ...`).
pub const DEFAULT_DEVICE_ID: u8 = 0x0F;

/// Opcode: read request (host -> unit), `F0 13 0B <dev> 01 <addr> F7`.
pub const READ_REQUEST: u8 = 0x01;
/// Opcode: read reply (unit -> host), the solicited answer to [`READ_REQUEST`].
pub const READ_REPLY: u8 = 0x12;
/// Opcode: change report (unit -> host), unsolicited when a knob moves.
pub const CHANGE_REPORT: u8 = 0x02;
/// Opcode: write/set (host -> unit), `F0 13 0B <dev> 00 <addr> <value> F7`.
/// Confirmed against hardware by set + read-back (amp Gain at `11 21 07`).
pub const WRITE: u8 = 0x00;

/// Universal Identity Request, the safe first probe: `F0 7E 7F 06 01 F7`.
const UNIVERSAL_NON_REALTIME: u8 = 0x7E;
const ID_GENERAL_INFO: u8 = 0x06;
const ID_REQUEST: u8 = 0x01;
const ID_REPLY: u8 = 0x02;

/// Build the Universal Identity Request (`F0 7E 7F 06 01 F7`).
#[must_use]
pub fn build_identity_request() -> Vec<u8> {
    vec![
        SYSEX_START,
        UNIVERSAL_NON_REALTIME,
        0x7F, // broadcast device id
        ID_GENERAL_INFO,
        ID_REQUEST,
        SYSEX_END,
    ]
}

/// Build a read request: `F0 13 0B <dev> 01 <addr> F7`.
#[must_use]
pub fn build_read_request(device_id: u8, addr: &[u8]) -> Vec<u8> {
    build(device_id, READ_REQUEST, addr, &[])
}

/// Build a write/set: `F0 13 0B <dev> 00 <addr> <value> F7`.
#[must_use]
pub fn build_write(device_id: u8, addr: &[u8], value: &RawValue) -> Vec<u8> {
    build(device_id, WRITE, addr, value.as_bytes())
}

/// Shared body of the frame builders.
fn build(device_id: u8, opcode: u8, addr: &[u8], value: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(addr.len() + value.len() + 6);
    msg.push(SYSEX_START);
    msg.push(DIGIDESIGN_ID);
    msg.push(ELEVEN_RACK_MODEL);
    msg.push(device_id);
    msg.push(opcode);
    msg.extend_from_slice(addr);
    msg.extend_from_slice(value);
    msg.push(SYSEX_END);
    msg
}

/// A parsed Digidesign Eleven Rack message.
///
/// `payload` is everything between the opcode and the closing `F7` — for a read
/// reply or change report that is `<addr..> <value..>`; for a read request it is
/// just `<addr..>`. The address/value split is parameter-dependent, so callers
/// use [`DigiMessage::value_at`] with a known address rather than assuming a
/// fixed address width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigiMessage {
    /// The device id from the message header.
    pub device_id: u8,
    /// The opcode byte (e.g. [`READ_REPLY`] or [`CHANGE_REPORT`]).
    pub opcode: u8,
    /// Address-plus-value region, framing and header removed.
    pub payload: Vec<u8>,
}

impl DigiMessage {
    /// If this message reports a value for `addr` (its payload is `addr`
    /// followed by exactly [`VALUE_LEN`] bytes), return that value. Works for
    /// both a solicited reply ([`READ_REPLY`]) and a change report
    /// ([`CHANGE_REPORT`]).
    #[must_use]
    pub fn value_at(&self, addr: &[u8]) -> Option<RawValue> {
        let rest = self.payload.strip_prefix(addr)?;
        let bytes: [u8; VALUE_LEN] = rest.try_into().ok()?;
        Some(RawValue::from_bytes(bytes))
    }
}

/// Parse a complete `F0..F7` message as a Digidesign Eleven Rack frame.
///
/// Validates the framing, the Digidesign manufacturer id and the Eleven Rack
/// model id, returning the device id, opcode and payload.
///
/// # Errors
/// [`Error::Sysex`] if the framing is wrong, the manufacturer/model id does not
/// match the Eleven Rack, or the message is too short.
pub fn parse(msg: &[u8]) -> Result<DigiMessage> {
    let inner = msg
        .strip_prefix(&[SYSEX_START])
        .and_then(|m| m.strip_suffix(&[SYSEX_END]))
        .ok_or_else(|| Error::Sysex("message is not framed by F0..F7".to_owned()))?;

    let (&manufacturer, rest) = inner
        .split_first()
        .ok_or_else(|| Error::Sysex("empty sysex message".to_owned()))?;
    if manufacturer != DIGIDESIGN_ID {
        return Err(Error::Sysex(format!(
            "manufacturer id {manufacturer:#04x} is not Digidesign ({DIGIDESIGN_ID:#04x})"
        )));
    }

    let (&model, rest) = rest
        .split_first()
        .ok_or_else(|| Error::Sysex("missing model id".to_owned()))?;
    if model != ELEVEN_RACK_MODEL {
        return Err(Error::Sysex(format!(
            "model id {model:#04x} is not the Eleven Rack ({ELEVEN_RACK_MODEL:#04x})"
        )));
    }

    let (&device_id, rest) = rest
        .split_first()
        .ok_or_else(|| Error::Sysex("missing device id".to_owned()))?;

    let (&opcode, payload) = rest
        .split_first()
        .ok_or_else(|| Error::Sysex("missing opcode byte".to_owned()))?;

    Ok(DigiMessage {
        device_id,
        opcode,
        payload: payload.to_vec(),
    })
}

/// A decoded Universal Identity reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The responding device id.
    pub device_id: u8,
    /// Manufacturer id (Digidesign reports `0x13`).
    pub manufacturer: u8,
    /// Device family code.
    pub family: u16,
    /// Device model number.
    pub model: u16,
    /// Firmware/version field, as text (e.g. `"0157"`).
    pub version: String,
}

/// Parse a Universal Identity reply
/// (`F0 7E <dev> 06 02 <mfr> <family LE> <model LE> <version..> F7`).
///
/// # Errors
/// [`Error::Sysex`] if the framing or the identity sub-ids are wrong, or the
/// message is too short to hold the manufacturer/family/model fields.
pub fn parse_identity_reply(msg: &[u8]) -> Result<Identity> {
    let inner = msg
        .strip_prefix(&[SYSEX_START])
        .and_then(|m| m.strip_suffix(&[SYSEX_END]))
        .ok_or_else(|| Error::Sysex("message is not framed by F0..F7".to_owned()))?;

    // Expect: 7E <dev> 06 02 <mfr> <fam_lo> <fam_hi> <mod_lo> <mod_hi> <version..>
    let bad = || Error::Sysex("not a Universal Identity reply".to_owned());
    let &kind = inner.first().ok_or_else(bad)?;
    let &device_id = inner.get(1).ok_or_else(bad)?;
    let &sub1 = inner.get(2).ok_or_else(bad)?;
    let &sub2 = inner.get(3).ok_or_else(bad)?;
    if kind != UNIVERSAL_NON_REALTIME || sub1 != ID_GENERAL_INFO || sub2 != ID_REPLY {
        return Err(bad());
    }
    let &manufacturer = inner.get(4).ok_or_else(bad)?;
    let &fam_lo = inner.get(5).ok_or_else(bad)?;
    let &fam_hi = inner.get(6).ok_or_else(bad)?;
    let &mod_lo = inner.get(7).ok_or_else(bad)?;
    let &mod_hi = inner.get(8).ok_or_else(bad)?;
    let version_bytes = inner.get(9..).unwrap_or(&[]);

    Ok(Identity {
        device_id,
        manufacturer,
        family: u16::from(fam_lo) | (u16::from(fam_hi) << 7),
        model: u16::from(mod_lo) | (u16::from(mod_hi) << 7),
        version: String::from_utf8_lossy(version_bytes).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    const AMP_GAIN: [u8; 3] = [0x11, 0x21, 0x0D];

    #[test]
    fn read_request_round_trips_through_parse() {
        let msg = build_read_request(DEFAULT_DEVICE_ID, &AMP_GAIN);
        assert_eq!(
            msg,
            vec![0xF0, 0x13, 0x0B, 0x0F, 0x01, 0x11, 0x21, 0x0D, 0xF7]
        );
        let parsed = parse(&msg).unwrap();
        assert_eq!(parsed.device_id, DEFAULT_DEVICE_ID);
        assert_eq!(parsed.opcode, READ_REQUEST);
        assert_eq!(parsed.payload, AMP_GAIN);
        // A bare request carries no value.
        assert_eq!(parsed.value_at(&AMP_GAIN), None);
    }

    #[test]
    fn read_reply_value_is_extracted() {
        // The real reply captured from hardware for amp Gain.
        let reply = [
            0xF0, 0x13, 0x0B, 0x0F, 0x12, 0x11, 0x21, 0x0D, 0x6D, 0x00, 0x00, 0x00, 0x10, 0xF7,
        ];
        let parsed = parse(&reply).unwrap();
        assert_eq!(parsed.opcode, READ_REPLY);
        let value = parsed.value_at(&AMP_GAIN).expect("value present");
        assert_eq!(value.as_bytes(), &[0x6D, 0x00, 0x00, 0x00, 0x10]);
        assert_eq!(value.decode(), 0x6D | (0x10u64 << 28));
    }

    #[test]
    fn write_frame_has_value_payload() {
        let value = RawValue::from_bytes([0x6D, 0, 0, 0, 0x10]);
        let msg = build_write(DEFAULT_DEVICE_ID, &AMP_GAIN, &value);
        let parsed = parse(&msg).unwrap();
        assert_eq!(parsed.opcode, WRITE);
        assert_eq!(parsed.value_at(&AMP_GAIN), Some(value));
    }

    #[test]
    fn parse_rejects_wrong_manufacturer() {
        let mut msg = build_read_request(DEFAULT_DEVICE_ID, &AMP_GAIN);
        if let Some(b) = msg.get_mut(1) {
            *b = 0x41; // Roland, not Digidesign
        }
        assert!(matches!(parse(&msg), Err(Error::Sysex(_))));
    }

    #[test]
    fn parse_rejects_wrong_model() {
        let mut msg = build_read_request(DEFAULT_DEVICE_ID, &AMP_GAIN);
        if let Some(b) = msg.get_mut(2) {
            *b = 0x0C;
        }
        assert!(matches!(parse(&msg), Err(Error::Sysex(_))));
    }

    #[test]
    fn identity_reply_decodes() {
        let reply = [
            0xF0, 0x7E, 0x0F, 0x06, 0x02, 0x13, 0x0B, 0x00, 0x01, 0x00, 0x30, 0x31, 0x35, 0x37,
            0xF7,
        ];
        let id = parse_identity_reply(&reply).unwrap();
        assert_eq!(id.device_id, 0x0F);
        assert_eq!(id.manufacturer, DIGIDESIGN_ID);
        assert_eq!(id.family, 0x000B);
        assert_eq!(id.model, 0x0001);
        assert_eq!(id.version, "0157");
    }

    #[test]
    fn framer_splits_two_messages_with_junk() {
        let a = build_read_request(DEFAULT_DEVICE_ID, &[0x10]);
        let b = build_read_request(DEFAULT_DEVICE_ID, &[0x20]);
        let mut stream = vec![0x90, 0x40, 0x7f]; // junk Note On
        stream.extend_from_slice(&a);
        stream.push(0xFE); // active sensing
        stream.extend_from_slice(&b);

        let mut framer = Framer::new();
        let msgs = framer.push(&stream);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs.first().map(Vec::as_slice), Some(a.as_slice()));
        assert_eq!(msgs.get(1).map(Vec::as_slice), Some(b.as_slice()));
    }
}
