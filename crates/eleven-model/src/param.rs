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
//! * **Effects** — an effect's CCs depend on *where it sits in the chain*. The
//!   dedicated-block effects (distortion, wah, delay, reverb, FX loop) have fixed
//!   CCs; the modulation-type effects can occupy the **Mod / FX1 / FX2** slots and
//!   take a slot-dependent CC. Every slot effect shares the *same positional CC
//!   table per slot* ([`MOD_SLOT_CC`] / [`FX1_SLOT_CC`] / [`FX2_SLOT_CC`]): the CC
//!   is a function of `(slot, parameter position)`, not of the effect. See
//!   [`slot_cc`].
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

/// One catalogued parameter: a display `name`, its MIDI `cc` (the remote-control
/// continuous-controller number — *not* a `SysEx` address; for slot effects it is
/// the primary-slot CC, see [`slot_cc`]), and its value [`Kind`].
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

const fn stepped(name: &'static str, cc: u8, steps: &'static [Step]) -> Param {
    Param {
        name,
        cc,
        kind: Kind::Stepped(steps),
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

/// Where an effect sits in the signal chain, which determines how its CCs are
/// assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Placement {
    /// A dedicated block (distortion, wah, delay, reverb, FX loop): the [`Param`]
    /// CCs are the real, fixed CCs.
    Fixed,
    /// A modulation-type effect that can occupy **Mod / FX1 / FX2**. The [`Param`]
    /// `cc` is the *Mod-slot* CC; the FX1/FX2 CCs come from [`slot_cc`] by position.
    ModFx,
    /// An effect that can occupy **FX1 / FX2** only (graphic EQ, gray compressor).
    /// The [`Param`] `cc` is the *FX1-slot* CC; the FX2 CC comes from [`slot_cc`].
    Fx12,
}

/// A chain slot a slot-based effect can occupy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Slot {
    /// The dedicated modulation slot.
    Mod,
    /// General-purpose effect slot 1.
    Fx1,
    /// General-purpose effect slot 2.
    Fx2,
}

/// CC numbers by parameter position when a slot effect sits in the **Mod** slot.
pub const MOD_SLOT_CC: &[u8] = &[50, 61, 52, 53, 54, 57];
/// CC numbers by parameter position when a slot effect sits in the **FX1** slot.
pub const FX1_SLOT_CC: &[u8] = &[63, 20, 42, 60, 77, 116, 117];
/// CC numbers by parameter position when a slot effect sits in the **FX2** slot.
pub const FX2_SLOT_CC: &[u8] = &[86, 113, 114, 115, 96, 97, 98];

/// The CC number for a slot effect's parameter at `position`, in the given `slot`.
/// Returns `None` if the slot has no CC for that position.
#[must_use]
pub fn slot_cc(slot: Slot, position: usize) -> Option<u8> {
    let table = match slot {
        Slot::Mod => MOD_SLOT_CC,
        Slot::Fx1 => FX1_SLOT_CC,
        Slot::Fx2 => FX2_SLOT_CC,
    };
    table.get(position).copied()
}

/// One effect and its parameters. For [`Placement::Fixed`] the [`Param`] CCs are
/// final; for slot effects use [`slot_cc`] with the parameter's position to get the
/// CC in a given [`Slot`].
#[derive(Debug, Clone, Copy)]
pub struct Effect {
    /// Effect name as in the User Guide.
    pub name: &'static str,
    /// How the effect is placed in the chain.
    pub placement: Placement,
    /// The effect's parameters, in chart order (the index is the slot position).
    pub params: &'static [Param],
}

