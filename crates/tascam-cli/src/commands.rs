//! Command handlers, generic over the [`Backend`] so the same logic drives the
//! mock and the real ALSA device.

use std::fs;
use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tascam_us16x08::{
    Backend, Control, Kind, Meters, NUM_CHANNELS, NUM_OUTPUTS, Preset, Scope, Us16x08, Value,
    Watcher,
};

use crate::value::{format_value, parse_value};

/// Print the full control catalog. Backend-independent.
pub(crate) fn list() {
    println!("{:<18} {:<8} {:<14} ALSA NAME", "KEY", "SCOPE", "KIND");
    for &c in Control::ALL {
        println!(
            "{:<18} {:<8} {:<14} {}",
            c.cli_key(),
            scope_str(c.scope()),
            kind_str(c.kind()),
            c.alsa_name()
        );
    }
}

/// Print the card's signal flow and routing model. Backend-independent.
pub(crate) fn topology() {
    print!(
        "\
Tascam US-16x08 signal flow
===========================

  16 inputs --> per channel: phase, EQ, compressor, fader, pan
                                  |
                                  v
                       summed into the stereo MASTER bus (L / R)

  computer playback (USB from the host) --> Output 1..8

  The 8 physical line outputs are the only routing point. Each one
  independently selects ONE source via `set route <src> -c <out>`:

      line out <out 0..8>  <--  Master Left | Master Right | Output 1..8

Notes
-----
  * The 16 input channels are NOT routed individually to outputs; they
    are mixed into the stereo master. Only the 8 outputs are routed, and
    each carries a single source.
  * `Output 1..8` are the computer/DAW playback streams.
  * `dsp-bypass` and `buss-out` affect the DSP path; consult the device
    manual for their exact placement in the chain.
"
    );
}

/// Print detailed metadata for one control. Backend-independent.
pub(crate) fn info(key: &str) -> Result<()> {
    let control = resolve(key)?;
    println!("{}  ({})", control.cli_key(), control.alsa_name());
    println!("  scope: {}", scope_detail(control.scope()));
    match control.kind() {
        Kind::Bool => println!("  kind:  bool (on/off/true/false/1/0/yes/no, or toggle)"),
        Kind::Int { min, max, default } => {
            println!("  kind:  int");
            println!("  range: {min}..={max} (default {default}); relative +N/-N supported");
        }
        Kind::Enum { values, default } => {
            println!("  kind:  enum (default {default})");
            let listed: Vec<String> = values
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{i}={v}"))
                .collect();
            println!("  values: {}", listed.join("  "));
        }
        Kind::Meter => println!("  kind:  meter (read-only; use the `meters` command)"),
        _ => println!("  kind:  ?"),
    }
    Ok(())
}

/// Read and print one control's value.
pub(crate) fn get<B: Backend>(dev: &Us16x08<B>, key: &str, channel: u32) -> Result<()> {
    let control = resolve(key)?;
    if matches!(control.kind(), Kind::Meter) {
        bail!("{key} is the meter block; use the `meters` command");
    }
    let value = dev.get(control, channel)?;
    println!("{}", format_value(control, value));
    Ok(())
}

/// Parse and write one control's value. Silent on success.
///
/// Accepts absolute values (number, `on`/`off`, enum index/label), a relative
/// `+N`/`-N` delta for integer controls (read-modify-write, clamped to range),
/// or `toggle` for booleans.
pub(crate) fn set<B: Backend>(
    dev: &mut Us16x08<B>,
    key: &str,
    raw_value: &str,
    channel: u32,
) -> Result<()> {
    let control = resolve(key)?;
    let value = resolve_value(dev, control, channel, raw_value)?;
    dev.set(control, channel, value)?;
    Ok(())
}

/// Turn the user's value token into a concrete [`Value`], resolving the relative
/// forms (`+N`/`-N`, `toggle`) against the control's current value.
fn resolve_value<B: Backend>(
    dev: &Us16x08<B>,
    control: Control,
    channel: u32,
    raw: &str,
) -> Result<Value> {
    // `toggle` flips a boolean.
    if raw.eq_ignore_ascii_case("toggle") {
        if !matches!(control.kind(), Kind::Bool) {
            bail!("`toggle` is only valid for boolean controls");
        }
        let Value::Bool(current) = dev.get(control, channel)? else {
            bail!("expected a boolean value");
        };
        return Ok(Value::Bool(!current));
    }

    // `+N` / `-N` adjusts an integer relative to its current value, clamped to
    // the control's range.
    if let Kind::Int { min, max, .. } = control.kind() {
        if raw.starts_with('+') || raw.starts_with('-') {
            let delta: i32 = raw
                .parse()
                .map_err(|_| anyhow!("invalid relative amount {raw:?} (expected +N or -N)"))?;
            let Value::Int(current) = dev.get(control, channel)? else {
                bail!("expected an integer value");
            };
            return Ok(Value::Int(current.saturating_add(delta).clamp(min, max)));
        }
    }

    // Otherwise it's an absolute value.
    parse_value(control.kind(), raw)
}

/// Save mixer state to a JSON file: the whole mixer, or one channel's strip if
/// `channel` is given.
pub(crate) fn save<B: Backend>(dev: &Us16x08<B>, path: &str, channel: Option<u32>) -> Result<()> {
    let preset = match channel {
        Some(ch) => dev.capture_strip(ch)?,
        None => dev.capture_mixer()?,
    };
    let json = serde_json::to_string_pretty(&preset).context("serializing preset")?;
    fs::write(path, json).with_context(|| format!("writing {path:?}"))?;
    Ok(())
}

