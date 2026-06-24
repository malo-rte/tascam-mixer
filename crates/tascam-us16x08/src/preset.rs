//! Capturing and restoring mixer / channel-strip state as presets.
//!
//! A [`Preset`] is a serde-(de)serializable snapshot of control values. Two
//! granularities, distinguished by a `kind` tag so a file is self-describing:
//!
//! - **Strip** — one channel's per-channel controls (fader, mute, pan, phase,
//!   EQ, compressor), stored without a channel index so it can be applied to
//!   any channel.
//! - **Mixer** — the global master controls, the per-output routing, and all 16
//!   channel strips.
//!
//! File and format handling lives in the caller (e.g. the CLI); this module only
//! turns device state into a [`Preset`] and back.

use std::collections::BTreeMap;
use std::thread::sleep;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::backend::Backend;
use crate::control::{Control, Kind, NUM_CHANNELS, NUM_OUTPUTS, Scope, Value};
use crate::device::Us16x08;
use crate::error::{Error, Result};

/// Schema version written into every preset, for forward compatibility.
pub const PRESET_VERSION: u32 = 1;

/// A single control value as stored in a preset. Enums are kept as their label
/// for readability; integers and booleans use their native JSON types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    /// A boolean switch value.
    Bool(bool),
    /// An integer value.
    Int(i64),
    /// An enum value, stored as its label.
    Text(String),
}

/// Map of control key ([`Control::cli_key`]) to value.
type ControlMap = BTreeMap<String, Scalar>;

/// A single resolved write: control, element index, and value.
type Target = (Control, u32, Value);

/// A saved snapshot of mixer state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Preset {
    /// One channel's per-channel controls, applicable to any channel.
    Strip {
        /// Schema version ([`PRESET_VERSION`]).
        version: u32,
        /// Per-channel control values, keyed by [`Control::cli_key`].
        controls: ControlMap,
    },
    /// The whole mixer: master globals, routing, and every channel strip.
    Mixer {
        /// Schema version ([`PRESET_VERSION`]).
        version: u32,
        /// Global master controls.
        master: ControlMap,
        /// Routing source per line output, keyed by output index (`"0".."7"`).
        route: BTreeMap<String, Scalar>,
        /// One control map per input channel.
        channels: Vec<ControlMap>,
    },
}

/// Outcome of applying a preset.
#[derive(Debug, Default, Clone)]
pub struct ApplyReport {
    /// How many control values were written.
    pub applied: usize,
    /// Keys that were skipped because the device lacks the control or the key is
    /// unknown (e.g. a preset from a different version).
    pub skipped: Vec<String>,
}

/// Timing policy for a robust load ([`Us16x08::apply_muted`] /
/// [`Us16x08::restore_values_muted`]). All three default to zero for tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadTiming {
    /// Pause after each individual control write. Sending the whole mixer
    /// back-to-back can outrun the device's USB control channel, so it silently
    /// drops writes; a small pace (e.g. 10 ms) lets it keep up.
    pub pace: Duration,
    /// How many times to restart the body from muting the master if a write
    /// errors.
    pub rounds: u32,
    /// Pause between restart attempts.
    pub settle: Duration,
}

fn to_scalar(control: Control, value: Value) -> Scalar {
    match value {
        Value::Bool(b) => Scalar::Bool(b),
        Value::Int(i) => Scalar::Int(i64::from(i)),
        Value::Enum(i) => {
            if let Kind::Enum { values, .. } = control.kind() {
                if let Some(label) = usize::try_from(i).ok().and_then(|n| values.get(n)) {
                    return Scalar::Text((*label).to_owned());
                }
            }
            Scalar::Int(i64::from(i))
        }
    }
}

