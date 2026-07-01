//! The Eleven Rack parameter catalog — the **MIDI continuous-controller (CC)
//! reference**.
//!
//! Source: the Eleven Rack *User Guide*, Chapter 11 "Controlling Eleven Rack with
//! MIDI", which lists every remote-controllable control as a `NAME → CC#` chart,
//! per amp model and per effect, together with each control's value semantics. This
//! is the authoritative naming + range reference and the canonical per-model
//! parameter list.
//!
//! # CC numbers are *not* `SysEx` addresses
//!
//! The CC here is the number a foot controller or Pro Tools automation sends to
//! move a control — a **separate namespace** from the internal `SysEx` parameter
//! *address*. The `SysEx` address is `11 <block> <index>` where the block byte is
//! *model/slot-specific* and the index is a small *sequential* offset within that
//! block (e.g. a captured amp had Gain at index `07`, Master `09`, … Presence
//! `0D`), bearing no relation to the CC (Gain CC 13, Master CC 10, Presence CC 21).
//! Those addresses are mapped by live capture, not derivable from this table; see
//! the "Parameter catalog" section of `docs/eleven-rack-sysex-protocol.adoc` and
//! the one confirmed example [`crate::AMP_GAIN`]. **Do not** treat a `cc` here as a
//! wire address.
//!
//! # Two parameter families
//!
//! * **Amps** — the manual lists each amp model's own controls and CCs (CC 13 is
//!   `tone` on `tweed-lux` but `presence` on `tweed-bass`). Each [`Amp`] lists its
//!   `(name, cc)` pairs; the amp-section globals (bypass, output, cab/mic) are in
//!   [`AMP_GLOBAL`]. Names are stable **kebab-case** identifiers (as on the CLI).
//! * **Effects** — an effect's CCs depend on *where it sits in the chain*: a
//!   [`Slot::Fixed`] dedicated-block effect (distortion, wah, delay, reverb, FX
//!   loop) has one CC set, while a modulation-type effect occupying **Mod / FX1 /
//!   FX2** has a different CC per slot. Each [`FxParam`] carries a `cc` array
//!   aligned with its [`Effect::slots`]; use [`Effect::cc_in`]. (The per-slot CCs
//!   are transcribed from the unit's on-device MIDI CC Reference — they are *not* a
//!   positional formula: Parametric EQ skips a slot and Multi Chorus's `Lo Cut` /
//!   `Width` share one CC across slots.)
//!
//! Values are **raw 7-bit device units** (`0..=127`). A [`Kind`] says how to read a
//! value: a continuous [`Kind::Knob`], a [`Kind::Switch`] split at 64, or a
//! [`Kind::Stepped`] selector whose inclusive raw ranges map to labels (reverb
//! type, rotary speed, the tempo-sync note divisions).

/// How a raw `0..=127` value is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// A continuous knob over the full `0..=127` range.
    Knob,
    /// A two-state switch: raw `0..=63` reads as `off`, `64..=127` as `on`.
    Switch {
        /// Label for the low half (`0..=63`).
        off: &'static str,
        /// Label for the high half (`64..=127`).
        on: &'static str,
    },
    /// A stepped selector: the first [`Step`] whose range contains the raw value
    /// gives the label (reverb type, rotary speed/type, tempo-sync divisions).
    Stepped(&'static [Step]),
}

impl Kind {
    /// Describe a raw value under this kind, e.g. `"On"`, `"Concert Hall"`, or the
    /// bare number for a plain knob.
    #[must_use]
    pub fn describe(self, raw: u8) -> String {
        match self {
            Kind::Knob => raw.to_string(),
            Kind::Switch { off, on } => (if raw < 64 { off } else { on }).to_string(),
            Kind::Stepped(steps) => steps
                .iter()
                .find(|s| s.contains(raw))
                .map_or_else(|| raw.to_string(), |s| s.label.to_string()),
        }
    }
}

/// One entry of a [`Kind::Stepped`] selector: an inclusive raw range and its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// Display label for this range.
    pub label: &'static str,
    /// Inclusive low raw value.
    pub lo: u8,
    /// Inclusive high raw value.
    pub hi: u8,
}

impl Step {
    /// Whether `raw` falls in this step's inclusive range.
    #[must_use]
    pub const fn contains(self, raw: u8) -> bool {
        raw >= self.lo && raw <= self.hi
    }
}

/// One catalogued amp/global parameter: a display `name`, its MIDI `cc` (the
/// remote-control continuous-controller number — *not* a `SysEx` address), and its
/// value [`Kind`]. (Effects use [`FxParam`], which carries a CC per slot.)
#[derive(Debug, Clone, Copy)]
pub struct Param {
    /// Stable kebab-case control name, for lookup and on the CLI (e.g. `"presence"`,
    /// `"noise-gate-threshold"`).
    pub name: &'static str,
    /// MIDI continuous-controller number for remote control (foot controller /
    /// Pro Tools automation). A separate namespace from the internal `SysEx`
    /// parameter index; see the module docs.
    pub cc: u8,
    /// How to interpret this parameter's raw value.
    pub kind: Kind,
}

