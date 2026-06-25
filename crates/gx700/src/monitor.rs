//! A small streaming MIDI message decoder for the `monitor` listener.
//!
//! Feeds raw MIDI bytes and yields one human-readable line per complete message.
//! It tracks running status, collects System Exclusive into `F0..F7`, and lets
//! System Real Time bytes interleave anywhere. Manufacturer-independent: a
//! candidate to lift into a shared crate alongside the `SysEx` codec.

/// Number of data bytes a channel-voice or system-common `status` expects.
fn data_len(status: u8) -> usize {
    match status & 0xF0 {
        0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
        0xC0 | 0xD0 => 1,
        // System common (0xF1..=0xF6).
        _ => match status {
            0xF2 => 2,
            0xF1 | 0xF3 => 1,
            _ => 0,
        },
    }
}

/// Format bytes as space-separated uppercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Name a System Real Time status byte (`0xF8..=0xFF`).
fn realtime_name(byte: u8) -> &'static str {
    match byte {
        0xF8 => "Clock",
        0xFA => "Start",
        0xFB => "Continue",
        0xFC => "Stop",
        0xFE => "Active Sensing",
        0xFF => "Reset",
        _ => "Real Time",
    }
}

/// Describe a channel-voice message from its `status` and `data` bytes.
fn channel_name(status: u8, data: &[u8]) -> String {
    let channel = (status & 0x0F) + 1;
    let d0 = data.first().copied().unwrap_or(0);
    let d1 = data.get(1).copied().unwrap_or(0);
    match status & 0xF0 {
        0x80 => format!("ch{channel:<2} Note Off    note {d0} vel {d1}"),
        0x90 => format!("ch{channel:<2} Note On     note {d0} vel {d1}"),
        0xA0 => format!("ch{channel:<2} Poly AT     note {d0} pres {d1}"),
        0xB0 => format!("ch{channel:<2} CC          ctrl {d0} val {d1}"),
        0xC0 => format!("ch{channel:<2} Program     {d0}"),
        0xD0 => format!("ch{channel:<2} Channel AT  {d0}"),
        0xE0 => {
            let bend = ((i32::from(d1) << 7) | i32::from(d0)) - 8192;
            format!("ch{channel:<2} Pitch Bend  {bend}")
        }
        _ => format!("ch{channel:<2} ?{status:#04x}"),
    }
}

/// Describe a system-common message from its `status` and `data` bytes.
fn common_name(status: u8, data: &[u8]) -> String {
    let d0 = data.first().copied().unwrap_or(0);
    let d1 = data.get(1).copied().unwrap_or(0);
    match status {
        0xF1 => format!("MTC Quarter {d0}"),
        0xF2 => format!("Song Position {}", (i32::from(d1) << 7) | i32::from(d0)),
        0xF3 => format!("Song Select {d0}"),
        0xF6 => "Tune Request".to_owned(),
        _ => format!("System Common {status:#04x}"),
    }
}

/// A streaming decoder: feed bytes, get back decoded message lines.
#[derive(Debug, Default)]
pub struct MidiDecoder {
    /// Current running status, or `0` if none.
    status: u8,
    /// Data bytes collected for the in-progress channel/common message.
    data: Vec<u8>,
    /// `SysEx` accumulator while inside an `F0..F7` message.
    sysex: Vec<u8>,
    /// Whether a `SysEx` message is being collected.
    in_sysex: bool,
}

impl MidiDecoder {
    /// Create an empty decoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw MIDI `bytes`, returning a line for each message completed.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &byte in bytes {
            if byte >= 0xF8 {
                // System Real Time: may appear anywhere, even inside SysEx.
                out.push(realtime_name(byte).to_owned());
            } else if self.in_sysex {
                self.feed_sysex(byte, &mut out);
            } else if byte == 0xF0 {
                self.in_sysex = true;
                self.sysex.clear();
                self.sysex.push(byte);
                self.status = 0;
            } else if byte >= 0x80 {
                self.begin_status(byte, &mut out);
            } else if self.status != 0 {
                // Data byte, using the running status.
                self.data.push(byte);
                self.maybe_emit(&mut out);
            }
        }
        out
    }

    /// Handle a byte while collecting a `SysEx` message.
    fn feed_sysex(&mut self, byte: u8, out: &mut Vec<String>) {
        if byte == 0xF7 {
            self.sysex.push(byte);
            out.push(format!("SysEx [{}]", hex(&self.sysex)));
            self.sysex.clear();
            self.in_sysex = false;
        } else if byte < 0x80 {
            self.sysex.push(byte);
        } else {
            // A status byte cuts an unterminated SysEx short.
            self.in_sysex = false;
            self.sysex.clear();
            self.begin_status(byte, out);
        }
    }

    /// Begin a channel-voice or system-common message at `status`.
    fn begin_status(&mut self, status: u8, out: &mut Vec<String>) {
        self.status = status;
        self.data.clear();
        if (0xF1..=0xF6).contains(&status) && data_len(status) == 0 {
            out.push(common_name(status, &[]));
            self.status = 0;
        }
    }

    /// Emit the current message once enough data bytes have arrived.
    fn maybe_emit(&mut self, out: &mut Vec<String>) {
        if self.data.len() < data_len(self.status) {
            return;
        }
        if (0xF1..=0xF6).contains(&self.status) {
            out.push(common_name(self.status, &self.data));
            self.status = 0; // system common clears running status
        } else {
            out.push(channel_name(self.status, &self.data));
        }
        self.data.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn decodes_common_messages() {
        let mut d = MidiDecoder::new();
        assert_eq!(
            d.push(&[0xB0, 0x07, 0x64]),
            vec!["ch1  CC          ctrl 7 val 100"]
        );
        assert_eq!(d.push(&[0xC2, 0x05]), vec!["ch3  Program     5"]);
        assert_eq!(
            d.push(&[0x92, 0x3C, 0x40]),
            vec!["ch3  Note On     note 60 vel 64"]
        );
    }

    #[test]
    fn handles_running_status() {
        let mut d = MidiDecoder::new();
        // One CC status, then two more value pairs reusing it.
        let out = d.push(&[0xB0, 0x10, 0x01, 0x10, 0x02, 0x10, 0x03]);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|l| l.contains("CC")));
    }

    #[test]
    fn collects_sysex_and_passes_realtime() {
        let mut d = MidiDecoder::new();
        // Active sensing (0xFE) interleaved inside a SysEx must not break it.
        let out = d.push(&[0xF0, 0x41, 0x00, 0xFE, 0x79, 0x12, 0xF7]);
        assert!(out.iter().any(|l| l == "Active Sensing"));
        assert!(
            out.iter()
                .any(|l| l.starts_with("SysEx [F0 41 00 79 12 F7]")),
            "{out:?}"
        );
    }
}
