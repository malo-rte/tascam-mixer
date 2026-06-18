//! Human-readable units for control values: formatting ([`format()`]) and
//! parsing ([`parse()`]) in dB / Hz / ms / pan, shared by the CLI and GUI so both speak
//! the same units. Controls without a special unit fall through to a bare
//! integer (e.g. the EQ Q index).
//!
//! The dB conventions are simple linear offsets of the raw control value (e.g.
//! the fader's `raw - 127`), matching what the GUI displays. The EQ band
//! frequencies are log-mapped over indicative ranges (only the LOW band's
//! 32 Hz-1.6 kHz span is manual-confirmed; see the control catalog in
//! `docs/user-manual.adoc`).
//!
//! Casts here are between small, in-range control values and `f64`; the
//! precision loss is immaterial and the truncation matches the raw integer
//! domain.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::control::{Control, Kind};

/// The `(max_raw, lo_hz, hi_hz)` log-frequency mapping for an EQ frequency
/// control, or `None` for any other control.
fn freq_range(control: Control) -> Option<(i32, f64, f64)> {
    match control {
        Control::EqLowFreq => Some((31, 32.0, 1600.0)),
        Control::EqMidLowFreq => Some((63, 100.0, 3200.0)),
        Control::EqMidHighFreq => Some((63, 500.0, 8000.0)),
        Control::EqHighFreq => Some((31, 1600.0, 16000.0)),
        _ => None,
    }
}

/// The centre (peaking) or corner (shelf) frequency in Hz of an EQ frequency
/// control at its raw value, or `None` for any other control.
#[must_use]
pub fn freq_hz(control: Control, raw: i32) -> Option<f64> {
    freq_range(control).map(|(max_raw, lo, hi)| {
        let t = (f64::from(raw) / f64::from(max_raw)).clamp(0.0, 1.0);
        lo * (hi / lo).powf(t)
    })
}

/// The numeric value of a control in its display unit: dB for the faders, EQ
/// gains, and compressor threshold/gain; Hz for EQ frequencies; ms for the
/// compressor attack/release; signed percent for pan (negative = left). For a
/// control with no special unit the raw value is returned unchanged.
#[must_use]
pub fn to_unit(control: Control, raw: i32) -> f64 {
    if let Some(hz) = freq_hz(control, raw) {
        return hz;
    }
    match control {
        Control::LineVolume | Control::MasterVolume => f64::from(raw - 127),
        Control::EqLowVolume
        | Control::EqMidLowVolume
        | Control::EqMidHighVolume
        | Control::EqHighVolume => f64::from(raw - 12),
        Control::CompThreshold => f64::from(raw - 32),
        Control::CompAttack => f64::from(raw + 2),
        Control::CompRelease => f64::from((raw + 1) * 10),
        Control::Pan => f64::from(raw - 127) * 100.0 / 127.0,
        // CompGain and the Q indices read out as their raw value.
        _ => f64::from(raw),
    }
}

/// Inverse of [`to_unit`]: the raw control value for a value in display units,
/// rounded and clamped to the control's range.
#[must_use]
pub fn from_unit(control: Control, value: f64) -> i32 {
    let raw = if let Some((max_raw, lo, hi)) = freq_range(control) {
        let hz = if value <= 0.0 { lo } else { value };
        f64::from(max_raw) * (hz / lo).ln() / (hi / lo).ln()
    } else {
        match control {
            Control::LineVolume | Control::MasterVolume => value + 127.0,
            Control::EqLowVolume
            | Control::EqMidLowVolume
            | Control::EqMidHighVolume
            | Control::EqHighVolume => value + 12.0,
            Control::CompThreshold => value + 32.0,
            Control::CompAttack => value - 2.0,
            Control::CompRelease => value / 10.0 - 1.0,
            Control::Pan => value / 100.0 * 127.0 + 127.0,
            _ => value,
        }
    };
    let rounded = raw.round() as i32;
    if let Kind::Int { min, max, .. } = control.kind() {
        rounded.clamp(min, max)
    } else {
        rounded
    }
}

/// Format a raw control value in its display unit (e.g. `+3 dB`, `1.2 kHz`,
/// `200 ms`, `L50%`). Controls with no special unit format as a bare integer.
#[must_use]
pub fn format(control: Control, raw: i32) -> String {
    if freq_range(control).is_some() {
        return freq_hz(control, raw).map_or_else(|| raw.to_string(), format_hz);
    }
    match control {
        Control::LineVolume | Control::MasterVolume => format!("{:+} dB", raw - 127),
        Control::EqLowVolume
        | Control::EqMidLowVolume
        | Control::EqMidHighVolume
        | Control::EqHighVolume => format!("{:+} dB", raw - 12),
        Control::CompThreshold => format!("{} dB", raw - 32),
        Control::CompGain => format!("+{raw} dB"),
        Control::CompAttack => format!("{} ms", raw + 2),
        Control::CompRelease => format!("{} ms", (raw + 1) * 10),
        Control::Pan => format_pan(raw),
        _ => raw.to_string(),
    }
}