fn from_scalar(control: Control, scalar: &Scalar) -> Result<Value> {
    let key = control.cli_key();
    match control.kind() {
        Kind::Bool => match scalar {
            Scalar::Bool(b) => Ok(Value::Bool(*b)),
            _ => Err(Error::Preset(format!("{key}: expected a boolean"))),
        },
        Kind::Int { .. } => match scalar {
            Scalar::Int(n) => i32::try_from(*n)
                .map(Value::Int)
                .map_err(|_| Error::Preset(format!("{key}: value {n} out of range"))),
            _ => Err(Error::Preset(format!("{key}: expected an integer"))),
        },
        Kind::Enum { values, .. } => match scalar {
            Scalar::Text(s) => values
                .iter()
                .position(|v| v.eq_ignore_ascii_case(s))
                .and_then(|i| i32::try_from(i).ok())
                .map(Value::Enum)
                .ok_or_else(|| Error::Preset(format!("{key}: unknown value {s:?}"))),
            Scalar::Int(n) => i32::try_from(*n)
                .map(Value::Enum)
                .map_err(|_| Error::Preset(format!("{key}: value {n} out of range"))),
            Scalar::Bool(_) => Err(Error::Preset(format!("{key}: expected an enum value"))),
        },
        Kind::Meter => Err(Error::Preset(format!("{key}: meter is not settable"))),
    }
}

impl<B: Backend> Us16x08<B> {
    /// Capture one channel's per-channel controls as a [`Preset::Strip`].
    ///
    /// # Errors
    /// Propagates backend read errors.
    pub fn capture_strip(&self, channel: u32) -> Result<Preset> {
        Ok(Preset::Strip {
            version: PRESET_VERSION,
            controls: self.strip_map(channel)?,
        })
    }

    /// Capture the whole mixer (master globals, routing, all channel strips) as a
    /// [`Preset::Mixer`].
    ///
    /// # Errors
    /// Propagates backend read errors.
    pub fn capture_mixer(&self) -> Result<Preset> {
        let mut master = ControlMap::new();
        for &control in Control::ALL {
            if control.scope() == Scope::Global
                && !matches!(control.kind(), Kind::Meter)
                && self.is_present(control)
            {
                master.insert(
                    control.cli_key().to_owned(),
                    to_scalar(control, self.get(control, 0)?),
                );
            }
        }

        let mut route = BTreeMap::new();
        if self.is_present(Control::LineOutRoute) {
            for out in 0..NUM_OUTPUTS {
                let value = self.get(Control::LineOutRoute, out)?;
                route.insert(out.to_string(), to_scalar(Control::LineOutRoute, value));
            }
        }

        let mut channels = Vec::with_capacity(NUM_CHANNELS as usize);
        for ch in 0..NUM_CHANNELS {
            channels.push(self.strip_map(ch)?);
        }

        Ok(Preset::Mixer {
            version: PRESET_VERSION,
            master,
            route,
            channels,
        })
    }

    /// Apply a preset. A [`Preset::Strip`] requires a target `channel`; a
    /// [`Preset::Mixer`] must not be given one.
    ///
    /// Controls absent from this device, and keys it does not recognise, are
    /// skipped and recorded in the returned [`ApplyReport`].
    ///
    /// # Errors
    /// [`Error::Preset`] on a kind/channel mismatch or a value that does not fit
    /// its control; otherwise backend write errors.
    pub fn apply(&mut self, preset: &Preset, channel: Option<u32>) -> Result<ApplyReport> {
        let (targets, skipped) = self.targets(preset, channel)?;
        for &(control, index, value) in &targets {
            self.set(control, index, value)?;
        }
        Ok(ApplyReport {
            applied: targets.len(),
            skipped,
        })
    }

