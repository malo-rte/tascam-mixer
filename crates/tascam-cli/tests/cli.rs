//! End-to-end CLI tests, all using `--mock` so they need no hardware and stay
//! deterministic (test-writing-rules TST-8/TST-12).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn tascamctl() -> Command {
    Command::cargo_bin("tascamctl").expect("binary builds")
}

/// Run `save` to `path` (optionally a single strip), asserting success.
fn save_to(path: &str, channel: Option<&str>) {
    let mut cmd = tascamctl();
    cmd.arg("--mock").arg("save").arg(path);
    if let Some(ch) = channel {
        cmd.arg("-c").arg(ch);
    }
    cmd.assert().success();
}

#[test]
fn list_succeeds_and_shows_known_keys() {
    tascamctl()
        .args(["--mock", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mute"))
        .stdout(predicate::str::contains("eq-low-volume"))
        .stdout(predicate::str::contains("master-volume"));
}

#[test]
fn get_returns_seeded_defaults() {
    tascamctl()
        .args(["--mock", "get", "master-volume"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("127"));
    tascamctl()
        .args(["--mock", "get", "mute", "-c", "3"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("false"));
}

#[test]
fn topology_explains_routing() {
    // Backend-independent, so it needs no device or --mock.
    tascamctl()
        .arg("topology")
        .assert()
        .success()
        .stdout(predicate::str::contains("signal flow"))
        .stdout(predicate::str::contains("MASTER"))
        .stdout(predicate::str::contains("Output 1..8"));
}

#[test]
fn info_enum_lists_values() {
    tascamctl()
        .args(["--mock", "info", "comp-ratio"])
        .assert()
        .success()
        .stdout(predicate::str::contains("enum"))
        .stdout(predicate::str::contains("0=1.0:1"))
        .stdout(predicate::str::contains("14=inf:1"));
}

#[test]
fn info_int_shows_range() {
    tascamctl()
        .args(["--mock", "info", "master-volume"])
        .assert()
        .success()
        .stdout(predicate::str::contains("int"))
        .stdout(predicate::str::contains("0..=133"));
}

#[test]
fn info_unknown_control_fails() {
    tascamctl()
        .args(["--mock", "info", "nonsuch"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn get_enum_shows_label() {
    tascamctl()
        .args(["--mock", "get", "route", "-c", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Master Left (0)"));
}

#[test]
fn set_valid_value_succeeds_silently() {
    tascamctl()
        .args(["--mock", "set", "mute", "on", "-c", "3"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn relative_and_toggle_succeed() {
    // `+N` / `-N` on an int and `toggle` on a bool all exit 0. The leading `-`
    // must be taken as the value, not parsed as an option.
    for args in [
        &["--mock", "set", "master-volume", "+5"][..],
        &["--mock", "set", "master-volume", "-5"][..],
        &["--mock", "set", "mute", "toggle", "-c", "2"][..],
    ] {
        tascamctl()
            .args(args)
            .assert()
            .success()
            .stdout(predicate::str::is_empty());
    }
}

#[test]
fn toggle_on_int_fails() {
    tascamctl()
        .args(["--mock", "set", "master-volume", "toggle"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn set_out_of_range_fails() {
    tascamctl()
        .args(["--mock", "set", "eq-low-volume", "999", "-c", "0"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn unknown_control_fails() {
    tascamctl()
        .args(["--mock", "get", "nonsuch"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn setting_the_meter_block_fails() {
    tascamctl()
        .args(["--mock", "set", "meter", "1"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn global_control_rejects_nonzero_channel() {
    tascamctl()
        .args(["--mock", "set", "master-volume", "100", "-c", "1"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn missing_argument_is_a_usage_error() {
    // clap reports usage errors with exit code 2.
    tascamctl()
        .args(["--mock", "set"])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn save_writes_mixer_and_strip_json() {
    let dir = tempfile::tempdir().unwrap();
    let mixer = dir.path().join("mix.json");
    let strip = dir.path().join("strip.json");

    save_to(mixer.to_str().unwrap(), None);
    save_to(strip.to_str().unwrap(), Some("0"));

    let mixer_json = std::fs::read_to_string(&mixer).unwrap();
    assert!(mixer_json.contains("\"kind\": \"mixer\""));
    assert!(mixer_json.contains("master-volume"));
    assert!(mixer_json.contains("channels"));

    let strip_json = std::fs::read_to_string(&strip).unwrap();
    assert!(strip_json.contains("\"kind\": \"strip\""));
    assert!(strip_json.contains("comp-ratio"));
}

#[test]
fn load_strip_requires_a_channel() {
    let dir = tempfile::tempdir().unwrap();
    let strip = dir.path().join("strip.json");
    save_to(strip.to_str().unwrap(), Some("0"));
    let strip = strip.to_str().unwrap();

    // Applying a strip to a channel works.
    tascamctl()
        .args(["--mock", "load", strip, "-c", "5"])
        .assert()
        .success();
    // Without a target channel it is an error.
    tascamctl()
        .args(["--mock", "load", strip])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn load_mixer_rejects_a_channel() {
    let dir = tempfile::tempdir().unwrap();
    let mixer = dir.path().join("mix.json");
    save_to(mixer.to_str().unwrap(), None);
    let mixer = mixer.to_str().unwrap();

    tascamctl()
        .args(["--mock", "load", mixer])
        .assert()
        .success();
    tascamctl()
        .args(["--mock", "load", mixer, "-c", "0"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn load_missing_file_fails() {
    tascamctl()
        .args(["--mock", "load", "/no/such/preset.json"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn meters_prints_all_channels() {
    for extra in [&[][..], &["--raw"][..]] {
        let mut cmd = tascamctl();
        cmd.arg("--mock").arg("meters").args(extra);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("ch1 "))
            .stdout(predicate::str::contains("ch16 "))
            .stdout(predicate::str::contains("master"));
    }
}