/// Parse a human-units string back to a raw control value, accepting the forms
/// [`format()`] produces (with or without the unit suffix, `k`/`kHz` for
/// kilohertz, `C`/`L..%`/`R..%` for pan) plus a bare number. Returns `None` if
/// the text cannot be parsed. Intended for `Int`-kind controls.
#[must_use]
pub fn parse(control: Control, text: &str) -> Option<i32> {
    let text = text.trim();
    if matches!(control, Control::Pan) {
        return parse_pan(text);
    }
    let lower = text.to_ascii_lowercase();
    let head = lower.trim_start_matches('+');
    let number: String = head
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
        .collect();
    let value: f64 = number.parse().ok()?;
    if freq_range(control).is_some() {
        let hz = if lower.contains('k') {
            value * 1000.0
        } else {
            value
        };
        return Some(from_unit(control, hz));
    }
    Some(from_unit(control, value))
}

/// Format a frequency in Hz / kHz.
fn format_hz(hz: f64) -> String {
    if hz >= 1000.0 {
        format!("{:.1} kHz", hz / 1000.0)
    } else {
        format!("{hz:.0} Hz")
    }
}

/// Format a raw pan value (0..=254, 127 centred) as `C` / `L..%` / `R..%`.
fn format_pan(raw: i32) -> String {
    let offset = raw - 127;
    if offset == 0 {
        return "C".to_owned();
    }
    let percent = (offset.abs() * 100 + 63) / 127;
    if offset < 0 {
        format!("L{percent}%")
    } else {
        format!("R{percent}%")
    }
}

/// Parse a pan value (`C`, `L50%`, `R50`, or a bare signed percent) into the raw
/// 0..=254 control value.
fn parse_pan(text: &str) -> Option<i32> {
    if text.eq_ignore_ascii_case("c") {
        return Some(127);
    }
    let (sign, rest) = match text.chars().next() {
        Some('l' | 'L') => (-1.0, &text[1..]),
        Some('r' | 'R') => (1.0, &text[1..]),
        // A bare number is taken as a signed percent (negative = left).
        _ => return Some(from_unit(Control::Pan, text.parse::<f64>().ok()?)),
    };
    let percent: f64 = rest.trim().trim_end_matches('%').trim().parse().ok()?;
    Some(from_unit(Control::Pan, sign * percent))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn fader_db_round_trips() {
        assert_eq!(format(Control::MasterVolume, 127), "+0 dB");
        assert_eq!(format(Control::MasterVolume, 133), "+6 dB");
        assert_eq!(parse(Control::MasterVolume, "+6 dB"), Some(133));
        assert_eq!(parse(Control::MasterVolume, "-7"), Some(120));
    }

    #[test]
    fn eq_gain_is_db_offset_and_clamps() {
        // +6 dB -> raw 18; the band maxes at +12 dB (raw 24).
        assert_eq!(parse(Control::EqLowVolume, "6"), Some(18));
        assert_eq!(format(Control::EqLowVolume, 18), "+6 dB");
        assert_eq!(parse(Control::EqLowVolume, "20"), Some(24));
        assert_eq!(parse(Control::EqLowVolume, "-6 dB"), Some(6));
    }

    #[test]
    fn comp_release_is_milliseconds() {
        assert_eq!(format(Control::CompRelease, 0), "10 ms");
        assert_eq!(parse(Control::CompRelease, "200 ms"), Some(19));
        assert_eq!(format(Control::CompRelease, 19), "200 ms");
    }

    #[test]
    fn eq_freq_is_hertz() {
        assert_eq!(format(Control::EqLowFreq, 0), "32 Hz");
        assert_eq!(format(Control::EqLowFreq, 31), "1.6 kHz");
        // Round-trip a kHz input back to a raw index.
        let raw = parse(Control::EqLowFreq, "1.6kHz").unwrap();
        assert!((raw - 31).abs() <= 1, "raw={raw}");
    }

    #[test]
    fn pan_is_signed_percent() {
        assert_eq!(format(Control::Pan, 127), "C");
        assert_eq!(parse(Control::Pan, "C"), Some(127));
        assert_eq!(parse(Control::Pan, "L100%"), Some(0));
        assert_eq!(parse(Control::Pan, "R100%"), Some(254));
        // A bare positive number is a right percent.
        assert_eq!(
            format(Control::Pan, parse(Control::Pan, "100").unwrap()),
            "R100%"
        );
    }

    #[test]
    fn plain_int_has_no_unit() {
        assert_eq!(format(Control::EqMidLowQ, 3), "3");
        assert_eq!(parse(Control::EqMidLowQ, "3"), Some(3));
    }
}
