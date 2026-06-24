# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and the project follows
[Semantic Versioning](https://semver.org/). All tools in the workspace share one
version.

## [Unreleased]

### Added

- **GUI** — the mixer now survives the interface being unplugged. While the
  device is gone the controls are hidden behind a centred "Tascam US-16x08 is
  disconnected" notice and a "reconnecting" status; it retries about once a
  second, and on replug reopens the card and re-applies the on-screen mix (which
  the re-enumerated device would otherwise have reset to its defaults).
- **GUI** — the mixer also starts when the card is absent, showing the
  disconnected notice until it appears. On that first connection it applies the
  saved default preset (or, with none saved, reads the device as-is); when the
  card is already running at startup it just reads its current settings.

## [0.1.0] - 2026-06-21

Initial release: a Rust rewrite of the Tascam US-16x08 DSP mixer, as a workspace
of a control-surface library, a command-line tool, and a graphical mixer. Linux
only, via the `snd-usb-audio` driver.

### Added

- **`tascam-us16x08` library** — a typed, UI-agnostic wrapper over the ALSA
  high-level control (HCTL) interface: the full `Control` catalog, `AlsaBackend`
  (default) and an in-memory `MockBackend`, a `Watcher` for external changes,
  level and gain-reduction meter decoding, JSON presets (whole-mixer and
  per-strip), and a shared `units` module for dB / Hz / ms / pan conversions.
- **`tascamctl` command-line tool** — `list`, `topology`, `info`, `get`, `set`,
  `meters`, `monitor`, `watch`, `save`, `load`, and a shared `default` preset.
  Values read and write in display units; `set` accepts absolute, relative
  (`+N`/`-N`), and `toggle` forms.
- **`tascam-mixer` graphical mixer** (egui/eframe) — an always-visible meter
  bridge with per-channel and master mute, a focused channel/pair editor with a
  faithful biquad EQ response and a square compressor transfer graph (with a
  gain-reduction meter), EQ/compressor reset, stereo link with a level-based
  balance, an output panel, an output-routing tab, JSON presets and a shared
  default, persisted interface zoom and window size, keyboard shortcuts (channel
  navigation, `m`/`M` mute, Esc/Q to quit), and a stable Wayland `app_id`.
- **User manual** (`docs/user-manual.adoc`) covering both tools and the verified
  signal chain, renderable to PDF.

### Notes

- The tools drive the DSP mixer control surface only; they do not stream audio.
  Capture to the computer is taken pre-DSP (the dry input).

[Unreleased]: https://github.com/malo-rte/tascam-mixer/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/malo-rte/tascam-mixer/releases/tag/v0.1.0