impl Effect {
    /// Find a parameter by case-insensitive name.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// All catalogued effects (User Guide Ch.11).
pub const EFFECTS: &[Effect] = &[
    // --- Distortion block (BYPASS 25) ---
    Effect {
        name: "Black Op Distortion",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 25),
            knob("DISTORTION", 27),
            knob("CUT", 78),
            knob("VOLUME", 79),
        ],
    },
    Effect {
        name: "Green JRC Overdrive",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 25),
            knob("DRIVE", 27),
            knob("TONE", 78),
            knob("LEVEL", 79),
        ],
    },
    Effect {
        name: "Tri-Knob Fuzz",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 25),
            knob("VOLUME", 27),
            knob("SUSTAIN", 78),
            knob("TONE", 79),
        ],
    },
    // --- Wah block (BYPASS 43) ---
    Effect {
        name: "Black Wah",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 43), knob("POSITION", 4)],
    },
    Effect {
        name: "Shine Wah",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 43), knob("POSITION", 4)],
    },
    // --- Delay block (BYPASS 28) ---
    Effect {
        name: "BBD Delay",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 28),
            knob("DELAY", 62),
            stepped("SYNC", 33, FX_SYNC_STEPS),
            knob("MIX", 85),
            knob("FEEDBACK", 35),
            knob("INPUT LEVEL", 87),
            sw("MOD", 34, "Chorus", "Vibrato"),
            knob("DEPTH", 48),
            onoff("NOISE", 55),
            onoff("EXPANDED DELAY", 49),
        ],
    },
    Effect {
        name: "Tape Echo",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 28),
            knob("DELAY", 62),
            stepped("SYNC", 33, FX_SYNC_STEPS),
            knob("MIX", 85),
            knob("FEEDBACK", 35),
            knob("REC LEVEL", 87),
            knob("HEAD", 34),
            knob("WOW", 48),
            onoff("HISS", 55),
            onoff("EXPANDED DELAY", 49),
        ],
    },
    // --- Reverb block (BYPASS 36) ---
    Effect {
        name: "Blackpanel Spring Reverb",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 36),
            knob("MIX", 18),
            knob("DECAY", 38),
            knob("TONE", 40),
        ],
    },
    Effect {
        name: "Eleven SR (Stereo Reverb)",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 36),
            knob("MIX", 18),
            knob("DECAY", 38),
            knob("TONE", 40),
            knob("PRE-DELAY", 39),
            stepped("TYPE", 76, REVERB_TYPE_STEPS),
        ],
    },
    // --- FX Loop block ---
    Effect {
        name: "FX Loop",
        placement: Placement::Fixed,
        params: &[
            onoff("BYPASS", 107),
            knob("SEND", 19),
            knob("RETURN", 108),
            knob("MIX", 88),
        ],
    },
    // --- Misc single-CC effects ---
    Effect {
        name: "Tuner",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 69)],
    },
    Effect {
        name: "Tap Tempo",
        placement: Placement::Fixed,
        params: &[knob("TAP", 64)],
    },
    Effect {
        name: "Volume Pedal",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 75), knob("POSITION", 7)],
    },
    // --- Mod/FX1/FX2 slot effects (cc = Mod-slot CC; use slot_cc for FX1/FX2) ---
    Effect {
        name: "C1 Chorus/Vibrato",
        placement: Placement::ModFx,
        params: &[
            onoff("BYPASS", 50),
            knob("CHORUS", 61),
            knob("RATE", 52),
            stepped("SYNC", 53, FX_SYNC_STEPS),
            knob("DEPTH", 54),
            sw("CHORUS/VIBRATO", 57, "Chorus", "Vibrato"),
        ],
    },
    Effect {
        name: "Flanger",
        placement: Placement::ModFx,
        params: &[
            onoff("BYPASS", 50),
            knob("PRE-DELAY", 61),
            knob("DEPTH", 52),
            knob("RATE", 53),
            stepped("SYNC", 54, FX_SYNC_STEPS),
            knob("FEEDBACK", 57),
        ],
    },
    Effect {
        name: "Orange Phaser",
        placement: Placement::ModFx,
        params: &[
            onoff("BYPASS", 50),
            knob("RATE", 61),
            stepped("SYNC", 52, FX_SYNC_STEPS),
        ],
    },
    Effect {
        name: "Roto Speaker",
        placement: Placement::ModFx,
        params: &[
            onoff("BYPASS", 50),
            stepped("SPEED", 61, ROTO_SPEED_STEPS),
            knob("BALANCE", 52),
            stepped("TYPE", 53, ROTO_TYPE_STEPS),
        ],
    },
    Effect {
        name: "Vibe Phaser",
        placement: Placement::ModFx,
        params: &[
            onoff("BYPASS", 50),
            knob("VOLUME", 61),
            knob("DEPTH", 52),
            knob("RATE", 53),
            stepped("SYNC", 54, FX_SYNC_STEPS),
            sw("CHORUS/VIBRATO", 57, "Chorus", "Vibrato"),
        ],
    },
    // --- FX1/FX2-only slot effects (cc = FX1-slot CC; use slot_cc for FX2) ---
    Effect {
        name: "Graphic EQ",
        placement: Placement::Fx12,
        params: &[
            onoff("BYPASS", 63),
            knob("100 Hz", 20),
            knob("370 Hz", 42),
            knob("800 Hz", 60),
            knob("2 kHz", 77),
            knob("3.25 kHz", 116),
            knob("OUTPUT", 117),
        ],
    },
    Effect {
        name: "Gray Compressor",
        placement: Placement::Fx12,
        params: &[onoff("BYPASS", 63), knob("SUSTAIN", 20), knob("LEVEL", 42)],
    },
    // --- Expansion Pack effects (firmware 2.x) ---
    // Names are the device's own (block 0x20 model catalog). Their full parameter
    // lists await the Expansion Pack documentation (the base User Guide v8.0.4
    // predates them); only BYPASS is known, from the block/slot CC scheme. These
    // names appear in [`PARAMS_PENDING`].
    Effect {
        name: "Multi Chorus",
        placement: Placement::ModFx,
        params: &[onoff("BYPASS", 50)],
    },
    Effect {
        name: "Parametric EQ",
        placement: Placement::Fx12,
        params: &[onoff("BYPASS", 63)],
    },
    Effect {
        name: "Dyn3 Compressor",
        placement: Placement::Fx12,
        params: &[onoff("BYPASS", 63)],
    },
    Effect {
        name: "White Boost",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 25)],
    },
    Effect {
        name: "DC Distortion",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 25)],
    },
    Effect {
        name: "Dyn Delay",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 28)],
    },
    Effect {
        name: "EP Tape Echo",
        placement: Placement::Fixed,
        params: &[onoff("BYPASS", 28)],
    },
];