/// Restore mixer state from a JSON file. A mixer preset restores everything (no
/// `channel`); a strip preset is applied to `channel`.
pub(crate) fn load<B: Backend>(
    dev: &mut Us16x08<B>,
    path: &str,
    channel: Option<u32>,
) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {path:?}"))?;
    let preset: Preset =
        serde_json::from_str(&text).with_context(|| format!("parsing {path:?}"))?;
    let report = dev.apply(&preset, channel)?;
    eprintln!("applied {} control(s)", report.applied);
    if !report.skipped.is_empty() {
        eprintln!(
            "skipped {}: {}",
            report.skipped.len(),
            report.skipped.join(", ")
        );
    }
    Ok(())
}

/// Read and print the level meters once.
pub(crate) fn meters<B: Backend>(dev: &Us16x08<B>, raw: bool) -> Result<()> {
    print_meters(&dev.meters()?, raw);
    Ok(())
}

/// Print the level meters repeatedly until interrupted.
pub(crate) fn monitor<B: Backend>(dev: &Us16x08<B>, interval_ms: u64, raw: bool) -> Result<()> {
    let interval = Duration::from_millis(interval_ms);
    loop {
        print_meters(&dev.meters()?, raw);
        println!("---");
        let _ = std::io::stdout().flush();
        sleep(interval);
    }
}

/// Print control changes as they happen, until interrupted.
pub(crate) fn watch<B: Backend>(dev: &Us16x08<B>, interval_ms: u64) -> Result<()> {
    let interval = Duration::from_millis(interval_ms);
    let mut watcher = Watcher::new();
    // Establish the baseline so only subsequent changes are reported.
    watcher.prime(dev)?;
    loop {
        for change in watcher.poll(dev)? {
            println!(
                "{} [{}] = {}",
                change.control.cli_key(),
                change.index,
                format_value(change.control, change.value)
            );
        }
        let _ = std::io::stdout().flush();
        sleep(interval);
    }
}

fn resolve(key: &str) -> Result<Control> {
    Control::from_key(key).with_context(|| format!("unknown control {key:?} (try `list`)"))
}

fn print_meters(m: &Meters, raw: bool) {
    for ch in 0..NUM_CHANNELS {
        let (level, reduction) = if raw {
            (m.channel_raw(ch), m.reduction_raw(ch))
        } else {
            (m.channel_db(ch), m.reduction_db(ch))
        };
        println!(
            "ch{:<2} level={:>6} reduction={:>6}",
            ch + 1,
            level.unwrap_or(0),
            reduction.unwrap_or(0)
        );
    }
    let (left, right) = if raw { m.master_raw() } else { m.master_db() };
    println!("master  L={left:>6} R={right:>6}");
}

fn scope_str(scope: Scope) -> &'static str {
    match scope {
        Scope::Global => "global",
        Scope::Channel => "channel",
        Scope::Output => "output",
        _ => "?",
    }
}

fn scope_detail(scope: Scope) -> String {
    match scope {
        Scope::Global => "global".to_owned(),
        Scope::Channel => format!("channel (0..{NUM_CHANNELS})"),
        Scope::Output => format!("output (0..{NUM_OUTPUTS})"),
        _ => "?".to_owned(),
    }
}

fn kind_str(kind: Kind) -> String {
    match kind {
        Kind::Bool => "bool".to_owned(),
        Kind::Int { min, max, .. } => format!("int {min}..={max}"),
        Kind::Enum { values, .. } => format!("enum[{}]", values.len()),
        Kind::Meter => "meter".to_owned(),
        _ => "?".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use tascam_us16x08::MockBackend;

    fn dev() -> Us16x08<MockBackend> {
        Us16x08::new(MockBackend::new())
    }

    #[test]
    fn relative_int_adds_to_current() {
        // master-volume default is 127.
        let d = dev();
        assert_eq!(
            resolve_value(&d, Control::MasterVolume, 0, "+5").unwrap(),
            Value::Int(132)
        );
        assert_eq!(
            resolve_value(&d, Control::MasterVolume, 0, "-7").unwrap(),
            Value::Int(120)
        );
    }

    #[test]
    fn relative_int_clamps_to_range() {
        // master-volume range is 0..=133, default 127.
        let d = dev();
        assert_eq!(
            resolve_value(&d, Control::MasterVolume, 0, "+100").unwrap(),
            Value::Int(133)
        );
        assert_eq!(
            resolve_value(&d, Control::MasterVolume, 0, "-200").unwrap(),
            Value::Int(0)
        );
    }

    #[test]
    fn absolute_int_has_no_sign() {
        let d = dev();
        assert_eq!(
            resolve_value(&d, Control::EqLowVolume, 0, "20").unwrap(),
            Value::Int(20)
        );
    }

    #[test]
    fn toggle_flips_bool() {
        // mute default is false.
        let d = dev();
        assert_eq!(
            resolve_value(&d, Control::MuteSwitch, 0, "toggle").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            resolve_value(&d, Control::MuteSwitch, 0, "TOGGLE").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn toggle_on_non_bool_errors() {
        let d = dev();
        assert!(resolve_value(&d, Control::MasterVolume, 0, "toggle").is_err());
    }

    #[test]
    fn relative_on_bool_errors() {
        // `+5` is not a valid boolean, so it is rejected rather than silently
        // misread.
        let d = dev();
        assert!(resolve_value(&d, Control::MuteSwitch, 0, "+5").is_err());
    }
}
