//! Behavioural tests for the device facade over the in-memory backend.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use tascam_us16x08::{
    Backend, COMP_RATIO_VALUES, Control, Kind, MockBackend, NUM_CHANNELS, ROUTE_VALUES, Scope,
    Us16x08, Value, Watcher,
};

fn dev() -> Us16x08<MockBackend> {
    Us16x08::new(MockBackend::new())
}

#[test]
fn catalog_is_well_formed() {
    for &c in Control::ALL {
        // Name is non-empty and aliases never duplicate the canonical name.
        assert!(!c.alsa_name().is_empty(), "{c:?} has empty name");
        assert!(
            !c.alsa_aliases().contains(&c.alsa_name()),
            "{c:?} alias duplicates canonical name"
        );
        // Int defaults sit within their declared range.
        if let Kind::Int { min, max, default } = c.kind() {
            assert!(min <= max, "{c:?} min > max");
            assert!(
                default >= min && default <= max,
                "{c:?} default out of range"
            );
        }
        if let Kind::Enum { values, default } = c.kind() {
            assert!(!values.is_empty(), "{c:?} empty enum");
            assert!(
                default >= 0 && (default as usize) < values.len(),
                "{c:?} bad enum default"
            );
        }
    }
}

#[test]
fn every_control_is_readable_at_every_index() {
    let dev = dev();
    for &c in Control::ALL {
        if matches!(c.kind(), Kind::Meter) {
            continue;
        }
        for index in 0..c.scope().count() {
            assert!(dev.get(c, index).is_ok(), "{c:?}[{index}] not readable");
        }
    }
}

#[test]
fn bool_roundtrip() {
    let mut dev = dev();
    dev.set(Control::MuteSwitch, 5, Value::Bool(true)).unwrap();
    assert_eq!(dev.get(Control::MuteSwitch, 5).unwrap(), Value::Bool(true));
    dev.set(Control::MuteSwitch, 5, Value::Bool(false)).unwrap();
    assert_eq!(dev.get(Control::MuteSwitch, 5).unwrap(), Value::Bool(false));
}

#[test]
fn int_roundtrip_and_range_check() {
    let mut dev = dev();
    dev.set(Control::EqLowVolume, 0, Value::Int(20)).unwrap();
    assert_eq!(dev.get(Control::EqLowVolume, 0).unwrap(), Value::Int(20));
    // Above max (24) is rejected, and the stored value is unchanged.
    assert!(dev.set(Control::EqLowVolume, 0, Value::Int(25)).is_err());
    assert_eq!(dev.get(Control::EqLowVolume, 0).unwrap(), Value::Int(20));
}

#[test]
fn enum_roundtrip_and_range_check() {
    let mut dev = dev();
    let last = ROUTE_VALUES.len() as i32 - 1;
    dev.set(Control::LineOutRoute, 7, Value::Enum(last))
        .unwrap();
    assert_eq!(
        dev.get(Control::LineOutRoute, 7).unwrap(),
        Value::Enum(last)
    );
    assert!(
        dev.set(Control::LineOutRoute, 0, Value::Enum(last + 1))
            .is_err()
    );
    // Compressor ratio is the other enum.
    assert_eq!(
        Control::CompRatio.kind(),
        Kind::Enum {
            values: COMP_RATIO_VALUES,
            default: 0
        }
    );
}

#[test]
fn type_mismatch_is_rejected() {
    let mut dev = dev();
    assert!(dev.set(Control::MuteSwitch, 0, Value::Int(1)).is_err());
    assert!(dev.set(Control::EqLowVolume, 0, Value::Bool(true)).is_err());
    // The meter block is not a scalar control.
    assert!(dev.get(Control::LevelMeter, 0).is_err());
}

#[test]
fn index_scope_is_enforced() {
    let mut dev = dev();
    // Channel scope: 0..16 valid, 16 invalid.
    assert!(dev.get(Control::Pan, NUM_CHANNELS - 1).is_ok());
    assert!(dev.get(Control::Pan, NUM_CHANNELS).is_err());
    // Output scope: 0..8.
    assert!(dev.get(Control::LineOutRoute, 7).is_ok());
    assert!(dev.get(Control::LineOutRoute, 8).is_err());
    // Global scope: only index 0.
    assert!(dev.get(Control::MasterMute, 0).is_ok());
    assert!(dev.set(Control::MasterMute, 1, Value::Bool(true)).is_err());
}

#[test]
fn meters_default_to_zero_and_select_dsp_channel_works() {
    let mut dev = dev();
    let m = dev.meters().unwrap();
    assert_eq!(m.master_raw(), (0, 0));
    assert_eq!(m.channel_raw(0), Some(0));
    assert_eq!(m.channel_raw(NUM_CHANNELS), None);
    // The DSP-channel select writes the meter element's slot 0.
    dev.select_dsp_channel(Some(2)).unwrap();
    assert_eq!(dev.backend().get_int("Level Meter", 0).unwrap(), 3);
    dev.select_dsp_channel(None).unwrap();
    assert_eq!(dev.backend().get_int("Level Meter", 0).unwrap(), 0);
}

#[test]
fn watcher_reports_only_changes() {
    let mut dev = dev();
    let mut w = Watcher::new();
    w.prime(&dev).unwrap();
    // No change yet.
    assert!(w.poll(&dev).unwrap().is_empty());
    // One change.
    dev.set(Control::PhaseSwitch, 1, Value::Bool(true)).unwrap();
    let changes = w.poll(&dev).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].control, Control::PhaseSwitch);
    assert_eq!(changes[0].index, 1);
    assert_eq!(changes[0].value, Value::Bool(true));
    // Polling again with no further change is empty.
    assert!(w.poll(&dev).unwrap().is_empty());
}

#[test]
fn fresh_watcher_reports_full_surface_then_settles() {
    let dev = dev();
    let mut w = Watcher::new();
    let first = w.poll(&dev).unwrap();
    assert!(!first.is_empty());
    assert!(w.poll(&dev).unwrap().is_empty());
    // It should have visited every non-meter element exactly once.
    let expected: usize = Control::ALL
        .iter()
        .filter(|c| !matches!(c.kind(), Kind::Meter))
        .map(|c| c.scope().count() as usize)
        .sum();
    assert_eq!(first.len(), expected);
    // Scope sanity: a global has 1, a channel control 16.
    assert_eq!(Scope::Global.count(), 1);
    assert_eq!(Scope::Channel.count(), NUM_CHANNELS);
}