    /// Apply a preset robustly, for use when the device may not yet take writes
    /// reliably -- in particular right after a USB re-enumeration.
    ///
    /// The load is treated as one transaction. For a whole-mixer preset the
    /// master is muted first, every control is written, then the master mute is
    /// set to the preset's own value (normally unmuted) and confirmed. If any
    /// write *errors*, the whole sequence restarts from muting the master, up to
    /// `rounds` times with `settle` between attempts, so the device is not left
    /// half-applied. Crucially, the master mute is *always* restored to its
    /// target at the end -- even if the body could not be fully written -- so a
    /// failed load never leaves the device silent.
    ///
    /// Writes are not read back per control (the caller is expected to gate on
    /// [`Us16x08::accepts_writes`] first, after which an `Ok` write has taken);
    /// only the master mute is verified, since it is what makes the device audible.
    /// `timing.pace` paces the writes so a slow device is not overrun.
    ///
    /// `timing` is the caller's policy; a zero [`LoadTiming`] applies once with no
    /// waiting. Absent or unknown controls are skipped and reported.
    ///
    /// # Errors
    /// [`Error::Preset`] on a kind/channel mismatch or a value that does not fit
    /// its control; [`Error::Backend`] if the body never wrote cleanly within the
    /// retry budget (the master is still left unmuted).
    pub fn apply_muted(
        &mut self,
        preset: &Preset,
        channel: Option<u32>,
        timing: LoadTiming,
    ) -> Result<ApplyReport> {
        let (targets, skipped) = self.targets(preset, channel)?;
        // Only a whole-mixer load brackets the master mute; a single strip must
        // not touch the master.
        let bracket =
            matches!(preset, Preset::Mixer { .. }) && self.is_present(Control::MasterMute);
        if self.apply_transaction(&targets, bracket, timing) {
            Ok(ApplyReport {
                applied: targets.len(),
                skipped,
            })
        } else {
            Err(Error::Backend(
                "the device kept rejecting writes; the mixer was not fully applied".to_owned(),
            ))
        }
    }

    /// Restore a list of explicit control values robustly, with the same
    /// transactional master-mute bracketing as [`Self::apply_muted`]. Meters and
    /// controls this device lacks are skipped. The master mute ends at whatever
    /// value the list carries for it (absent: left unmuted). Returns whether the
    /// whole sequence completed cleanly within the retry budget.
    ///
    /// Used to push a cached mix back after the interface is replugged.
    pub fn restore_values_muted(&mut self, values: &[Target], timing: LoadTiming) -> bool {
        let targets: Vec<Target> = values
            .iter()
            .copied()
            .filter(|&(control, _, _)| {
                !matches!(control.kind(), Kind::Meter) && self.is_present(control)
            })
            .collect();
        let bracket = self.is_present(Control::MasterMute);
        self.apply_transaction(&targets, bracket, timing)
    }

    /// Build the (control, index, value) writes a preset implies, recording keys
    /// that are skipped (unknown or absent on this device).
    fn targets(&self, preset: &Preset, channel: Option<u32>) -> Result<(Vec<Target>, Vec<String>)> {
        let mut targets = Vec::new();
        let mut skipped = Vec::new();
        match preset {
            Preset::Strip { controls, .. } => {
                let ch = channel.ok_or_else(|| {
                    Error::Preset("strip preset requires a target channel".to_owned())
                })?;
                self.collect_map(controls, ch, &mut targets, &mut skipped)?;
            }
            Preset::Mixer {
                master,
                route,
                channels,
                ..
            } => {
                if channel.is_some() {
                    return Err(Error::Preset(
                        "mixer preset cannot target a single channel".to_owned(),
                    ));
                }
                self.collect_map(master, 0, &mut targets, &mut skipped)?;
                for (out_key, scalar) in route {
                    if self.is_present(Control::LineOutRoute) {
                        let out: u32 = out_key.parse().map_err(|_| {
                            Error::Preset(format!("invalid route index {out_key:?}"))
                        })?;
                        targets.push((
                            Control::LineOutRoute,
                            out,
                            from_scalar(Control::LineOutRoute, scalar)?,
                        ));
                    } else {
                        skipped.push(format!("route[{out_key}]"));
                    }
                }
                for (i, map) in channels.iter().enumerate() {
                    let ch = u32::try_from(i)
                        .map_err(|_| Error::Preset("too many channels in preset".to_owned()))?;
                    self.collect_map(map, ch, &mut targets, &mut skipped)?;
                }
            }
        }
        Ok((targets, skipped))
    }