const fn knob(name: &'static str, cc: u8) -> Param {
    Param {
        name,
        cc,
        kind: Kind::Knob,
    }
}

const fn onoff(name: &'static str, cc: u8) -> Param {
    Param {
        name,
        cc,
        kind: Kind::Switch {
            off: "Off",
            on: "On",
        },
    }
}

const fn sw(name: &'static str, cc: u8, off: &'static str, on: &'static str) -> Param {
    Param {
        name,
        cc,
        kind: Kind::Switch { off, on },
    }
}

// ----------------------------------------------------------------------------
// Shared stepped-value tables (User Guide Ch.11).
// ----------------------------------------------------------------------------

const fn step(label: &'static str, lo: u8, hi: u8) -> Step {
    Step { label, lo, hi }
}

/// Tempo-sync note divisions (`†FX SYNC Setting Values`), shared by every effect
/// `SYNC` parameter.
pub const FX_SYNC_STEPS: &[Step] = &[
    step("Off", 0, 4),
    step("Whole Note", 5, 14),
    step("Dotted Half Note", 15, 24),
    step("Half Note", 25, 34),
    step("Half Note Triplet", 35, 44),
    step("Dotted Quarter Note", 45, 54),
    step("Quarter Note", 55, 63),
    step("Quarter Note Triplet", 64, 73),
    step("Dotted Eighth Note", 74, 83),
    step("Eighth Note", 84, 93),
    step("Eighth Note Triplet", 94, 103),
    step("Dotted Sixteenth Note", 104, 113),
    step("Sixteenth Note", 114, 123),
    step("Sixteenth Note Triplet", 124, 127),
];

/// Eleven SR (Stereo Reverb) `TYPE` selector.
pub const REVERB_TYPE_STEPS: &[Step] = &[
    step("Echo Room", 0, 2),
    step("Studio", 3, 7),
    step("Small Room", 8, 13),
    step("Jazz Club", 14, 18),
    step("Small Club", 19, 23),
    step("Garage", 24, 29),
    step("Medium Room", 30, 34),
    step("Tiled Room", 35, 39),
    step("Wood Room", 40, 45),
    step("Small Theater", 46, 50),
    step("Medium Theater", 51, 55),
    step("Large Theater", 56, 61),
    step("Rich Hall", 62, 66),
    step("Concert Hall", 67, 71),
    step("Bright Hall", 72, 77),
    step("Church", 78, 82),
    step("Cathedral", 83, 87),
    step("Arena", 88, 93),
    step("Small Plate", 94, 98),
    step("Medium Plate", 99, 103),
    step("Large Plate", 104, 109),
    step("Canyon", 110, 114),
    step("Supa Long", 115, 119),
    step("Early Reflect 1", 120, 125),
    step("Early Reflect 2", 126, 127),
];

/// Roto Speaker `SPEED` selector.
pub const ROTO_SPEED_STEPS: &[Step] = &[
    step("Slow", 0, 31),
    step("Brake", 32, 95),
    step("Fast", 96, 127),
];

/// Roto Speaker `TYPE` selector.
pub const ROTO_TYPE_STEPS: &[Step] = &[
    step("120", 0, 9),
    step("122", 10, 27),
    step("21H", 28, 45),
    step("Foam Drum", 46, 63),
    step("Rover", 64, 82),
    step("Memphis", 83, 100),
    step("Wolf", 101, 118),
    step("Watery", 119, 127),
];

// ----------------------------------------------------------------------------
// Amp section.
// ----------------------------------------------------------------------------

/// One amplifier model and its parameter names, MIDI CCs and value kinds (User
/// Guide Ch.11). Each model has its own control set and CCs.
#[derive(Debug, Clone, Copy)]
pub struct Amp {
    /// Model name as shown on the unit and in the User Guide.
    pub name: &'static str,
    /// The model's named parameters, with their MIDI CC and value kind.
    pub params: &'static [Param],
}

