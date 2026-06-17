//! The US-16x08 control catalog.
//!
//! Each [`Control`] maps to an ALSA control element by name; per-channel and
//! per-output controls additionally carry an index. The names, value kinds, and
//! ranges are ported from the kernel `snd-usb-audio` US-16x08 quirk as used by
//! the original `tascamgtk` C++ application (`OAlsa.h`, plus the dial ranges
//! configured in `OMainWnd.cpp`).

/// Number of input channels (per-channel controls are indexed `0..16`).
pub const NUM_CHANNELS: u32 = 16;

/// Number of physical line outputs (the routing control is indexed `0..8`).
pub const NUM_OUTPUTS: u32 = 8;

/// Display labels for the [`Control::CompRatio`] enum, in value order.
///
/// Ported verbatim from `cp_ration_map` in `OComp.cpp`.
pub const COMP_RATIO_VALUES: &[&str] = &[
    "1.0:1", "1.1:1", "1.3:1", "1.5:1", "1.7:1", "2.0:1", "2.5:1", "3.0:1", "3.5:1", "4.0:1",
    "5.0:1", "6.0:1", "8.0:1", "16.0:1", "inf:1",
];

/// Display labels for the [`Control::LineOutRoute`] enum, in value order.
///
/// Ported from the combo population in `ORoute.cpp`.
pub const ROUTE_VALUES: &[&str] = &[
    "Master Left",
    "Master Right",
    "Output 1",
    "Output 2",
    "Output 3",
    "Output 4",
    "Output 5",
    "Output 6",
    "Output 7",
    "Output 8",
];

/// Where a control lives, and therefore how many indices it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scope {
    /// A single global element, addressed with index `0`.
    Global,
    /// One element per input channel (`0..16`).
    Channel,
    /// One element per line output (`0..8`).
    Output,
}

impl Scope {
    /// Number of valid indices for this scope.
    #[must_use]
    pub const fn count(self) -> u32 {
        match self {
            Self::Global => 1,
            Self::Channel => NUM_CHANNELS,
            Self::Output => NUM_OUTPUTS,
        }
    }
}

/// The value kind of a control, with its hardware range/defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// An on/off switch.
    Bool,
    /// A bounded integer (inclusive `min..=max`).
    Int {
        /// Inclusive minimum accepted by the hardware.
        min: i32,
        /// Inclusive maximum accepted by the hardware.
        max: i32,
        /// Power-on / reset default.
        default: i32,
    },
    /// An enumerated choice; the value is an index into `values`.
    Enum {
        /// Display labels in value order.
        values: &'static [&'static str],
        /// Default value index.
        default: i32,
    },
    /// A read-only integer array (the level-meter block).
    Meter,
}

/// A concrete control value, tagged by kind.
///
/// `Meter` controls are not read through [`Value`]; use
/// [`crate::Us16x08::meters`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Value {
    /// Boolean switch value.
    Bool(bool),
    /// Bounded integer value.
    Int(i32),
    /// Enum choice, as an index into the control's value list.
    Enum(i32),
}

/// Static metadata describing one control.
#[derive(Debug, Clone, Copy)]
struct Spec {
    name: &'static str,
    aliases: &'static [&'static str],
    kind: Kind,
    scope: Scope,
}

/// A DSP/mixer control on the US-16x08.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Control {
    /// Per-channel input fader level (log dB curve; see [`crate::convert`]).
    LineVolume,
    /// Per-channel mute.
    MuteSwitch,
    /// Per-channel pan (0 = hard left, 254 = hard right, 127 = centre).
    Pan,
    /// Per-channel phase invert.
    PhaseSwitch,
    /// Per-channel EQ master enable.
    EqSwitch,
    /// EQ low band gain.
    EqLowVolume,
    /// EQ low band frequency.
    EqLowFreq,
    /// EQ mid-low band gain.
    EqMidLowVolume,
    /// EQ mid-low band frequency.
    EqMidLowFreq,
    /// EQ mid-low band width (Q).
    EqMidLowQ,
    /// EQ mid-high band gain.
    EqMidHighVolume,
    /// EQ mid-high band frequency.
    EqMidHighFreq,
    /// EQ mid-high band width (Q).
    EqMidHighQ,
    /// EQ high band gain.
    EqHighVolume,
    /// EQ high band frequency.
    EqHighFreq,
    /// Per-channel compressor threshold.
    CompThreshold,
    /// Per-channel compressor make-up gain.
    CompGain,
    /// Per-channel compressor attack.
    CompAttack,
    /// Per-channel compressor release.
    CompRelease,
    /// Per-channel compressor ratio (see [`COMP_RATIO_VALUES`]).
    CompRatio,
    /// Per-channel compressor enable.
    CompSwitch,
    /// Global DSP bypass (true hardware bypass).
    DspBypass,
    /// Global "bus out" / compressor-to-stereo switch.
    BussOut,
    /// Global master mute.
    MasterMute,
    /// Global master output volume (same dB curve as [`Control::LineVolume`]).
    MasterVolume,
    /// Per-output routing source (see [`ROUTE_VALUES`]).
    LineOutRoute,
    /// Global read-only level-meter block (see [`crate::Meters`]).
    LevelMeter,
}

