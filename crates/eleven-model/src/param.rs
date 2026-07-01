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
//!   `TONE` on Tweed Lux but `PRESENCE` on Tweed Bass). Each [`Amp`] lists its
//!   `(name, cc)` pairs; the amp-section globals (bypass, output, cab/mic) are in
//!   [`AMP_GLOBAL`].
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
    /// Human-readable control name as printed in the User Guide (e.g. `"PRESENCE"`).
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
    onoff("AMP BYPASS", 111),
    knob("AMP OUTPUT", 92),
    onoff("CAB/MIC BYPASS", 71),
];

/// All catalogued amplifier models (User Guide Ch.11).
pub const AMPS: &[Amp] = &[
    Amp {
        name: "Tweed Lux",
        params: &[
            knob("TONE", 13),
            knob("INSTRUMENT VOLUME", 14),
            knob("MIC VOLUME", 15),
            knob("NOISE GATE THRESHOLD", 16),
            knob("NOISE GATE RELEASE", 21),
        ],
    },
    Amp {
        name: "Tweed Bass",
        params: &[
            knob("PRESENCE", 13),
            knob("MIDDLE", 14),
            knob("BASS", 15),
            knob("TREBLE", 16),
            knob("BRIGHT VOLUME", 21),
            knob("NORMAL VOLUME", 10),
            knob("NOISE GATE THRESHOLD", 112),
            knob("NOISE GATE RELEASE", 3),
        ],
    },
    Amp {
        name: "Black Panel Lux Vibrato",
        params: &[
            knob("VOLUME", 13),
            knob("TREBLE", 14),
            knob("BASS", 15),
            knob("VIBRATO SPEED", 16),
            knob("VIBRATO SYNC", 21),
            knob("VIBRATO INTENSITY", 10),
            onoff("VIBRATO ON/OFF", 112),
            knob("NOISE GATE THRESHOLD", 3),
            knob("NOISE GATE RELEASE", 84),
        ],
    },
    Amp {
        name: "Black Panel Lux Normal",
        params: &[
            knob("VOLUME", 13),
            knob("TREBLE", 14),
            knob("BASS", 15),
            knob("VIBRATO SPEED", 16),
            knob("VIBRATO SYNC", 21),
            knob("VIBRATO INTENSITY", 10),
            knob("NOISE GATE THRESHOLD", 3),
            knob("NOISE GATE RELEASE", 84),
        ],
    },
    Amp {
        name: "AC Hi Boost",
        params: &[
            knob("NORMAL VOLUME", 13),
            knob("BRILLIANT VOLUME", 14),
            knob("BASS", 15),
            knob("TREBLE", 16),
            knob("CUT", 21),
            knob("TREMOLO SPEED", 10),
            knob("TREMOLO SYNC", 112),
            knob("TREMOLO DEPTH", 3),
            onoff("TREMOLO ON/OFF", 22),
            knob("NOISE GATE THRESHOLD", 84),
            knob("NOISE GATE RELEASE", 24),
        ],
    },
    Amp {
        name: "Black Panel Duo",
        params: &[
            knob("VOLUME", 13),
            knob("TREBLE", 14),
            knob("MIDDLE", 15),
            knob("BASS", 16),
            onoff("BRIGHT", 21),
            knob("VIBRATO SPEED", 10),
            knob("VIBRATO SYNC", 112),
            knob("VIBRATO INTENSITY", 3),
            onoff("VIBRATO ON/OFF", 22),
            knob("NOISE GATE THRESHOLD", 84),
            knob("NOISE GATE RELEASE", 24),
        ],
    },
    Amp {
        name: "Plexiglas - 100W",
        params: &[
            knob("PRESENCE", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("VOLUME 1", 21),
            knob("VOLUME 2", 10),
            knob("NOISE GATE THRESHOLD", 112),
            knob("NOISE GATE RELEASE", 3),
        ],
    },
    Amp {
        name: "Lead 800 - 100W",
        params: &[
            knob("PRESENCE", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("PREAMP VOLUME", 10),
            knob("MASTER VOLUME", 21),
            knob("NOISE GATE THRESHOLD", 112),
            knob("NOISE GATE RELEASE", 3),
        ],
    },
    Amp {
        name: "M-2 Lead",
        params: &[
            knob("VOLUME", 13),
            knob("TREBLE", 14),
            knob("BASS", 15),
            knob("MIDDLE", 16),
            knob("DRIVE", 21),
            knob("MASTER", 10),
            onoff("BRIGHT", 112),
            knob("PRESENCE", 3),
            knob("NOISE GATE THRESHOLD", 84),
            knob("NOISE GATE RELEASE", 24),
        ],
    },
    Amp {
        name: "SL-100 Drive",
        params: &[
            knob("PREAMP", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("PRESENCE", 21),
            knob("MASTER", 10),
            onoff("MOD", 112),
            knob("NOISE GATE THRESHOLD", 3),
            knob("NOISE GATE RELEASE", 84),
        ],
    },
    Amp {
        name: "SL-100 Crunch",
        params: &[
            knob("PREAMP", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("PRESENCE", 21),
            knob("MASTER", 10),
            onoff("BRIGHT", 112),
            knob("NOISE GATE THRESHOLD", 3),
            knob("NOISE GATE RELEASE", 84),
        ],
    },
    Amp {
        name: "SL-100 Clean",
        params: &[
            knob("PREAMP", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("PRESENCE", 21),
            knob("MASTER", 10),
            onoff("BRIGHT", 112),
            knob("NOISE GATE THRESHOLD", 3),
            knob("NOISE GATE RELEASE", 84),
        ],
    },
    Amp {
        name: "Treadplate Modern",
        params: &[
            knob("MASTER", 13),
            knob("PRESENCE", 14),
            knob("BASS", 15),
            knob("MIDDLE", 16),
            knob("TREBLE", 21),
            knob("GAIN", 10),
            knob("NOISE GATE THRESHOLD", 112),
            knob("NOISE GATE RELEASE", 3),
        ],
    },
    Amp {
        name: "Treadplate Vintage",
        params: &[
            knob("MASTER", 13),
            knob("PRESENCE", 14),
            knob("BASS", 15),
            knob("MIDDLE", 16),
            knob("TREBLE", 21),
            knob("GAIN", 10),
            knob("NOISE GATE THRESHOLD", 112),
            knob("NOISE GATE RELEASE", 3),
        ],
    },
    Amp {
        name: "DC Modern Crunch",
        params: &[
            knob("GAIN", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("PRESENCE", 21),
            knob("MASTER", 10),
            onoff("BRIGHT", 112),
            knob("TREMOLO SPEED", 3),
            knob("TREMOLO SYNC", 84),
            knob("TREMOLO DEPTH", 24),
            onoff("TREMOLO ON/OFF", 45),
            knob("NOISE GATE THRESHOLD", 23),
            knob("NOISE GATE RELEASE", 22),
        ],
    },
    Amp {
        name: "DC Vintage Overdrive",
        params: &[
            knob("GAIN", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("PRESENCE", 21),
            knob("MASTER", 10),
            onoff("BRIGHT", 112),
            knob("TREMOLO SPEED", 3),
            knob("TREMOLO SYNC", 84),
            knob("TREMOLO DEPTH", 24),
            onoff("TREMOLO ON/OFF", 45),
            knob("NOISE GATE THRESHOLD", 23),
            knob("NOISE GATE RELEASE", 22),
        ],
    },
    // --- Expansion Pack amps (firmware 2.x; from the on-device MIDI CC Reference) ---
    Amp {
        name: "J45",
        params: &[
            knob("PRESENCE", 13),
            knob("BASS", 14),
            knob("MIDDLE", 15),
            knob("TREBLE", 16),
            knob("VOLUME 1", 21),
            knob("VOLUME 2", 10),
            knob("NOISE GATE THRESHOLD", 112),
            knob("NOISE GATE RELEASE", 3),
        ],
    },
    Amp {
        name: "Black SR",
        params: &[
            knob("VOLUME", 13),
            knob("TREBLE", 14),
            knob("MIDDLE", 15),
            knob("BASS", 16),
            onoff("BRIGHT", 21),
            knob("VIBRATO SPEED", 10),
            knob("VIBRATO SYNC", 112),
            knob("VIBRATO DEPTH", 3),
            onoff("VIBRATO ON/OFF", 22),
            knob("NOISE GATE THRESHOLD", 84),
            knob("NOISE GATE RELEASE", 24),
        ],
    },
    Amp {
        name: "Black Vib",
        params: &[
            knob("VOLUME", 13),
            knob("TREBLE", 14),
            knob("MIDDLE", 15),
            knob("BASS", 16),
            onoff("BRIGHT", 21),
            knob("VIBRATO SPEED", 10),
            knob("VIBRATO SYNC", 112),
            knob("VIBRATO DEPTH", 3),
            onoff("VIBRATO ON/OFF", 22),
            knob("NOISE GATE THRESHOLD", 84),
            knob("NOISE GATE RELEASE", 24),
        ],
    },
    Amp {
        name: "Blue Line Bass",
        params: &[
            knob("VOLUME", 13),
            knob("TREBLE", 14),
            knob("MIDDLE", 15),
            knob("BASS", 16),
            knob("ULTRA HI", 21),
            knob("ULTRA LO", 10),
            onoff("BRIGHT", 112),
            knob("MID FREQ", 3),
            knob("NOISE GATE THRESHOLD", 84),
            knob("NOISE GATE RELEASE", 24),
        ],
    },
    Amp {
        name: "MS-30",
        params: &[
            knob("VOLUME", 13),
            knob("BASS", 14),
            knob("TREBLE", 15),
            knob("CUT", 16),
            knob("MASTER", 21),
            knob("NOISE GATE THRESHOLD", 10),
            knob("NOISE GATE RELEASE", 112),
        ],
    },
    Amp {
        name: "RB-01b",
        params: &[
            knob("PRESENCE", 13),
            knob("VOLUME", 14),
            knob("TREBLE", 15),
            knob("MIDDLE", 16),
            knob("BASS", 21),
            knob("GAIN", 10),
            onoff("BRIGHT", 112),
            knob("BOOST", 3),
            knob("NOISE GATE THRESHOLD", 84),
            knob("NOISE GATE RELEASE", 24),
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
    /// Control name as shown on the unit (e.g. `"Pre-Delay"`, `"Lo Cut"`).
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
        name: "Black Op Distortion",
        slots: FIXED,
        params: &[
            fon("Bypass", &[25]),
            fk("Distortion", &[27]),
            fk("Cut", &[78]),
            fk("Volume", &[79]),
        ],
    },
    Effect {
        name: "Green JRC Overdrive",
        slots: FIXED,
        params: &[
            fon("Bypass", &[25]),
            fk("Drive", &[27]),
            fk("Tone", &[78]),
            fk("Level", &[79]),
        ],
    },
    Effect {
        name: "Tri-Knob Fuzz",
        slots: FIXED,
        params: &[
            fon("Bypass", &[25]),
            fk("Volume", &[27]),
            fk("Sustain", &[78]),
            fk("Tone", &[79]),
        ],
    },
    Effect {
        name: "White Boost",
        slots: FIXED,
        params: &[
            fon("Bypass", &[25]),
            fk("Gain", &[27]),
            fk("Treble", &[78]),
            fk("Bass", &[79]),
            fk("Volume", &[80]),
        ],
    },
    Effect {
        name: "DC Distortion",
        slots: FIXED,
        params: &[
            fon("Bypass", &[25]),
            fk("Gain", &[27]),
            fk("Treble", &[78]),
            fk("Bass", &[79]),
            fk("Volume", &[80]),
        ],
    },
    // --- Wah / Volume pedal ---
    Effect {
        name: "Black Wah",
        slots: FIXED,
        params: &[fon("Bypass", &[43]), fk("Position", &[4])],
    },
    Effect {
        name: "Shine Wah",
        slots: FIXED,
        params: &[fon("Bypass", &[43]), fk("Position", &[4])],
    },
    Effect {
        name: "Volume Pedal",
        slots: FIXED,
        params: &[fon("Bypass", &[75]), fk("Position", &[7])],
    },
    Effect {
        name: "Tuner",
        slots: FIXED,
        params: &[fon("Bypass", &[69])],
    },
    // --- Delay block (Bypass 28) ---
    Effect {
        name: "BBD Delay",
        slots: FIXED,
        params: &[
            fon("Bypass", &[28]),
            fk("Delay", &[62]),
            fst("Sync", FX_SYNC_STEPS, &[33]),
            fk("Mix", &[85]),
            fk("Feedback", &[35]),
            fk("Input Level", &[87]),
            fsw("Mod", "Chorus", "Vibrato", &[34]),
            fk("Depth", &[48]),
            fon("Noise", &[55]),
            fon("Expanded Delay", &[49]),
        ],
    },
    Effect {
        name: "Tape Echo",
        slots: FIXED,
        params: &[
            fon("Bypass", &[28]),
            fk("Delay", &[62]),
            fst("Sync", FX_SYNC_STEPS, &[33]),
            fk("Mix", &[85]),
            fk("Feedback", &[35]),
            fk("Rec Level", &[87]),
            fk("Head", &[34]),
            fk("Wow", &[48]),
            fon("Hiss", &[55]),
            fon("Expanded Delay", &[49]),
        ],
    },
    Effect {
        name: "Dyn Delay",
        slots: FIXED,
        params: &[
            fon("Bypass", &[28]),
            fk("Delay", &[62]),
            fst("Sync", FX_SYNC_STEPS, &[33]),
            fk("Mix", &[85]),
            fk("Feedback", &[35]),
            fk("Mode", &[87]),
            fk("Ratio", &[34]),
            fk("Hi Cut", &[48]),
            fk("Lo Cut", &[49]),
            fk("Width", &[55]),
            fk("EM Rate", &[59]),
            fk("EM Feedback", &[72]),
            fk("EM Mix", &[73]),
        ],
    },
    // --- Reverb block (Bypass 36) / FX Loop ---
    Effect {
        name: "Blackpanel Spring Reverb",
        slots: FIXED,
        params: &[
            fon("Bypass", &[36]),
            fk("Mix", &[18]),
            fk("Decay", &[38]),
            fk("Tone", &[40]),
        ],
    },
    Effect {
        name: "Eleven SR (Stereo Reverb)",
        slots: FIXED,
        params: &[
            fon("Bypass", &[36]),
            fk("Mix", &[18]),
            fk("Decay", &[38]),
            fk("Tone", &[40]),
            fk("Pre-Delay", &[39]),
            fst("Type", REVERB_TYPE_STEPS, &[76]),
        ],
    },
    Effect {
        name: "FX Loop",
        slots: FIXED,
        params: &[
            fon("Bypass", &[107]),
            fk("Send", &[19]),
            fk("Return", &[108]),
            fk("Mix", &[88]),
        ],
    },
    // --- Mod/FX1/FX2 slot effects (cc = [Mod, FX1, FX2]) ---
    Effect {
        name: "C1 Chorus/Vibrato",
        slots: MOD_FX,
        params: &[
            fon("Bypass", &[50, 63, 86]),
            fk("Chorus", &[61, 20, 113]),
            fk("Rate", &[52, 42, 114]),
            fst("Sync", FX_SYNC_STEPS, &[53, 60, 115]),
            fk("Depth", &[54, 77, 96]),
            fsw("Chorus/Vibrato", "Chorus", "Vibrato", &[57, 116, 97]),
        ],
    },
    Effect {
        name: "Flanger",
        slots: MOD_FX,
        params: &[
            fon("Bypass", &[50, 63, 86]),
            fk("Pre-Delay", &[61, 20, 113]),
            fk("Depth", &[52, 42, 114]),
            fk("Rate", &[53, 60, 115]),
            fst("Sync", FX_SYNC_STEPS, &[54, 77, 96]),
            fk("Feedback", &[57, 116, 97]),
        ],
    },
    Effect {
        name: "Orange Phaser",
        slots: MOD_FX,
        params: &[
            fon("Bypass", &[50, 63, 86]),
            fk("Rate", &[61, 20, 113]),
            fst("Sync", FX_SYNC_STEPS, &[52, 42, 114]),
        ],
    },
    Effect {
        name: "Roto Speaker",
        slots: MOD_FX,
        params: &[
            fon("Bypass", &[50, 63, 86]),
            fst("Speed", ROTO_SPEED_STEPS, &[61, 20, 113]),
            fk("Balance", &[52, 42, 114]),
            fst("Type", ROTO_TYPE_STEPS, &[53, 60, 115]),
        ],
    },
    Effect {
        name: "Vibe Phaser",
        slots: MOD_FX,
        params: &[
            fon("Bypass", &[50, 63, 86]),
            fk("Volume", &[61, 20, 113]),
            fk("Depth", &[52, 42, 114]),
            fk("Rate", &[53, 60, 115]),
            fst("Sync", FX_SYNC_STEPS, &[54, 77, 96]),
            fsw("Chorus/Vibrato", "Chorus", "Vibrato", &[57, 116, 97]),
        ],
    },
    Effect {
        name: "Multi Chorus",
        slots: MOD_FX,
        params: &[
            fon("Bypass", &[50, 63, 86]),
            fk("Rate", &[61, 20, 113]),
            fst("Sync", FX_SYNC_STEPS, &[52, 42, 114]),
            fk("Depth", &[53, 60, 115]),
            fk("Pre-Delay", &[54, 77, 96]),
            fk("Mix", &[57, 116, 97]),
            fsw("Triangle/Sine", "Triangle", "Sine", &[51, 117, 98]),
            fk("Voices", &[56, 118, 99]),
            fk("Lo Cut", &[89, 89, 89]),
            fk("Width", &[90, 90, 90]),
        ],
    },
    // --- FX1/FX2 slot effects (cc = [FX1, FX2]) ---
    Effect {
        name: "Graphic EQ",
        slots: FX12,
        params: &[
            fon("Bypass", &[63, 86]),
            fk("100 Hz", &[20, 113]),
            fk("370 Hz", &[42, 114]),
            fk("800 Hz", &[60, 115]),
            fk("2 kHz", &[77, 96]),
            fk("3.25 kHz", &[116, 97]),
            fk("Output", &[117, 98]),
        ],
    },
    Effect {
        name: "Parametric EQ",
        slots: FX12,
        params: &[
            fon("Bypass", &[63, 86]),
            fk("L Gain", &[20, 113]),
            fk("LM Gain", &[42, 114]),
            fk("HM Gain", &[77, 96]),
            fk("H Gain", &[116, 97]),
            fk("Output", &[117, 98]),
            fk("L Freq", &[118, 99]),
            fk("L Q", &[5, 37]),
            fk("LM Freq", &[119, 46]),
            fk("LM Q", &[9, 47]),
            fk("HM Freq", &[12, 58]),
            fk("HM Q", &[26, 109]),
            fk("H Freq", &[29, 110]),
            fk("H Q", &[30, 70]),
        ],
    },
    Effect {
        name: "Gray Compressor",
        slots: FX12,
        params: &[
            fon("Bypass", &[63, 86]),
            fk("Sustain", &[20, 113]),
            fk("Level", &[42, 114]),
        ],
    },
    Effect {
        name: "Dyn3 Compressor",
        slots: FX12,
        params: &[
            fon("Bypass", &[63, 86]),
            fk("Threshold", &[20, 113]),
            fk("Attack", &[42, 114]),
            fk("Release", &[60, 115]),
            fk("Gain", &[77, 96]),
            fk("Ratio", &[116, 97]),
            fk("Knee", &[117, 98]),
        ],
    },
];

// ----------------------------------------------------------------------------
// General / frequently-used and miscellaneous controls.
// ----------------------------------------------------------------------------

/// The "General/Frequently Used Controls" bypass shortcuts and pedal positions.
pub const GENERAL: &[Param] = &[
    onoff("DIST BYPASS", 25),
    onoff("MOD BYPASS", 50),
    onoff("DELAY BYPASS", 28),
    onoff("REVERB BYPASS", 36),
    onoff("FX LOOP BYPASS", 107),
    onoff("FX1 BYPASS", 63),
    onoff("FX2 BYPASS", 86),
    onoff("WAH BYPASS", 43),
    onoff("AMP BYPASS", 111),
    knob("VOLUME PEDAL POSITION", 7),
    knob("WAH POSITION", 4),
    knob("MULTI FX CONTROL", 11),
    knob("TAP TEMPO", 64),
];

/// "Miscellaneous MIDI Controls": the multi-FX and rig-volume pedal assignments.
/// Bank select (CC 32: `1` = Factory rigs, `0` = User rigs) precedes a Program
/// Change to select a rig; see `docs/eleven-rack-sysex-protocol.adoc`.
pub const MISC: &[Param] = &[
    knob("MULTI FX PEDAL POSITION", 11),
    knob("RIG VOLUME PEDAL POSITION", 17),
    sw("BANK CHANGE", 32, "User Rigs", "Factory Rigs"),
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
/// * `amp_ctx` — resolve an amp model's parameter (e.g. `PRESENCE` is CC 13 on
///   Tweed Bass but CC 3 on M-2 Lead).
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
        let dc = amp("DC Modern Crunch").unwrap();
        assert_eq!(dc.param("GAIN").unwrap().cc, 13);
        assert_eq!(dc.param("MASTER").unwrap().cc, 10);
        assert_eq!(dc.param("PRESENCE").unwrap().cc, 21);
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
        let c1 = effect("C1 Chorus/Vibrato").unwrap();
        let byp = c1.param("Bypass").unwrap();
        assert_eq!(c1.cc_in(byp, Slot::Mod), Some(50));
        assert_eq!(c1.cc_in(byp, Slot::Fx1), Some(63));
        assert_eq!(c1.cc_in(byp, Slot::Fx2), Some(86));
    }

    #[test]
    fn stepped_describe_picks_the_right_label() {
        let rev = effect("Eleven SR (Stereo Reverb)").unwrap();
        let ty = rev.param("Type").unwrap();
        assert_eq!(ty.kind.describe(70), "Concert Hall");
        assert_eq!(ty.kind.describe(0), "Echo Room");
        assert_eq!(ty.kind.describe(127), "Early Reflect 2");
    }

    #[test]
    fn expansion_effects_now_fully_catalogued() {
        // The Expansion Pack effects are present with their full parameters.
        let peq = effect("Parametric EQ").unwrap();
        assert_eq!(peq.slots, [Slot::Fx1, Slot::Fx2]);
        assert_eq!(peq.params.len(), 14);
        assert_eq!(peq.cc_in(peq.param("H Q").unwrap(), Slot::Fx2), Some(70));
        // Multi Chorus: Lo Cut/Width share one CC across all slots.
        let mc = effect("Multi Chorus").unwrap();
        let lo = mc.param("Lo Cut").unwrap();
        assert_eq!(mc.cc_in(lo, Slot::Mod), Some(89));
        assert_eq!(mc.cc_in(lo, Slot::Fx2), Some(89));
        // White Boost / DC Distortion are fixed-block, 5 params.
        assert_eq!(effect("DC Distortion").unwrap().slots, [Slot::Fixed]);
        assert_eq!(
            effect("White Boost").unwrap().param("Volume").unwrap().cc,
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
        // Amp context: PRESENCE differs per model.
        assert_eq!(
            resolve_cc("PRESENCE", Some("Tweed Bass"), None).unwrap().0,
            13
        );
        assert_eq!(resolve_cc("PRESENCE", Some("M-2 Lead"), None).unwrap().0, 3);
        // Effect context: an effect's CC differs per slot.
        assert_eq!(
            resolve_cc("Rate", None, Some(("Multi Chorus", Slot::Fx1)))
                .unwrap()
                .0,
            20
        );
        assert_eq!(
            resolve_cc("Rate", None, Some(("Multi Chorus", Slot::Fx2)))
                .unwrap()
                .0,
            113
        );
        // Global control needs no context.
        assert_eq!(resolve_cc("DIST BYPASS", None, None).unwrap().0, 25);
        assert_eq!(resolve_cc("AMP OUTPUT", None, None).unwrap().0, 92);
        // Unknown / missing context.
        assert!(resolve_cc("PRESENCE", None, None).is_none()); // ambiguous amp param, no ctx
        assert!(resolve_cc("nope", Some("Tweed Bass"), None).is_none());
    }

    #[test]
    fn switch_describe_splits_at_64() {
        let p = onoff("X", 1);
        assert_eq!(p.kind.describe(63), "Off");
        assert_eq!(p.kind.describe(64), "On");
    }
}
