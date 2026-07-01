//! Manufacturer-independent MIDI **System Exclusive framing**.
//!
//! Every MIDI device in the suite needs to reassemble complete `F0..F7` messages
//! from a byte stream that may split a message across reads, interleave running
//! junk (Active Sensing, Note On, …), or truncate a message. [`Framer`] does exactly
//! that and nothing device-specific — the Roland (GX-700) and Digidesign (Eleven
//! Rack) codecs build on it in their own crates.
#![forbid(unsafe_code)]

/// MIDI System Exclusive start-of-message status byte.
pub const SYSEX_START: u8 = 0xF0;
/// MIDI System Exclusive end-of-message status byte.
pub const SYSEX_END: u8 = 0xF7;

/// Accumulates a byte stream and yields complete `F0..F7` System Exclusive
/// messages, manufacturer-independent.
///
/// Bytes seen while not inside a message are ignored. A fresh [`SYSEX_START`]
/// clears any partial buffer, so a truncated message cannot corrupt the next one.
#[derive(Debug, Default, Clone)]
pub struct Framer {
    buf: Vec<u8>,
    in_message: bool,
}

impl Framer {
    /// Create an empty framer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            in_message: false,
        }
    }

    /// Feed `bytes` to the framer, returning every complete `F0..F7` message
    /// that became available. Partial messages are retained for the next call.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for &b in bytes {
            match b {
                SYSEX_START => {
                    // A new start clears any partial message.
                    self.buf.clear();
                    self.buf.push(b);
                    self.in_message = true;
                }
                SYSEX_END if self.in_message => {
                    self.buf.push(b);
                    out.push(std::mem::take(&mut self.buf));
                    self.in_message = false;
                }
                _ if self.in_message => self.buf.push(b),
                _ => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    /// A minimal complete `SysEx` message `F0 <body...> F7`.
    fn msg(body: &[u8]) -> Vec<u8> {
        let mut m = vec![SYSEX_START];
        m.extend_from_slice(body);
        m.push(SYSEX_END);
        m
    }

    #[test]
    fn splits_stream_with_junk_and_two_messages() {
        let a = msg(&[0x41, 0x10, 0xAA]);
        let b = msg(&[0x13, 0x20, 0xBB]);

        let mut stream = Vec::new();
        stream.extend_from_slice(&[0x90, 0x40, 0x7f]); // junk: a Note On, no sysex
        stream.extend_from_slice(&a);
        stream.push(0xFE); // junk: active sensing between messages
        stream.extend_from_slice(&b);

        let mut framer = Framer::new();
        let msgs = framer.push(&stream);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs.first().map(Vec::as_slice), Some(a.as_slice()));
        assert_eq!(msgs.get(1).map(Vec::as_slice), Some(b.as_slice()));
    }

    #[test]
    fn handles_split_across_pushes() {
        let a = msg(&[0x41, 0x10, 0xAA]);
        let (head, tail) = a.split_at(3);
        let mut framer = Framer::new();
        assert!(framer.push(head).is_empty());
        let msgs = framer.push(tail);
        assert_eq!(msgs.first().map(Vec::as_slice), Some(a.as_slice()));
    }

    #[test]
    fn new_start_clears_partial() {
        let a = msg(&[0x41, 0x10, 0xAA]);
        let mut framer = Framer::new();
        // A partial message, then a fresh F0 that should discard it.
        assert!(framer.push(&[SYSEX_START, 0x41, 0x00]).is_empty());
        let msgs = framer.push(&a);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs.first().map(Vec::as_slice), Some(a.as_slice()));
    }
}