impl Amp {
    /// Find a parameter by case-insensitive name.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// Amp-section controls common to every model ("Applies to all amps").
pub const AMP_GLOBAL: &[Param] = &[
    onoff("amp-bypass", 111),
    knob("amp-output", 92),
    onoff("cab-mic-bypass", 71),
];

/// All catalogued amplifier models (User Guide Ch.11).
pub const AMPS: &[Amp] = &[
    Amp {
        name: "tweed-lux",
        params: &[
            knob("tone", 13),
            knob("instrument-volume", 14),
            knob("mic-volume", 15),
            knob("noise-gate-threshold", 16),
            knob("noise-gate-release", 21),
        ],
    },
    Amp {
        name: "tweed-bass",
        params: &[
            knob("presence", 13),
            knob("middle", 14),
            knob("bass", 15),
            knob("treble", 16),
            knob("bright-volume", 21),
            knob("normal-volume", 10),
            knob("noise-gate-threshold", 112),
            knob("noise-gate-release", 3),
        ],
    },
    Amp {
        name: "black-panel-lux-vibrato",
        params: &[
            knob("volume", 13),
            knob("treble", 14),
            knob("bass", 15),
            knob("vibrato-speed", 16),
            knob("vibrato-sync", 21),
            knob("vibrato-intensity", 10),
            onoff("vibrato-on-off", 112),
            knob("noise-gate-threshold", 3),
            knob("noise-gate-release", 84),
        ],
    },
    Amp {
        name: "black-panel-lux-normal",
        params: &[
            knob("volume", 13),
            knob("treble", 14),
            knob("bass", 15),
            knob("vibrato-speed", 16),
            knob("vibrato-sync", 21),
            knob("vibrato-intensity", 10),
            knob("noise-gate-threshold", 3),
            knob("noise-gate-release", 84),
        ],
    },
    Amp {
        name: "ac-hi-boost",
        params: &[
            knob("normal-volume", 13),
            knob("brilliant-volume", 14),
            knob("bass", 15),
            knob("treble", 16),
            knob("cut", 21),
            knob("tremolo-speed", 10),
            knob("tremolo-sync", 112),
            knob("tremolo-depth", 3),
            onoff("tremolo-on-off", 22),
            knob("noise-gate-threshold", 84),
            knob("noise-gate-release", 24),
        ],
    },
    Amp {
        name: "black-panel-duo",
        params: &[
            knob("volume", 13),
            knob("treble", 14),
            knob("middle", 15),
            knob("bass", 16),
            onoff("bright", 21),
            knob("vibrato-speed", 10),
            knob("vibrato-sync", 112),
            knob("vibrato-intensity", 3),
            onoff("vibrato-on-off", 22),
            knob("noise-gate-threshold", 84),
            knob("noise-gate-release", 24),
        ],
    },
    Amp {
        name: "plexiglas-100w",
        params: &[
            knob("presence", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("volume-1", 21),
            knob("volume-2", 10),
            knob("noise-gate-threshold", 112),
            knob("noise-gate-release", 3),
        ],
    },
    Amp {
        name: "lead-800-100w",
        params: &[
            knob("presence", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("preamp-volume", 10),
            knob("master-volume", 21),
            knob("noise-gate-threshold", 112),
            knob("noise-gate-release", 3),
        ],
    },
    Amp {
        name: "m-2-lead",
        params: &[
            knob("volume", 13),
            knob("treble", 14),
            knob("bass", 15),
            knob("middle", 16),
            knob("drive", 21),
            knob("master", 10),
            onoff("bright", 112),
            knob("presence", 3),
            knob("noise-gate-threshold", 84),
            knob("noise-gate-release", 24),
        ],
    },
    Amp {
        name: "sl-100-drive",
        params: &[
            knob("preamp", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("presence", 21),
            knob("master", 10),
            onoff("mod", 112),
            knob("noise-gate-threshold", 3),
            knob("noise-gate-release", 84),
        ],
    },
    Amp {
        name: "sl-100-crunch",
        params: &[
            knob("preamp", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("presence", 21),
            knob("master", 10),
            onoff("bright", 112),
            knob("noise-gate-threshold", 3),
            knob("noise-gate-release", 84),
        ],
    },
    Amp {
        name: "sl-100-clean",
        params: &[
            knob("preamp", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("presence", 21),
            knob("master", 10),
            onoff("bright", 112),
            knob("noise-gate-threshold", 3),
            knob("noise-gate-release", 84),
        ],
    },
    Amp {
        name: "treadplate-modern",
        params: &[
            knob("master", 13),
            knob("presence", 14),
            knob("bass", 15),
            knob("middle", 16),
            knob("treble", 21),
            knob("gain", 10),
            knob("noise-gate-threshold", 112),
            knob("noise-gate-release", 3),
        ],
    },
    Amp {
        name: "treadplate-vintage",
        params: &[
            knob("master", 13),
            knob("presence", 14),
            knob("bass", 15),
            knob("middle", 16),
            knob("treble", 21),
            knob("gain", 10),
            knob("noise-gate-threshold", 112),
            knob("noise-gate-release", 3),
        ],
    },
    Amp {
        name: "dc-modern-crunch",
        params: &[
            knob("gain", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("presence", 21),
            knob("master", 10),
            onoff("bright", 112),
            knob("tremolo-speed", 3),
            knob("tremolo-sync", 84),
            knob("tremolo-depth", 24),
            onoff("tremolo-on-off", 45),
            knob("noise-gate-threshold", 23),
            knob("noise-gate-release", 22),
        ],
    },
    Amp {
        name: "dc-vintage-overdrive",
        params: &[
            knob("gain", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("presence", 21),
            knob("master", 10),
            onoff("bright", 112),
            knob("tremolo-speed", 3),
            knob("tremolo-sync", 84),
            knob("tremolo-depth", 24),
            onoff("tremolo-on-off", 45),
            knob("noise-gate-threshold", 23),
            knob("noise-gate-release", 22),
        ],
    },
    // --- Expansion Pack amps (firmware 2.x; from the on-device MIDI CC Reference) ---
    Amp {
        name: "j45",
        params: &[
            knob("presence", 13),
            knob("bass", 14),
            knob("middle", 15),
            knob("treble", 16),
            knob("volume-1", 21),
            knob("volume-2", 10),
            knob("noise-gate-threshold", 112),
            knob("noise-gate-release", 3),
        ],
    },
    Amp {
        name: "black-sr",
        params: &[
            knob("volume", 13),
            knob("treble", 14),
            knob("middle", 15),
            knob("bass", 16),
            onoff("bright", 21),
            knob("vibrato-speed", 10),
            knob("vibrato-sync", 112),
            knob("vibrato-depth", 3),
            onoff("vibrato-on-off", 22),
            knob("noise-gate-threshold", 84),
            knob("noise-gate-release", 24),
        ],
    },
    Amp {
        name: "black-vib",
        params: &[
            knob("volume", 13),
            knob("treble", 14),
            knob("middle", 15),
            knob("bass", 16),
            onoff("bright", 21),
            knob("vibrato-speed", 10),
            knob("vibrato-sync", 112),
            knob("vibrato-depth", 3),
            onoff("vibrato-on-off", 22),
            knob("noise-gate-threshold", 84),
            knob("noise-gate-release", 24),
        ],
    },
    Amp {
        name: "blue-line-bass",
        params: &[
            knob("volume", 13),
            knob("treble", 14),
            knob("middle", 15),
            knob("bass", 16),
            knob("ultra-hi", 21),
            knob("ultra-lo", 10),
            onoff("bright", 112),
            knob("mid-freq", 3),
            knob("noise-gate-threshold", 84),
            knob("noise-gate-release", 24),
        ],
    },
    Amp {
        name: "ms-30",
        params: &[
            knob("volume", 13),
            knob("bass", 14),
            knob("treble", 15),
            knob("cut", 16),
            knob("master", 21),
            knob("noise-gate-threshold", 10),
            knob("noise-gate-release", 112),
        ],
    },
    Amp {
        name: "rb-01b",
        params: &[
            knob("presence", 13),
            knob("volume", 14),
            knob("treble", 15),
            knob("middle", 16),
            knob("bass", 21),
            knob("gain", 10),
            onoff("bright", 112),
            knob("boost", 3),
            knob("noise-gate-threshold", 84),
            knob("noise-gate-release", 24),
        ],
    },
];

/// The speaker-cabinet models (User Guide ch.3, "The Speaker Cabinets"). The amp
/// section is *Amp + Cab + Mic*; the cabinet is chosen on the Cab page. These are
/// model selections, not MIDI-CC-addressable parameters (only `CAB/MIC BYPASS`,
/// CC 71, is — see [`AMP_GLOBAL`]).
pub const CABS: &[&str] = &[
    "1x12 Black Lux",
    "1x12 Tweed Lux",
    "2x12 AC Blue",
    "2x12 Black Duo",
    "4x10 Tweed Bass",
    "4x12 Classic 30",
    "4x12 Green 25Watt",
];

/// The microphone models (User Guide ch.3, "The Microphones"), chosen alongside the
/// cabinet on the Cab page. Two dynamic-American (`Dyn 7`/`Dyn 57`), two
/// dynamic-German (`Dyn 409`/`Dyn 421`), two condenser (`Cond 67`/`Cond 87`) and one
/// ribbon (`Ribbon 121`).
pub const MICS: &[&str] = &[
    "Dyn 7",
    "Dyn 57",
    "Dyn 409",
    "Dyn 421",
    "Cond 67",
    "Cond 87",
    "Ribbon 121",
];

/// The microphone position, toggled by the front-panel **SW2** on the Cab page:
/// the virtual mic is either centred on the speaker cone (on-axis, brighter) or
/// angled toward its edge (off-axis, warmer).
pub const MIC_POSITION: &[&str] = &["On-axis", "Off-axis"];

// ----------------------------------------------------------------------------
// Effects.
// ----------------------------------------------------------------------------

/// A chain slot an effect can occupy, or its dedicated block. An effect's CCs
/// differ per slot; [`Slot::Fixed`] is a dedicated-block effect with one CC set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Slot {
    /// The dedicated modulation slot.
    Mod,
    /// General-purpose effect slot 1.
    Fx1,
    /// General-purpose effect slot 2.
    Fx2,
    /// A dedicated block (distortion, wah, delay, reverb, FX loop): one CC set.
    Fixed,
}

impl Slot {
    /// Short label for listings.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Slot::Mod => "MOD",
            Slot::Fx1 => "FX1",
            Slot::Fx2 => "FX2",
            Slot::Fixed => "fixed",
        }
    }
}

/// One effect parameter: its display `name`, value [`Kind`], and MIDI CC in each
/// slot the effect can occupy — `cc` is aligned position-for-position with the
/// owning [`Effect::slots`]. The Eleven Rack assigns a *different* CC to a control
/// depending on whether the effect sits in Mod / FX1 / FX2 (and the mapping is not
/// a simple positional formula — e.g. Parametric EQ skips a slot, and Multi
/// Chorus's `Lo Cut`/`Width` share one CC across slots), so these come straight
/// from the unit's on-device MIDI CC Reference.
#[derive(Debug, Clone, Copy)]
pub struct FxParam {
    /// Stable kebab-case control name (e.g. `"pre-delay"`, `"lo-cut"`).
    pub name: &'static str,
    /// Value kind (knob / switch / stepped).
    pub kind: Kind,
    /// MIDI CC per slot, aligned with [`Effect::slots`].
    pub cc: &'static [u8],
}

const fn fk(name: &'static str, cc: &'static [u8]) -> FxParam {
    FxParam {
        name,
        kind: Kind::Knob,
        cc,
    }
}
const fn fon(name: &'static str, cc: &'static [u8]) -> FxParam {
    FxParam {
        name,
        kind: Kind::Switch {
            off: "Off",
            on: "On",
        },
        cc,
    }
}
const fn fsw(
    name: &'static str,
    off: &'static str,
    on: &'static str,
    cc: &'static [u8],
) -> FxParam {
    FxParam {
        name,
        kind: Kind::Switch { off, on },
        cc,
    }
}
const fn fst(name: &'static str, steps: &'static [Step], cc: &'static [u8]) -> FxParam {
    FxParam {
        name,
        kind: Kind::Stepped(steps),
        cc,
    }
}

/// One effect: the slots it can occupy and its parameters (each carrying a CC per
/// slot). Transcribed from the on-device MIDI CC Reference (which, being firmware-
/// generated, includes the Expansion Pack effects the base User Guide lacks).
#[derive(Debug, Clone, Copy)]
pub struct Effect {
    /// Effect name.
    pub name: &'static str,
    /// The slots this effect occupies, in the order its [`FxParam::cc`] arrays use.
    pub slots: &'static [Slot],
    /// The effect's parameters.
    pub params: &'static [FxParam],
}

impl Effect {
    /// Find a parameter by case-insensitive name.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&FxParam> {
        self.params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The CC of parameter `p` when this effect sits in `slot`, if it can.
    #[must_use]
    pub fn cc_in(&self, p: &FxParam, slot: Slot) -> Option<u8> {
        let i = self.slots.iter().position(|&s| s == slot)?;
        p.cc.get(i).copied()
    }
}

const MOD_FX: &[Slot] = &[Slot::Mod, Slot::Fx1, Slot::Fx2];
const FX12: &[Slot] = &[Slot::Fx1, Slot::Fx2];
const FIXED: &[Slot] = &[Slot::Fixed];

/// All catalogued effects, with per-slot CCs (base User Guide Ch.11 plus the
/// Expansion Pack effects, all from the on-device MIDI CC Reference).
pub const EFFECTS: &[Effect] = &[
    // --- Distortion block (Bypass 25) ---
    Effect {
        name: "black-op-distortion",
        slots: FIXED,
        params: &[
            fon("bypass", &[25]),
            fk("distortion", &[27]),
            fk("cut", &[78]),
            fk("volume", &[79]),
        ],
    },
    Effect {
        name: "green-jrc-overdrive",
        slots: FIXED,
        params: &[
            fon("bypass", &[25]),
            fk("drive", &[27]),
            fk("tone", &[78]),
            fk("level", &[79]),
        ],
    },
    Effect {
        name: "tri-knob-fuzz",
        slots: FIXED,
        params: &[
            fon("bypass", &[25]),
            fk("volume", &[27]),
            fk("sustain", &[78]),
            fk("tone", &[79]),
        ],
    },
    Effect {
        name: "white-boost",
        slots: FIXED,
        params: &[
            fon("bypass", &[25]),
            fk("gain", &[27]),
            fk("treble", &[78]),
            fk("bass", &[79]),
            fk("volume", &[80]),
        ],
    },
    Effect {
        name: "dc-distortion",
        slots: FIXED,
        params: &[
            fon("bypass", &[25]),
            fk("gain", &[27]),
            fk("treble", &[78]),
            fk("bass", &[79]),
            fk("volume", &[80]),
        ],
    },
    // --- Wah / Volume pedal ---
    Effect {
        name: "black-wah",
        slots: FIXED,
        params: &[fon("bypass", &[43]), fk("position", &[4])],
    },
    Effect {
        name: "shine-wah",
        slots: FIXED,
        params: &[fon("bypass", &[43]), fk("position", &[4])],
    },
    Effect {
        name: "volume-pedal",
        slots: FIXED,
        params: &[fon("bypass", &[75]), fk("position", &[7])],
    },
    Effect {
        name: "tuner",
        slots: FIXED,
        params: &[fon("bypass", &[69])],
    },
    // --- Delay block (Bypass 28) ---
    Effect {
        name: "bbd-delay",
        slots: FIXED,
        params: &[
            fon("bypass", &[28]),
            fk("delay", &[62]),
            fst("sync", FX_SYNC_STEPS, &[33]),
            fk("mix", &[85]),
            fk("feedback", &[35]),
            fk("input-level", &[87]),
            fsw("mod", "Chorus", "Vibrato", &[34]),
            fk("depth", &[48]),
            fon("noise", &[55]),
            fon("expanded-delay", &[49]),
        ],
    },
    Effect {
        name: "tape-echo",
        slots: FIXED,
        params: &[
            fon("bypass", &[28]),
            fk("delay", &[62]),
            fst("sync", FX_SYNC_STEPS, &[33]),
            fk("mix", &[85]),
            fk("feedback", &[35]),
            fk("rec-level", &[87]),
            fk("head", &[34]),
            fk("wow", &[48]),
            fon("hiss", &[55]),
            fon("expanded-delay", &[49]),
        ],
    },
    Effect {
        name: "dyn-delay",
        slots: FIXED,
        params: &[
            fon("bypass", &[28]),
            fk("delay", &[62]),
            fst("sync", FX_SYNC_STEPS, &[33]),
            fk("mix", &[85]),
            fk("feedback", &[35]),
            fk("mode", &[87]),
            fk("ratio", &[34]),
            fk("hi-cut", &[48]),
            fk("lo-cut", &[49]),
            fk("width", &[55]),
            fk("em-rate", &[59]),
            fk("em-feedback", &[72]),
            fk("em-mix", &[73]),
        ],
    },
    // --- Reverb block (Bypass 36) / FX Loop ---
    Effect {
        name: "blackpanel-spring-reverb",
        slots: FIXED,
        params: &[
            fon("bypass", &[36]),
            fk("mix", &[18]),
            fk("decay", &[38]),
            fk("tone", &[40]),
        ],
    },
    Effect {
        name: "eleven-sr",
        slots: FIXED,
        params: &[
            fon("bypass", &[36]),
            fk("mix", &[18]),
            fk("decay", &[38]),
            fk("tone", &[40]),
            fk("pre-delay", &[39]),
            fst("type", REVERB_TYPE_STEPS, &[76]),
        ],
    },
    Effect {
        name: "fx-loop",
        slots: FIXED,
        params: &[
            fon("bypass", &[107]),
            fk("send", &[19]),
            fk("return", &[108]),
            fk("mix", &[88]),
        ],
    },
    // --- Mod/FX1/FX2 slot effects (cc = [Mod, FX1, FX2]) ---
    Effect {
        name: "c1-chorus-vibrato",
        slots: MOD_FX,
        params: &[
            fon("bypass", &[50, 63, 86]),
            fk("chorus", &[61, 20, 113]),
            fk("rate", &[52, 42, 114]),
            fst("sync", FX_SYNC_STEPS, &[53, 60, 115]),
            fk("depth", &[54, 77, 96]),
            fsw("chorus-vibrato", "Chorus", "Vibrato", &[57, 116, 97]),
        ],
    },
    Effect {
        name: "flanger",
        slots: MOD_FX,
        params: &[
            fon("bypass", &[50, 63, 86]),
            fk("pre-delay", &[61, 20, 113]),
            fk("depth", &[52, 42, 114]),
            fk("rate", &[53, 60, 115]),
            fst("sync", FX_SYNC_STEPS, &[54, 77, 96]),
            fk("feedback", &[57, 116, 97]),
        ],
    },
    Effect {
        name: "orange-phaser",
        slots: MOD_FX,
        params: &[
            fon("bypass", &[50, 63, 86]),
            fk("rate", &[61, 20, 113]),
            fst("sync", FX_SYNC_STEPS, &[52, 42, 114]),
        ],
    },
    Effect {
        name: "roto-speaker",
        slots: MOD_FX,
        params: &[
            fon("bypass", &[50, 63, 86]),
            fst("speed", ROTO_SPEED_STEPS, &[61, 20, 113]),
            fk("balance", &[52, 42, 114]),
            fst("type", ROTO_TYPE_STEPS, &[53, 60, 115]),
        ],
    },
    Effect {
        name: "vibe-phaser",
        slots: MOD_FX,
        params: &[
            fon("bypass", &[50, 63, 86]),
            fk("volume", &[61, 20, 113]),
            fk("depth", &[52, 42, 114]),
            fk("rate", &[53, 60, 115]),
            fst("sync", FX_SYNC_STEPS, &[54, 77, 96]),
            fsw("chorus-vibrato", "Chorus", "Vibrato", &[57, 116, 97]),
        ],
    },
    Effect {
        name: "multi-chorus",
        slots: MOD_FX,
        params: &[
            fon("bypass", &[50, 63, 86]),
            fk("rate", &[61, 20, 113]),
            fst("sync", FX_SYNC_STEPS, &[52, 42, 114]),
            fk("depth", &[53, 60, 115]),
            fk("pre-delay", &[54, 77, 96]),
            fk("mix", &[57, 116, 97]),
            fsw("triangle-sine", "Triangle", "Sine", &[51, 117, 98]),
            fk("voices", &[56, 118, 99]),
            fk("lo-cut", &[89, 89, 89]),
            fk("width", &[90, 90, 90]),
        ],
    },
    // --- FX1/FX2 slot effects (cc = [FX1, FX2]) ---
    Effect {
        name: "graphic-eq",
        slots: FX12,
        params: &[
            fon("bypass", &[63, 86]),
            fk("100-hz", &[20, 113]),
            fk("370-hz", &[42, 114]),
            fk("800-hz", &[60, 115]),
            fk("2-khz", &[77, 96]),
            fk("3-25-khz", &[116, 97]),
            fk("output", &[117, 98]),
        ],
    },
    Effect {
        name: "parametric-eq",
        slots: FX12,
        params: &[
            fon("bypass", &[63, 86]),
            fk("l-gain", &[20, 113]),
            fk("lm-gain", &[42, 114]),
            fk("hm-gain", &[77, 96]),
            fk("h-gain", &[116, 97]),
            fk("output", &[117, 98]),
            fk("l-freq", &[118, 99]),
            fk("l-q", &[5, 37]),
            fk("lm-freq", &[119, 46]),
            fk("lm-q", &[9, 47]),
            fk("hm-freq", &[12, 58]),
            fk("hm-q", &[26, 109]),
            fk("h-freq", &[29, 110]),
            fk("h-q", &[30, 70]),
        ],
    },
    Effect {
        name: "gray-compressor",
        slots: FX12,
        params: &[
            fon("bypass", &[63, 86]),
            fk("sustain", &[20, 113]),
            fk("level", &[42, 114]),
        ],
    },
    Effect {
        name: "dyn3-compressor",
        slots: FX12,
        params: &[
            fon("bypass", &[63, 86]),
            fk("threshold", &[20, 113]),
            fk("attack", &[42, 114]),
            fk("release", &[60, 115]),
            fk("gain", &[77, 96]),
            fk("ratio", &[116, 97]),
            fk("knee", &[117, 98]),
        ],
    },
];

// ----------------------------------------------------------------------------
// General / frequently-used and miscellaneous controls.
// ----------------------------------------------------------------------------

/// The "General/Frequently Used Controls" bypass shortcuts and pedal positions.
pub const GENERAL: &[Param] = &[
    onoff("dist-bypass", 25),
    onoff("mod-bypass", 50),
    onoff("delay-bypass", 28),
    onoff("reverb-bypass", 36),
    onoff("fx-loop-bypass", 107),
    onoff("fx1-bypass", 63),
    onoff("fx2-bypass", 86),
    onoff("wah-bypass", 43),
    onoff("amp-bypass", 111),
    knob("volume-pedal-position", 7),
    knob("wah-position", 4),
    knob("multi-fx-control", 11),
    knob("tap-tempo", 64),
];

/// "Miscellaneous MIDI Controls": the multi-FX and rig-volume pedal assignments.
/// Bank select (CC 32: `1` = Factory rigs, `0` = User rigs) precedes a Program
/// Change to select a rig; see `docs/eleven-rack-sysex-protocol.adoc`.
pub const MISC: &[Param] = &[
    knob("multi-fx-pedal-position", 11),
    knob("rig-volume-pedal-position", 17),
    sw("bank-change", 32, "User Rigs", "Factory Rigs"),
];

/// Look up an amp model by case-insensitive name.
#[must_use]
pub fn amp(name: &str) -> Option<&'static Amp> {
    AMPS.iter().find(|a| a.name.eq_ignore_ascii_case(name))
}

/// Look up an effect by case-insensitive name.
#[must_use]
pub fn effect(name: &str) -> Option<&'static Effect> {
    EFFECTS.iter().find(|e| e.name.eq_ignore_ascii_case(name))
}

/// Look up a *global* control (one with a single fixed CC regardless of model or
/// slot) by case-insensitive name: the [`GENERAL`] bypass shortcuts / pedals, the
/// [`AMP_GLOBAL`] amp-section controls, and the [`MISC`] pedal/bank controls.
#[must_use]
pub fn global(name: &str) -> Option<&'static Param> {
    GENERAL
        .iter()
        .chain(AMP_GLOBAL)
        .chain(MISC)
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

/// Resolve a control `name` to its MIDI CC and value [`Kind`] for remote control.
///
/// Context disambiguates names that appear on many models/slots:
/// * `amp_ctx` — resolve an amp model's parameter (e.g. `presence` is CC 13 on
///   `tweed-bass` but CC 3 on `m-2-lead`).
/// * `fx_ctx` — resolve an effect parameter in a given [`Slot`] (an effect's CC
///   differs per slot).
/// * neither — a [`global`] control.
///
/// Returns `None` if the name (or the amp/effect/slot) is not found.
#[must_use]
pub fn resolve_cc(
    name: &str,
    amp_ctx: Option<&str>,
    fx_ctx: Option<(&str, Slot)>,
) -> Option<(u8, Kind)> {
    if let Some(a) = amp_ctx {
        let p = amp(a)?.param(name)?;
        return Some((p.cc, p.kind));
    }
    if let Some((fx, slot)) = fx_ctx {
        let e = effect(fx)?;
        let p = e.param(name)?;
        return Some((e.cc_in(p, slot)?, p.kind));
    }
    global(name).map(|p| (p.cc, p.kind))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn amp_param_lookup_returns_documented_cc() {
        // Spot-check the transcription against the User Guide chart.
        let dc = amp("dc-modern-crunch").unwrap();
        assert_eq!(dc.param("gain").unwrap().cc, 13);
        assert_eq!(dc.param("master").unwrap().cc, 10);
        assert_eq!(dc.param("presence").unwrap().cc, 21);
        // CC is the remote-control number, not the SysEx index (those differ).
        assert!(dc.param("missing").is_none());
    }

    #[test]
    fn every_amp_cc_is_a_valid_7bit_value() {
        for a in AMPS {
            for p in a.params {
                assert!(p.cc <= 127, "{} {} cc out of range", a.name, p.name);
            }
        }
    }

    #[test]
    fn effect_per_slot_cc_and_alignment() {
        // Every FxParam's cc array length matches its effect's slot count, and all
        // CCs are valid 7-bit values.
        for e in EFFECTS {
            for p in e.params {
                assert_eq!(
                    p.cc.len(),
                    e.slots.len(),
                    "{} / {} slot/cc mismatch",
                    e.name,
                    p.name
                );
                for &c in p.cc {
                    assert!(c <= 127, "{} / {} cc {c} out of range", e.name, p.name);
                }
            }
        }
        // cc_in resolves the right per-slot CC (C1 Chorus Bypass: Mod 50 / FX1 63 / FX2 86).
        let c1 = effect("c1-chorus-vibrato").unwrap();
        let byp = c1.param("bypass").unwrap();
        assert_eq!(c1.cc_in(byp, Slot::Mod), Some(50));
        assert_eq!(c1.cc_in(byp, Slot::Fx1), Some(63));
        assert_eq!(c1.cc_in(byp, Slot::Fx2), Some(86));
    }

    #[test]
    fn stepped_describe_picks_the_right_label() {
        let rev = effect("eleven-sr").unwrap();
        let ty = rev.param("type").unwrap();
        assert_eq!(ty.kind.describe(70), "Concert Hall");
        assert_eq!(ty.kind.describe(0), "Echo Room");
        assert_eq!(ty.kind.describe(127), "Early Reflect 2");
    }

    #[test]
    fn expansion_effects_now_fully_catalogued() {
        // The Expansion Pack effects are present with their full parameters.
        let peq = effect("parametric-eq").unwrap();
        assert_eq!(peq.slots, [Slot::Fx1, Slot::Fx2]);
        assert_eq!(peq.params.len(), 14);
        assert_eq!(peq.cc_in(peq.param("h-q").unwrap(), Slot::Fx2), Some(70));
        // Multi Chorus: lo-cut/width share one CC across all slots.
        let mc = effect("multi-chorus").unwrap();
        let lo = mc.param("lo-cut").unwrap();
        assert_eq!(mc.cc_in(lo, Slot::Mod), Some(89));
        assert_eq!(mc.cc_in(lo, Slot::Fx2), Some(89));
        // white-boost / dc-distortion are fixed-block, 5 params.
        assert_eq!(effect("dc-distortion").unwrap().slots, [Slot::Fixed]);
        assert_eq!(
            effect("white-boost").unwrap().param("volume").unwrap().cc,
            [80]
        );
    }

    #[test]
    fn amp_section_models_are_populated() {
        assert_eq!(CABS.len(), 7);
        assert_eq!(MICS.len(), 7);
        assert_eq!(MIC_POSITION, ["On-axis", "Off-axis"]);
        assert!(CABS.contains(&"4x12 Green 25Watt"));
        assert!(MICS.contains(&"Ribbon 121"));
    }

    #[test]
    fn resolve_cc_uses_context() {
        // Amp context: presence differs per model.
        assert_eq!(
            resolve_cc("presence", Some("tweed-bass"), None).unwrap().0,
            13
        );
        assert_eq!(resolve_cc("presence", Some("m-2-lead"), None).unwrap().0, 3);
        // Effect context: an effect's CC differs per slot.
        assert_eq!(
            resolve_cc("rate", None, Some(("multi-chorus", Slot::Fx1)))
                .unwrap()
                .0,
            20
        );
        assert_eq!(
            resolve_cc("rate", None, Some(("multi-chorus", Slot::Fx2)))
                .unwrap()
                .0,
            113
        );
        // Global control needs no context.
        assert_eq!(resolve_cc("dist-bypass", None, None).unwrap().0, 25);
        assert_eq!(resolve_cc("amp-output", None, None).unwrap().0, 92);
        // Unknown / missing context.
        assert!(resolve_cc("presence", None, None).is_none()); // ambiguous amp param, no ctx
        assert!(resolve_cc("nope", Some("tweed-bass"), None).is_none());
    }

    #[test]
    fn switch_describe_splits_at_64() {
        let p = onoff("x", 1);
        assert_eq!(p.kind.describe(63), "Off");
        assert_eq!(p.kind.describe(64), "On");
    }
}