impl Control {
    /// Every control, in a stable order. Useful for enumeration, mock seeding,
    /// and CLI listings.
    pub const ALL: &'static [Control] = &[
        Self::LineVolume,
        Self::MuteSwitch,
        Self::Pan,
        Self::PhaseSwitch,
        Self::EqSwitch,
        Self::EqLowVolume,
        Self::EqLowFreq,
        Self::EqMidLowVolume,
        Self::EqMidLowFreq,
        Self::EqMidLowQ,
        Self::EqMidHighVolume,
        Self::EqMidHighFreq,
        Self::EqMidHighQ,
        Self::EqHighVolume,
        Self::EqHighFreq,
        Self::CompThreshold,
        Self::CompGain,
        Self::CompAttack,
        Self::CompRelease,
        Self::CompRatio,
        Self::CompSwitch,
        Self::DspBypass,
        Self::BussOut,
        Self::MasterMute,
        Self::MasterVolume,
        Self::LineOutRoute,
        Self::LevelMeter,
    ];

    #[allow(clippy::too_many_lines)] // a flat catalog table reads better than splitting it
    const fn spec(self) -> Spec {
        // Switches and meters.
        const BOOL: Kind = Kind::Bool;
        match self {
            Self::LineVolume => Spec {
                name: "Line Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 133,
                    default: 127,
                },
                scope: Scope::Channel,
            },
            Self::MuteSwitch => Spec {
                name: "Mute Switch",
                aliases: &[],
                kind: BOOL,
                scope: Scope::Channel,
            },
            Self::Pan => Spec {
                name: "Pan Left-Right Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 254,
                    default: 127,
                },
                scope: Scope::Channel,
            },
            Self::PhaseSwitch => Spec {
                name: "Phase Switch",
                aliases: &[],
                kind: BOOL,
                scope: Scope::Channel,
            },
            Self::EqSwitch => Spec {
                name: "EQ Switch",
                aliases: &[],
                kind: BOOL,
                scope: Scope::Channel,
            },
            Self::EqLowVolume => Spec {
                name: "EQ Low Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 24,
                    default: 12,
                },
                scope: Scope::Channel,
            },
            // Kernels < 5.10 spelled the EQ frequency controls "Frequence";
            // 5.10+ corrected them to "Frequency". Carry both so the device
            // layer can resolve whichever the loaded card exposes.
            Self::EqLowFreq => Spec {
                name: "EQ Low Frequency",
                aliases: &["EQ Low Frequence"],
                kind: Kind::Int {
                    min: 0,
                    max: 31,
                    default: 5,
                },
                scope: Scope::Channel,
            },
            Self::EqMidLowVolume => Spec {
                name: "EQ MidLow Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 24,
                    default: 12,
                },
                scope: Scope::Channel,
            },
            Self::EqMidLowFreq => Spec {
                name: "EQ MidLow Frequency",
                aliases: &["EQ MidLow Frequence"],
                kind: Kind::Int {
                    min: 0,
                    max: 63,
                    default: 14,
                },
                scope: Scope::Channel,
            },
            Self::EqMidLowQ => Spec {
                name: "EQ MidLow Q",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 6,
                    default: 2,
                },
                scope: Scope::Channel,
            },
            Self::EqMidHighVolume => Spec {
                name: "EQ MidHigh Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 24,
                    default: 12,
                },
                scope: Scope::Channel,
            },
            Self::EqMidHighFreq => Spec {
                name: "EQ MidHigh Frequency",
                aliases: &["EQ MidHigh Frequence"],
                kind: Kind::Int {
                    min: 0,
                    max: 63,
                    default: 27,
                },
                scope: Scope::Channel,
            },
            Self::EqMidHighQ => Spec {
                name: "EQ MidHigh Q",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 6,
                    default: 2,
                },
                scope: Scope::Channel,
            },
            Self::EqHighVolume => Spec {
                name: "EQ High Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 24,
                    default: 12,
                },
                scope: Scope::Channel,
            },
            Self::EqHighFreq => Spec {
                name: "EQ High Frequency",
                aliases: &["EQ High Frequence"],
                kind: Kind::Int {
                    min: 0,
                    max: 31,
                    default: 15,
                },
                scope: Scope::Channel,
            },
            Self::CompThreshold => Spec {
                name: "Compressor Threshold Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 32,
                    default: 32,
                },
                scope: Scope::Channel,
            },
            Self::CompGain => Spec {
                name: "Compressor Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 20,
                    default: 0,
                },
                scope: Scope::Channel,
            },
            Self::CompAttack => Spec {
                name: "Compressor Attack",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 198,
                    default: 0,
                },
                scope: Scope::Channel,
            },
            Self::CompRelease => Spec {
                name: "Compressor Release",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 99,
                    default: 0,
                },
                scope: Scope::Channel,
            },
            Self::CompRatio => Spec {
                name: "Compressor Ratio",
                aliases: &[],
                kind: Kind::Enum {
                    values: COMP_RATIO_VALUES,
                    default: 0,
                },
                scope: Scope::Channel,
            },
            Self::CompSwitch => Spec {
                name: "Compressor Switch",
                aliases: &[],
                kind: BOOL,
                scope: Scope::Channel,
            },
            Self::DspBypass => Spec {
                name: "DSP Bypass Switch",
                aliases: &[],
                kind: BOOL,
                scope: Scope::Global,
            },
            Self::BussOut => Spec {
                name: "Buss Out Switch",
                aliases: &[],
                kind: BOOL,
                scope: Scope::Global,
            },
            Self::MasterMute => Spec {
                name: "Master Mute Switch",
                aliases: &[],
                kind: BOOL,
                scope: Scope::Global,
            },
            Self::MasterVolume => Spec {
                name: "Master Volume",
                aliases: &[],
                kind: Kind::Int {
                    min: 0,
                    max: 133,
                    default: 127,
                },
                scope: Scope::Global,
            },
            Self::LineOutRoute => Spec {
                name: "Line Out Route",
                aliases: &[],
                kind: Kind::Enum {
                    values: ROUTE_VALUES,
                    default: 0,
                },
                scope: Scope::Output,
            },
            Self::LevelMeter => Spec {
                name: "Level Meter",
                aliases: &[],
                kind: Kind::Meter,
                scope: Scope::Global,
            },
        }
    }

    /// The canonical ALSA control name.
    #[must_use]
    pub const fn alsa_name(self) -> &'static str {
        self.spec().name
    }

    /// Alternate ALSA names this control has gone by (e.g. kernel spelling
    /// changes). Empty for most controls.
    #[must_use]
    pub const fn alsa_aliases(self) -> &'static [&'static str] {
        self.spec().aliases
    }

    /// The control's value kind and range.
    #[must_use]
    pub const fn kind(self) -> Kind {
        self.spec().kind
    }

    /// The control's scope (how many indices it has).
    #[must_use]
    pub const fn scope(self) -> Scope {
        self.spec().scope
    }

    /// A stable, friendly kebab-case token for this control, used by the CLI and
    /// config files instead of the quoted ALSA name. Inverse of [`Self::from_key`].
    #[must_use]
    pub const fn cli_key(self) -> &'static str {
        match self {
            Self::LineVolume => "line-volume",
            Self::MuteSwitch => "mute",
            Self::Pan => "pan",
            Self::PhaseSwitch => "phase",
            Self::EqSwitch => "eq-enable",
            Self::EqLowVolume => "eq-low-volume",
            Self::EqLowFreq => "eq-low-freq",
            Self::EqMidLowVolume => "eq-midlow-volume",
            Self::EqMidLowFreq => "eq-midlow-freq",
            Self::EqMidLowQ => "eq-midlow-q",
            Self::EqMidHighVolume => "eq-midhigh-volume",
            Self::EqMidHighFreq => "eq-midhigh-freq",
            Self::EqMidHighQ => "eq-midhigh-q",
            Self::EqHighVolume => "eq-high-volume",
            Self::EqHighFreq => "eq-high-freq",
            Self::CompThreshold => "comp-threshold",
            Self::CompGain => "comp-gain",
            Self::CompAttack => "comp-attack",
            Self::CompRelease => "comp-release",
            Self::CompRatio => "comp-ratio",
            Self::CompSwitch => "comp-enable",
            Self::DspBypass => "dsp-bypass",
            Self::BussOut => "buss-out",
            Self::MasterMute => "master-mute",
            Self::MasterVolume => "master-volume",
            Self::LineOutRoute => "route",
            Self::LevelMeter => "meter",
        }
    }

    /// Look up a control by its [`Self::cli_key`] token.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Control> {
        Self::ALL.iter().copied().find(|c| c.cli_key() == key)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn cli_keys_are_unique() {
        let mut seen = HashSet::new();
        for &c in Control::ALL {
            assert!(
                seen.insert(c.cli_key()),
                "duplicate cli_key: {}",
                c.cli_key()
            );
        }
    }

    #[test]
    fn cli_key_round_trips_through_from_key() {
        for &c in Control::ALL {
            assert_eq!(Control::from_key(c.cli_key()), Some(c));
        }
        assert_eq!(Control::from_key("nonsuch"), None);
    }
}
