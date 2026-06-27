//! Pure compressor-transfer math, shared by the device GUIs' curve plots.

/// Compressor output level (dB) for an input level (dB): unity below `threshold_db`,
/// `ratio`-compressed above it, then `makeup_db` make-up gain.
#[must_use]
pub fn output_db(input_db: f64, threshold_db: f64, ratio: f64, makeup_db: f64) -> f64 {
    let shaped = if input_db <= threshold_db {
        input_db
    } else {
        threshold_db + (input_db - threshold_db) / ratio
    };
    shaped + makeup_db
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn transfer() {
        // Below threshold: unity (no make-up).
        assert!(close(output_db(-40.0, -20.0, 4.0, 0.0), -40.0));
        // 8 dB over a -20 dB threshold at 4:1 -> 2 dB over -> -18 dB.
        assert!(close(output_db(-12.0, -20.0, 4.0, 0.0), -18.0));
        // inf:1 limits to the threshold.
        assert!(close(output_db(0.0, -20.0, f64::INFINITY, 0.0), -20.0));
        // Make-up gain adds.
        assert!(close(output_db(-40.0, -20.0, 4.0, 3.0), -37.0));
    }
}
