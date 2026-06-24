# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and the project follows
[Semantic Versioning](https://semver.org/). All tools in the workspace share one
version.

## [Unreleased]

### Added

- **GUI** — a *Scenes* tab: save the whole mixer as a named snapshot and recall
  it later. Scenes are stored as ordinary preset files in the settings directory
  (no fixed limit), each with Load and Delete; equivalent to a hardware mixer's
  scene memories, kept on the host.

### Fixed

- **GUI** — the stereo-link grouping is now saved inside every whole-mixer preset
  and scene (as an extra `links` field that `tascamctl` ignores), so loading a
  preset, scene, or the default restores which pairs are linked. Previously the
  grouping only travelled with the shared default.

## [0.2.1] - 2026-06-24

### Fixed

- **Reliable, silent full-mixer loads on a settling device.** Loading a whole
  mixer (the GUI replug restore, *Load default* / *Load mixer*, and `tascamctl
  load` / `default`) used to write each control once and ignore failures, so a
  just-re-enumerated card — which answers reads while still silently dropping
  writes — kept its power-up state (often muted), and the restore "did not always"
  take or left the device silent. A full load now waits until a control write
  actually round-trips, then applies the whole mixer as one transaction: mute the
  master, write every control, and finally set the master mute to its loaded
  value. If a write errors, the body restarts from muting the master; and the
  master mute is *always* restored to its target at the end — even if the body
  could not be fully written — so a failed or interrupted load is never left
  silent. (A device that drops in and out on the USB bus can still interrupt a
  load, but it will not be stuck muted.) Each control write is also paced (10 ms
  apart) so sending the whole mixer back-to-back does not outrun the device's USB
  control channel and silently drop values.

## [0.2.0] - 2026-06-24

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

### Changed

- **CLI** — clearer `--help`: a short summary and command list with `-h`, fuller
  per-command detail with `--help`, an examples section, and a readable breakdown
  of the `set` value forms. Help and error output is now colorized on a terminal
  (and stays plain when piped or redirected).

### Documentation

- **Manual** — a recipe for applying the default preset automatically on connect
  via a `udev` rule and a `systemd` service that runs `tascamctl default`.

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

[Unreleased]: https://github.com/malo-rte/tascam-mixer/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/malo-rte/tascam-mixer/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/malo-rte/tascam-mixer/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/malo-rte/tascam-mixer/releases/tag/v0.1.0
