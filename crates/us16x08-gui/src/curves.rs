//! The channel editor's compressor-transfer curve and the raw-control-value to
//! human-unit conversions. The EQ-response biquad math is shared across the GUIs
//! in [`rackctl_ui::eq`]; it is re-exported here so the channel editor's existing
//! `curves::EqBand` / `curves::eq_response_db` paths are unchanged.
//!
//! The band centre-frequency mapping is approximate (only the LOW band's
//! 32 Hz-1.6 kHz span is manual-confirmed), so absolute Hz are indicative. See the
//! control catalog in `docs/user-manual.adoc`.
#![allow(clippy::cast_precision_loss)]

pub(crate) use rackctl_ui::eq::{BandType, EqBand, eq_response_db};

/// Map a raw Q-control index (0..=6) to a Q factor, log-spaced over 0.25..=16.
pub(crate) fn q_value(raw: i32) -> f64 {
    0.25 * (16.0_f64 / 0.25).powf((f64::from(raw) / 6.0).clamp(0.0, 1.0))
}

/// EQ band gain in dB for a raw `*-volume` value (0..=24, 12 = 0 dB).
pub(crate) fn eq_gain_db(raw: i32) -> f64 {
    f64::from(raw - 12)
}

/// Parse a compressor ratio label (`"2.0:1"`, `"inf:1"`) into a numeric ratio.
pub(crate) fn ratio_from_label(label: &str) -> f64 {
    let head = label.split(':').next().unwrap_or("1");
    if head.eq_ignore_ascii_case("inf") {
        f64::INFINITY
    } else {
        head.parse().unwrap_or(1.0)
    }
}

pub(crate) use rackctl_ui::comp::output_db as comp_output_db;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn ratio_parsing() {
        assert!(close(ratio_from_label("2.0:1"), 2.0));
        assert!(ratio_from_label("inf:1").is_infinite());
        assert!(close(ratio_from_label("1.0:1"), 1.0));
    }
}
