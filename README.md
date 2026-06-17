# Tascam US-16x08 mixer — Rust rewrite

This repository is being rewritten in Rust. The device's DSP mixer (faders, EQ,
compressor, routing, metering) is exposed by the Linux `snd-usb-audio` driver as
~280 ALSA control elements; the project drives that surface — there is no PCM
streaming involved.

The work proceeds in three stages:

1. **Library** — [`crates/tascam-us16x08`](crates/tascam-us16x08): a pure,
   UI-agnostic crate wrapping the ALSA control surface. **(done)**
2. **CLI** — [`crates/tascam-cli`](crates/tascam-cli): the `tascamctl`
   command-line tool. **(done)**
3. **GUI** — a graphical mixer. *(planned)*

The original GTKmm C++ application is preserved under
[`legacy/`](legacy/) for reference until the Rust mixer reaches feature parity.

## Workspace layout

```
Cargo.toml                 # Rust workspace (+ shared lint policy)
clippy.toml, deny.toml     # project lint / supply-chain config
crates/
  tascam-us16x08/          # stage 1: the control-surface library
  tascam-cli/              # stage 2: the `tascamctl` command-line tool
legacy/                    # original C++ tascamgtk app (reference)
```

## The CLI (`tascamctl`)

A thin command-line front-end over the library. A global `--mock` flag runs
against the in-memory backend so every command works with no card attached.

```
tascamctl list                       # list all controls (key, scope, kind, ALSA name)
tascamctl topology                   # explain the signal flow and output routing
tascamctl info comp-ratio            # detail one control (scope, range, enum values)
tascamctl get  master-volume         # read a global control
tascamctl get  mute -c 3             # read channel 3's mute
tascamctl set  mute on -c 3          # write it (bool: on/off, true/false, 1/0, yes/no)
tascamctl set  mute toggle -c 3      # flip a boolean
tascamctl set  master-volume +5      # relative: adjust an int, clamped to range
tascamctl set  master-volume -5      # (leading - is taken as the value)
tascamctl set  route "Output 3" -c 0 # enum by label or index
tascamctl meters                     # one-shot meter read (--raw for linear samples)
tascamctl monitor                    # live meters until Ctrl-C
tascamctl watch                      # print control changes as they happen
tascamctl save mix.json              # save the whole mixer to JSON
tascamctl save strip.json -c 0       # save just channel 0's strip
tascamctl load mix.json              # restore the whole mixer
tascamctl load strip.json -c 5       # apply a saved strip to channel 5
```

Presets are JSON. A whole-mixer preset holds the master controls, the routing,
and all 16 channel strips; a strip preset holds one channel's controls and can
be applied to any channel. Controls the device doesn't expose are skipped on
load (reported on stderr).

Run without a device using `cargo run -p tascam-cli -- --mock <command>`. Exit
codes: `0` success, `1` runtime error (card missing, unknown control, value out
of range), `2` usage error.

## The library (`tascam-us16x08`)

A typed wrapper over the ALSA high-level control (HCTL) interface — the Rust port
of the C++ `OAlsa` hardware layer.

- `Control` enumerates every DSP/mixer control with its ALSA name, value kind,
  range, and scope (global / per-channel / per-output).
- `Us16x08<B>` is the device façade, generic over a `Backend`:
  - `AlsaBackend` (feature `alsa`, default) talks to real hardware.
  - `MockBackend` is an in-memory stand-in needing no card or `libasound`, so the
    full surface can be exercised in tests and on CI.
- `Meters` decodes the 34-value level-meter block; `convert` ports the
  fader/meter dB curves; `Watcher` reports external control changes.

### Building and testing

The default build links `libasound` via the `alsa` crate (install
`libasound2-dev` on Debian/Ubuntu, `alsa-lib` on Arch):

```
cargo build
cargo test
```

To build and test without a sound card or ALSA headers (mock backend only):

```
cargo test -p tascam-us16x08 --no-default-features
```

### The full gate

`./run-all-checks.sh` runs the same checks as CI: `cargo fmt --check`, `clippy
-D warnings` and `cargo test` over both feature sets, `rustdoc -D warnings`, and
— when installed — `cargo deny`, `shellcheck`, and `shfmt`. It runs every check
and reports all failures, exiting non-zero if any fails. CI
(`.github/workflows/ci.yml`) additionally builds and tests on the MSRV (1.85).

---

# Legacy C++ application (`legacy/`)

The original GTK+ application. It relies on
[US16x08 support](https://github.com/torvalds/linux/blob/master/sound/usb/mixer_us16x08.c)
in the Linux snd-usb-audio driver (Linux 4.11+).

![screenshot.png](/legacy/screenshot.png?raw=true)

Because this device contains about 280 control elements, working with traditional
mixer applications like alsamixer or gnome-alsamixer was no option, so the author
developed a dedicated mixer for comfortable access to the device's built-in DSP
effects. An [LV2 plugin](https://github.com/onkelDead/tascam.lv2) offers EQ and
compressor control from any LV2-capable DAW.

## Building the legacy app

Build tools: `autoconf`, `automake`, `autopoint`, `make`, `g++`. Libraries:
`libgtkmm-3.0`, `libxml++` (2.6 to 5.0), and optionally `liblo`.

On Debian-based systems:
```
apt install build-essential autoconf automake autopoint libgtkmm-3.0-dev libxml++2.6-dev liblo-dev
```

On Arch-based systems: `base-devel autoconf automake gtkmm3 libxml++-5.0 liblo`

Then, from `legacy/`:
```
autoreconf -fiv
./configure
make
```

See `./configure --help` for options (install prefix, disabling OSC support).
Run `make install` as root to install.