/// Effects whose full parameter set is *not yet catalogued* — the Expansion Pack
/// additions, present in [`EFFECTS`] by name (and BYPASS) but awaiting the
/// Expansion Pack CC chart for the rest of their parameters.
pub const PARAMS_PENDING: &[&str] = &[
    "Multi Chorus",
    "Parametric EQ",
    "Dyn3 Compressor",
    "White Boost",
    "DC Distortion",
    "Dyn Delay",
    "EP Tape Echo",
];

/// Whether `name` is an effect present by name but with parameters still pending
/// (see [`PARAMS_PENDING`]).
#[must_use]
pub fn params_pending(name: &str) -> bool {
    PARAMS_PENDING.iter().any(|p| p.eq_ignore_ascii_case(name))
}

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
    fn slot_cc_tables_align_with_modfx_primary_cc() {
        // For a ModFx effect, each param's stored cc is the Mod-slot CC at its
        // position, and FX1/FX2 come from the slot tables.
        let c1 = effect("C1 Chorus/Vibrato").unwrap();
        for (pos, p) in c1.params.iter().enumerate() {
            assert_eq!(slot_cc(Slot::Mod, pos), Some(p.cc));
            assert_eq!(slot_cc(Slot::Fx1, pos), FX1_SLOT_CC.get(pos).copied());
            assert_eq!(slot_cc(Slot::Fx2, pos), FX2_SLOT_CC.get(pos).copied());
        }
    }

    #[test]
    fn fx12_primary_cc_is_fx1_slot() {
        let eq = effect("Graphic EQ").unwrap();
        for (pos, p) in eq.params.iter().enumerate() {
            assert_eq!(slot_cc(Slot::Fx1, pos), Some(p.cc));
        }
    }

    #[test]
    fn stepped_describe_picks_the_right_label() {
        let rev = effect("Eleven SR (Stereo Reverb)").unwrap();
        let ty = rev.param("TYPE").unwrap();
        assert_eq!(ty.kind.describe(70), "Concert Hall");
        assert_eq!(ty.kind.describe(0), "Echo Room");
        assert_eq!(ty.kind.describe(127), "Early Reflect 2");
    }

    #[test]
    fn expansion_pack_effects_present_but_flagged_pending() {
        for name in PARAMS_PENDING {
            let fx = effect(name).unwrap_or_else(|| panic!("{name} in EFFECTS"));
            // Present with at least a BYPASS, and flagged as parameters-pending.
            assert!(fx.param("BYPASS").is_some(), "{name} has BYPASS");
            assert!(params_pending(name));
        }
        // A fully-catalogued effect is not flagged pending.
        assert!(!params_pending("Graphic EQ"));
        assert_eq!(PARAMS_PENDING.len(), 7);
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
    fn switch_describe_splits_at_64() {
        let p = onoff("X", 1);
        assert_eq!(p.kind.describe(63), "Off");
        assert_eq!(p.kind.describe(64), "On");
    }
}