    /// Apply `targets` as a transaction. The body (mute + every other control) is
    /// written and, on any write *error*, restarted from the top up to
    /// `timing.rounds` times with `timing.settle` between attempts. The master
    /// mute is then *always* restored to its target and confirmed -- even if the
    /// body never wrote cleanly -- so a failed load never leaves the device
    /// silent. Returns whether the body wrote cleanly.
    fn apply_transaction(&mut self, targets: &[Target], bracket: bool, timing: LoadTiming) -> bool {
        let mut applied = false;
        for round in 0..=timing.rounds {
            if self.write_body(targets, bracket, timing.pace) {
                applied = true;
                break;
            }
            if round < timing.rounds && !timing.settle.is_zero() {
                sleep(timing.settle);
            }
        }
        if bracket {
            let target = targets
                .iter()
                .find(|&&(control, _, _)| control == Control::MasterMute)
                .map_or(Value::Bool(false), |&(_, _, value)| value);
            self.confirm(Control::MasterMute, 0, target, timing.rounds, timing.settle);
        }
        applied
    }

    /// Mute the master (when `bracket`), then write every other target, pausing
    /// `pace` after each write so a slow device is not overrun. Returns `false`
    /// at the first write that errors, so the caller restarts the whole sequence
    /// (re-muting first). Writes are not read back here: the caller gates on
    /// [`Us16x08::accepts_writes`] first, so an `Ok` write has taken.
    fn write_body(&mut self, targets: &[Target], bracket: bool, pace: Duration) -> bool {
        if bracket {
            if self.set(Control::MasterMute, 0, Value::Bool(true)).is_err() {
                return false;
            }
            if !pace.is_zero() {
                sleep(pace);
            }
        }
        for &(control, index, value) in targets {
            if control == Control::MasterMute {
                continue;
            }
            if self.set(control, index, value).is_err() {
                return false;
            }
            if !pace.is_zero() {
                sleep(pace);
            }
        }
        true
    }

    /// Set a control and confirm it reads back, retrying up to `rounds` times
    /// with `settle` between. Used for the master mute, whose final state must be
    /// right so the device is left audible. The last attempt still issues the
    /// write even if it cannot confirm, so the command reaches the device.
    fn confirm(
        &mut self,
        control: Control,
        index: u32,
        value: Value,
        rounds: u32,
        settle: Duration,
    ) {
        for round in 0..=rounds {
            if self.set(control, index, value).is_ok()
                && matches!(self.get(control, index), Ok(read) if read == value)
            {
                return;
            }
            if round < rounds && !settle.is_zero() {
                sleep(settle);
            }
        }
    }

    /// The present, settable per-channel controls at `channel`, as a map.
    fn strip_map(&self, channel: u32) -> Result<ControlMap> {
        let mut map = ControlMap::new();
        for &control in Control::ALL {
            if control.scope() == Scope::Channel
                && !matches!(control.kind(), Kind::Meter)
                && self.is_present(control)
            {
                let value = self.get(control, channel)?;
                map.insert(control.cli_key().to_owned(), to_scalar(control, value));
            }
        }
        Ok(map)
    }

