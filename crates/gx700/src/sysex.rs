//! Pure Roland/BOSS System Exclusive codec, with no I/O.
//!
//! The manufacturer-independent framing ([`Framer`], `SYSEX_START`/`SYSEX_END`)
//! lives in the shared [`rackctl_sysex`] crate and is re-exported here so the rest
//! of this crate can keep using `crate::sysex::Framer`. What remains is Roland-
//! specific: [`build_dt1`] / [`build_rq1`] / [`parse_roland`] and the `ROLAND_*` /
//! model constants — the `F0 41 <dev> 79 <cmd> <addr..> <data..> <checksum> F7`
//! frame with the Roland one-byte checksum.

use crate::error::{Error, Result};
pub use rackctl_sysex::{Framer, SYSEX_END, SYSEX_START};

/// Roland's MIDI manufacturer id.
pub const ROLAND_ID: u8 = 0x41;
/// The GX-700's Roland model id.
pub const GX700_MODEL_ID: u8 = 0x79;
/// Roland "Data Set 1" command: write data at an address (set).
pub const DT1: u8 = 0x12;
/// Roland "Data Request 1" command: request data from an address.
pub const RQ1: u8 = 0x11;

/// Compute the Roland one-byte checksum over `body` (address plus data).
///
/// Roland defines the checksum as `(128 - sum % 128) % 128`. The two's
/// complement identity `(-sum) & 0x7f` computes the same value while staying in
/// `u8`, avoiding any `as` cast.
#[must_use]
pub fn checksum(body: &[u8]) -> u8 {
    body.iter()
        .fold(0u8, |a, &b| a.wrapping_add(b))
        .wrapping_neg()
        & 0x7f
}

/// Build a Roland DT1 (set) message: `F0 41 <dev> 79 12 <addr..> <data..>
/// <checksum> F7`. The checksum covers the address and data bytes.
#[must_use]
pub fn build_dt1(device_id: u8, addr: &[u8], data: &[u8]) -> Vec<u8> {
    build(device_id, DT1, addr, data)
}

/// Build a Roland RQ1 (request) message: `F0 41 <dev> 79 11 <addr..> <size..>
/// <checksum> F7`. The checksum covers the address and size bytes.
#[must_use]
pub fn build_rq1(device_id: u8, addr: &[u8], size: &[u8]) -> Vec<u8> {
    build(device_id, RQ1, addr, size)
}

/// Shared body of the DT1/RQ1 builders.
fn build(device_id: u8, command: u8, addr: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(addr.len() + payload.len());
    body.extend_from_slice(addr);
    body.extend_from_slice(payload);
    let sum = checksum(&body);

    let mut msg = Vec::with_capacity(body.len() + 7);
    msg.push(SYSEX_START);
    msg.push(ROLAND_ID);
    msg.push(device_id);
    msg.push(GX700_MODEL_ID);
    msg.push(command);
    msg.extend_from_slice(&body);
    msg.push(sum);
    msg.push(SYSEX_END);
    msg
}

/// A parsed, checksum-verified Roland message.
///
/// `body` is the address-plus-data region with the trailing checksum stripped;
/// for a DT1 reply this is `<addr..> <data..>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolandMessage {
    /// The device id from the message header.
    pub device_id: u8,
    /// The Roland command byte (e.g. [`DT1`] or [`RQ1`]).
    pub command: u8,
    /// Address plus data, with the checksum verified and removed.
    pub body: Vec<u8>,
}

/// Parse a complete `F0..F7` message as a Roland/BOSS GX-700 frame.
///
/// Validates the `SysEx` framing, the Roland manufacturer id, the GX-700 model
/// id, and the trailing checksum, returning the device id, command, and the
/// checksum-stripped body.
///
/// # Errors
/// [`Error::Sysex`] if the framing is wrong, the manufacturer/model id does not
/// match the GX-700, the message is too short, or the checksum is invalid.
pub fn parse_roland(msg: &[u8]) -> Result<RolandMessage> {
    let inner = msg
        .strip_prefix(&[SYSEX_START])
        .and_then(|m| m.strip_suffix(&[SYSEX_END]))
        .ok_or_else(|| Error::Sysex("message is not framed by F0..F7".to_owned()))?;

    let (&manufacturer, rest) = inner
        .split_first()
        .ok_or_else(|| Error::Sysex("empty sysex message".to_owned()))?;
    if manufacturer != ROLAND_ID {
        return Err(Error::Sysex(format!(
            "manufacturer id {manufacturer:#04x} is not Roland ({ROLAND_ID:#04x})"
        )));
    }

    let (&device_id, rest) = rest
        .split_first()
        .ok_or_else(|| Error::Sysex("missing device id".to_owned()))?;

    let (&model, rest) = rest
        .split_first()
        .ok_or_else(|| Error::Sysex("missing model id".to_owned()))?;
    if model != GX700_MODEL_ID {
        return Err(Error::Sysex(format!(
            "model id {model:#04x} is not the GX-700 ({GX700_MODEL_ID:#04x})"
        )));
    }

    let (&command, rest) = rest
        .split_first()
        .ok_or_else(|| Error::Sysex("missing command byte".to_owned()))?;

    let (&sum, body) = rest
        .split_last()
        .ok_or_else(|| Error::Sysex("missing checksum byte".to_owned()))?;
    let expected = checksum(body);
    if sum != expected {
        return Err(Error::Sysex(format!(
            "checksum mismatch: got {sum:#04x}, expected {expected:#04x}"
        )));
    }

    Ok(RolandMessage {
        device_id,
        command,
        body: body.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn checksum_known_vectors() {
        assert_eq!(checksum(&[0x40]), 0x40);
        assert_eq!(checksum(&[0x7f]), 0x01);
        assert_eq!(checksum(&[0]), 0);
        // Sum 0 across several bytes still checksums to 0.
        assert_eq!(checksum(&[]), 0);
    }

    #[test]
    fn dt1_round_trips_through_parse() {
        let addr = [0x10, 0x20];
        let data = [0x01, 0x02, 0x03];
        let msg = build_dt1(0x00, &addr, &data);

        let parsed = parse_roland(&msg).unwrap();
        assert_eq!(parsed.device_id, 0x00);
        assert_eq!(parsed.command, DT1);
        let mut expected_body = Vec::new();
        expected_body.extend_from_slice(&addr);
        expected_body.extend_from_slice(&data);
        assert_eq!(parsed.body, expected_body);
    }

    #[test]
    fn rq1_round_trips_through_parse() {
        let addr = [0x00, 0x00, 0x00, 0x10];
        let size = [0x00, 0x00, 0x00, 0x04];
        let msg = build_rq1(0x10, &addr, &size);
        let parsed = parse_roland(&msg).unwrap();
        assert_eq!(parsed.device_id, 0x10);
        assert_eq!(parsed.command, RQ1);
    }

    #[test]
    fn parse_rejects_bad_checksum() {
        let mut msg = build_dt1(0, &[0x40], &[0x01]);
        // Corrupt the checksum byte (second from the end).
        let len = msg.len();
        if let Some(byte) = msg.get_mut(len - 2) {
            *byte ^= 0x7f;
        }
        assert!(matches!(parse_roland(&msg), Err(Error::Sysex(_))));
    }

    #[test]
    fn parse_rejects_wrong_manufacturer() {
        let mut msg = build_dt1(0, &[0x40], &[0x01]);
        if let Some(byte) = msg.get_mut(1) {
            *byte = 0x42; // not Roland
        }
        assert!(matches!(parse_roland(&msg), Err(Error::Sysex(_))));
    }
}