    /// Resolve a control map into `targets`, pushing unknown or absent keys to
    /// `skipped`.
    fn collect_map(
        &self,
        map: &ControlMap,
        index: u32,
        targets: &mut Vec<Target>,
        skipped: &mut Vec<String>,
    ) -> Result<()> {
        for (key, scalar) in map {
            let Some(control) = Control::from_key(key) else {
                skipped.push(key.clone());
                continue;
            };
            if !self.is_present(control) {
                skipped.push(key.clone());
                continue;
            }
            targets.push((control, index, from_scalar(control, scalar)?));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::MockBackend;

    fn dev() -> Us16x08<MockBackend> {
        Us16x08::new(MockBackend::new())
    }

    /// Test timing: `rounds` restart attempts, no waiting or pacing.
    fn timing(rounds: u32) -> LoadTiming {
        LoadTiming {
            rounds,
            ..LoadTiming::default()
        }
    }

    #[test]
    fn mixer_round_trips_through_a_fresh_device() {
        let mut a = dev();
        a.set(Control::MasterVolume, 0, Value::Int(120)).unwrap();
        a.set(Control::MasterMute, 0, Value::Bool(true)).unwrap();
        a.set(Control::LineOutRoute, 3, Value::Enum(5)).unwrap();
        a.set(Control::EqLowVolume, 7, Value::Int(20)).unwrap();
        a.set(Control::CompRatio, 7, Value::Enum(9)).unwrap();

        let preset = a.capture_mixer().unwrap();

        let mut b = dev();
        let report = b.apply(&preset, None).unwrap();
        assert!(report.skipped.is_empty());
        assert!(report.applied > 0);

        assert_eq!(b.get(Control::MasterVolume, 0).unwrap(), Value::Int(120));
        assert_eq!(b.get(Control::MasterMute, 0).unwrap(), Value::Bool(true));
        assert_eq!(b.get(Control::LineOutRoute, 3).unwrap(), Value::Enum(5));
        assert_eq!(b.get(Control::EqLowVolume, 7).unwrap(), Value::Int(20));
        assert_eq!(b.get(Control::CompRatio, 7).unwrap(), Value::Enum(9));
    }

    #[test]
    fn strip_applies_to_a_different_channel() {
        let mut a = dev();
        a.set(Control::Pan, 2, Value::Int(200)).unwrap();
        a.set(Control::MuteSwitch, 2, Value::Bool(true)).unwrap();
        let strip = a.capture_strip(2).unwrap();

        let mut b = dev();
        b.apply(&strip, Some(5)).unwrap();
        assert_eq!(b.get(Control::Pan, 5).unwrap(), Value::Int(200));
        assert_eq!(b.get(Control::MuteSwitch, 5).unwrap(), Value::Bool(true));
        // Channel 2 on the fresh device is untouched (still the default).
        assert_eq!(b.get(Control::Pan, 2).unwrap(), Value::Int(127));
    }

    #[test]
    fn serde_json_round_trip() {
        let preset = dev().capture_mixer().unwrap();
        let json = serde_json::to_string(&preset).unwrap();
        let back: Preset = serde_json::from_str(&json).unwrap();
        assert_eq!(preset, back);
        assert!(json.contains("\"kind\":\"mixer\""));
    }

    #[test]
    fn unknown_keys_are_skipped_not_fatal() {
        let mut controls = BTreeMap::new();
        controls.insert("mute".to_owned(), Scalar::Bool(true));
        controls.insert("not-a-control".to_owned(), Scalar::Int(1));
        let preset = Preset::Strip {
            version: PRESET_VERSION,
            controls,
        };

        let mut d = dev();
        let report = d.apply(&preset, Some(0)).unwrap();
        assert_eq!(report.applied, 1);
        assert_eq!(report.skipped, vec!["not-a-control".to_owned()]);
    }

    #[test]
    fn kind_and_channel_mismatches_error() {
        let mut d = dev();
        let strip = d.capture_strip(0).unwrap();
        let mixer = d.capture_mixer().unwrap();
        assert!(d.apply(&strip, None).is_err());
        assert!(d.apply(&mixer, Some(0)).is_err());
    }

    /// A backend that rejects its first `fails` writes, then behaves like the
    /// mock. Stands in for a just-re-enumerated device that refuses early writes.
    struct Flaky {
        inner: MockBackend,
        fails: u32,
    }

    impl Backend for Flaky {
        fn get_int(&self, name: &str, index: u32) -> Result<i32> {
            self.inner.get_int(name, index)
        }
        fn get_bool(&self, name: &str, index: u32) -> Result<bool> {
            self.inner.get_bool(name, index)
        }
        fn get_ints(&self, name: &str, out: &mut [i32]) -> Result<usize> {
            self.inner.get_ints(name, out)
        }
        fn elem_names(&self) -> Vec<String> {
            self.inner.elem_names()
        }
        fn set_int(&mut self, name: &str, index: u32, val: i32) -> Result<()> {
            if self.fails > 0 {
                self.fails -= 1;
                return Err(Error::Backend("flaky".to_owned()));
            }
            self.inner.set_int(name, index, val)
        }
        fn set_bool(&mut self, name: &str, index: u32, val: bool) -> Result<()> {
            if self.fails > 0 {
                self.fails -= 1;
                return Err(Error::Backend("flaky".to_owned()));
            }
            self.inner.set_bool(name, index, val)
        }
    }

    #[test]
    fn apply_muted_retries_until_writes_take() {
        // Build a non-trivial mixer to restore.
        let mut a = dev();
        a.set(Control::MasterVolume, 0, Value::Int(120)).unwrap();
        a.set(Control::EqLowVolume, 7, Value::Int(20)).unwrap();
        a.set(Control::CompRatio, 7, Value::Enum(9)).unwrap();
        // Master ends unmuted in the captured preset.
        a.set(Control::MasterMute, 0, Value::Bool(false)).unwrap();
        let preset = a.capture_mixer().unwrap();

        // The first several writes fail; retries must recover all of them.
        let mut b = Us16x08::new(Flaky {
            inner: MockBackend::new(),
            fails: 12,
        });
        let report = b
            .apply_muted(&preset, None, timing(20))
            .expect("apply_muted");

        assert!(report.skipped.is_empty());
        assert_eq!(b.get(Control::MasterVolume, 0).unwrap(), Value::Int(120));
        assert_eq!(b.get(Control::EqLowVolume, 7).unwrap(), Value::Int(20));
        assert_eq!(b.get(Control::CompRatio, 7).unwrap(), Value::Enum(9));
        // The bracket leaves the master at the preset's value, not stuck muted.
        assert_eq!(b.get(Control::MasterMute, 0).unwrap(), Value::Bool(false));
    }

    /// Counts master-mute-on writes and fails the first body (int) write once,
    /// to prove a mid-sequence failure restarts the whole sequence from muting.
    struct RestartProbe {
        inner: MockBackend,
        fail_first_int: bool,
        mute_on_writes: u32,
    }

    impl Backend for RestartProbe {
        fn get_int(&self, name: &str, index: u32) -> Result<i32> {
            self.inner.get_int(name, index)
        }
        fn get_bool(&self, name: &str, index: u32) -> Result<bool> {
            self.inner.get_bool(name, index)
        }
        fn get_ints(&self, name: &str, out: &mut [i32]) -> Result<usize> {
            self.inner.get_ints(name, out)
        }
        fn elem_names(&self) -> Vec<String> {
            self.inner.elem_names()
        }
        fn set_int(&mut self, name: &str, index: u32, val: i32) -> Result<()> {
            if self.fail_first_int {
                self.fail_first_int = false;
                return Err(Error::Backend("flaky".to_owned()));
            }
            self.inner.set_int(name, index, val)
        }
        fn set_bool(&mut self, name: &str, index: u32, val: bool) -> Result<()> {
            if name == "Master Mute Switch" && val {
                self.mute_on_writes += 1;
            }
            self.inner.set_bool(name, index, val)
        }
    }

    #[test]
    fn a_mid_sequence_failure_restarts_from_muting() {
        let mut a = dev();
        a.set(Control::MasterVolume, 0, Value::Int(120)).unwrap();
        a.set(Control::MasterMute, 0, Value::Bool(false)).unwrap();
        let preset = a.capture_mixer().unwrap();

        let mut b = Us16x08::new(RestartProbe {
            inner: MockBackend::new(),
            fail_first_int: true,
            mute_on_writes: 0,
        });
        b.apply_muted(&preset, None, timing(5))
            .expect("apply_muted");

        // First attempt: mute, then the first int write fails -> abort. Second
        // attempt: mute again and complete. So the master was muted twice.
        assert!(
            b.backend().mute_on_writes >= 2,
            "muted {} time(s)",
            b.backend().mute_on_writes
        );
        assert_eq!(b.get(Control::MasterVolume, 0).unwrap(), Value::Int(120));
        assert_eq!(b.get(Control::MasterMute, 0).unwrap(), Value::Bool(false));
    }

    /// Lets boolean writes (the master mute) through but fails every integer
    /// write, so the body can never complete -- to prove the master is still
    /// left unmuted.
    struct BodyAlwaysFails {
        inner: MockBackend,
    }

    impl Backend for BodyAlwaysFails {
        fn get_int(&self, name: &str, index: u32) -> Result<i32> {
            self.inner.get_int(name, index)
        }
        fn get_bool(&self, name: &str, index: u32) -> Result<bool> {
            self.inner.get_bool(name, index)
        }
        fn get_ints(&self, name: &str, out: &mut [i32]) -> Result<usize> {
            self.inner.get_ints(name, out)
        }
        fn elem_names(&self) -> Vec<String> {
            self.inner.elem_names()
        }
        fn set_int(&mut self, _name: &str, _index: u32, _val: i32) -> Result<()> {
            Err(Error::Backend("int writes always fail".to_owned()))
        }
        fn set_bool(&mut self, name: &str, index: u32, val: bool) -> Result<()> {
            self.inner.set_bool(name, index, val)
        }
    }

    #[test]
    fn a_failed_body_still_leaves_the_master_unmuted() {
        let mut a = dev();
        a.set(Control::MasterMute, 0, Value::Bool(false)).unwrap();
        a.set(Control::MasterVolume, 0, Value::Int(120)).unwrap();
        let preset = a.capture_mixer().unwrap();

        let mut b = Us16x08::new(BodyAlwaysFails {
            inner: MockBackend::new(),
        });
        // The body never writes cleanly, so the load reports an error...
        assert!(b.apply_muted(&preset, None, timing(2)).is_err());
        // ...but the master is left unmuted, never stuck silent.
        assert_eq!(b.get(Control::MasterMute, 0).unwrap(), Value::Bool(false));
    }

    #[test]
    fn accepts_writes_reports_write_readiness() {
        // A healthy device accepts writes immediately, and the probe leaves the
        // master mute as it found it (unmuted by default).
        let mut ok = dev();
        assert!(ok.accepts_writes());
        assert_eq!(ok.get(Control::MasterMute, 0).unwrap(), Value::Bool(false));

        // A device that rejects its first writes is not ready until they land.
        let mut flaky = Us16x08::new(Flaky {
            inner: MockBackend::new(),
            fails: 2,
        });
        assert!(!flaky.accepts_writes());
        assert!(flaky.accepts_writes());
    }

    #[test]
    fn restore_values_muted_pushes_a_value_list() {
        let values = [
            (Control::MasterVolume, 0, Value::Int(100)),
            (Control::Pan, 4, Value::Int(200)),
            (Control::MuteSwitch, 4, Value::Bool(true)),
            (Control::MasterMute, 0, Value::Bool(false)),
        ];
        let mut d = dev();
        assert!(d.restore_values_muted(&values, timing(4)));
        assert_eq!(d.get(Control::MasterVolume, 0).unwrap(), Value::Int(100));
        assert_eq!(d.get(Control::Pan, 4).unwrap(), Value::Int(200));
        assert_eq!(d.get(Control::MuteSwitch, 4).unwrap(), Value::Bool(true));
        assert_eq!(d.get(Control::MasterMute, 0).unwrap(), Value::Bool(false));
    }
}
