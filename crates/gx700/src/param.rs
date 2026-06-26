//! The GX-700 parameter catalog.
//!
//! Each [`Param`] names one editable value in a GX-700 effect block, carrying a
//! value [`Kind`] (bool / bounded int / enum), the effect [`Block`] it lives in,
//! and its one-byte offset within that block.
//!
//! # Addresses
//!
//! The GX-700 uses 4-byte Roland addresses. A block occupies offset `00 00 XX 00`
//! (where `XX` is [`Block::base`]); a parameter sits at `00 00 XX NN` within a
//! patch. For *live* editing the catalog targets the individual temporary buffer,
//! so a parameter's wire address is [`Param::address`] = `08 00 XX NN`. See
//! `docs/gx700-sysex-protocol.adoc` and the Roland *GX-700 MIDI Implementation*.
//!
//! # Units
//!
//! Values are **raw 7-bit device units**. Display-unit conversions (a tone shown
//! as `-50..+50`, an EQ gain as `-20..+20 dB`, a delay time in ms) are layered
//! above the catalog and are not applied here; a parameter's range is its raw
//! device range.
//!
//! # Coverage
//!
//! Every **single-byte** parameter of every effect block is catalogued, including
//! the full Modulation block and the Chorus/Reverb pre-delay and Reverb time. The
//! complete per-offset map is `docs/gx700-patch-data-format.adoc`, and the
//! `catalog_bank` integration test cross-checks every catalogued parameter against
//! a real 100-patch bank.
//!
//! Still deferred (they are **multi-byte**, needing a model that spans >1 byte, or
//! belong to patch data rather than live edits):
//! - Delay tempo (`0x03`/`0x04`) and tap times (`0x05`..`0x0A`) — 14-bit values.
//! - Modulation resonance (`0x0B`/`0x0C`) and flanger separation (`0x0E`/`0x0F`) —
//!   nibble-split, and the 36-byte harmonist scale map (`0x29`..`0x4C`).
//! - The Level/Chain control assigns (`0x1A`..`0x41`).

/// Base address (`08 00 00 00`) of the *individual* temporary buffer: writing one
/// parameter here edits the current sound live, with immediate effect.
pub const TEMP_INDIVIDUAL_BASE: [u8; 4] = [0x08, 0x00, 0x00, 0x00];

/// An effect block in the GX-700's signal chain. Each occupies patch offset
/// `00 00 <base> 00`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Block {
    /// Patch common: output level, chain order, name, control assigns.
    LevelChain,
    /// Compressor / limiter.
    Compressor,
    /// Wah.
    Wah,
    /// Overdrive / distortion.
    Distortion,
    /// Preamp / amp model.
    Preamp,
    /// External effects loop.
    Loop,
    /// 3-band equalizer.
    Equalizer,
    /// Speaker simulator.
    SpeakerSim,
    /// Noise suppressor.
    NoiseSuppressor,
    /// Modulation (flanger / phaser / pitch / harmonist / vibrato / ring / humanizer).
    Modulation,
    /// Delay.
    Delay,
    /// Chorus.
    Chorus,
    /// Tremolo / pan.
    TremoloPan,
    /// Reverb.
    Reverb,
}

impl Block {
    /// The block's base byte (`XX` in the `00 00 XX 00` patch offset).
    #[must_use]
    pub const fn base(self) -> u8 {
        match self {
            Block::LevelChain => 0x00,
            Block::Compressor => 0x01,
            Block::Wah => 0x02,
            Block::Distortion => 0x03,
            Block::Preamp => 0x04,
            Block::Loop => 0x05,
            Block::Equalizer => 0x06,
            Block::SpeakerSim => 0x07,
            Block::NoiseSuppressor => 0x08,
            Block::Modulation => 0x09,
            Block::Delay => 0x0A,
            Block::Chorus => 0x0B,
            Block::TremoloPan => 0x0C,
            Block::Reverb => 0x0D,
        }
    }

    /// The block whose [`Self::base`] is `base`, or `None` if none matches. Used
    /// to label chain-order bytes.
    #[must_use]
    pub const fn from_base(base: u8) -> Option<Block> {
        Some(match base {
            0x00 => Block::LevelChain,
            0x01 => Block::Compressor,
            0x02 => Block::Wah,
            0x03 => Block::Distortion,
            0x04 => Block::Preamp,
            0x05 => Block::Loop,
            0x06 => Block::Equalizer,
            0x07 => Block::SpeakerSim,
            0x08 => Block::NoiseSuppressor,
            0x09 => Block::Modulation,
            0x0A => Block::Delay,
            0x0B => Block::Chorus,
            0x0C => Block::TremoloPan,
            0x0D => Block::Reverb,
            _ => return None,
        })
    }

    /// A human-readable block label, for listings.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Block::LevelChain => "Level/Chain",
            Block::Compressor => "Compressor",
            Block::Wah => "Wah",
            Block::Distortion => "Distortion",
            Block::Preamp => "Preamp",
            Block::Loop => "Loop",
            Block::Equalizer => "Equalizer",
            Block::SpeakerSim => "Speaker Sim",
            Block::NoiseSuppressor => "Noise Suppressor",
            Block::Modulation => "Modulation",
            Block::Delay => "Delay",
            Block::Chorus => "Chorus",
            Block::TremoloPan => "Tremolo/Pan",
            Block::Reverb => "Reverb",
        }
    }
}

/// The value kind of a parameter, with its raw device range/defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// An on/off switch (typically a block enable).
    Bool,
    /// A bounded integer (inclusive `min..=max`), in raw device units.
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
}

/// A concrete parameter value, tagged by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Value {
    /// Boolean switch value.
    Bool(bool),
    /// Bounded integer value, in raw device units.
    Int(i32),
    /// Enum choice, as an index into the parameter's value list.
    Enum(i32),
}

/// Compressor mode (Table 2).
pub const COMP_TYPE_VALUES: &[&str] = &["Compressor", "Limiter"];
/// Wah mode (Table 3).
pub const WAH_MODE_VALUES: &[&str] = &["Pedal Wah", "SW-Pedal Wah", "Auto Wah"];
/// Wah auto polarity (Table 3).
pub const WAH_POLARITY_VALUES: &[&str] = &["Down", "Up"];
/// Distortion model (Table 4).
pub const DIST_TYPE_VALUES: &[&str] = &[
    "Vintage OD",
    "Turbo OD",
    "Blues",
    "Distortion",
    "Turbo DS",
    "Metal",
    "Fuzz",
];
/// Preamp model (Table 5).
pub const PREAMP_TYPE_VALUES: &[&str] = &[
    "JC-120",
    "Clean Twin",
    "Match Drive",
    "BG Lead",
    "MS1959 (I)",
    "MS1959 (II)",
    "MS1959 (I+II)",
    "SLDN Lead",
    "Metal 5150",
];
/// Preamp gain switch (Table 5).
pub const PREAMP_GAIN_VALUES: &[&str] = &["Low", "Mid", "Hi"];
/// Effects-loop routing (Table 6).
pub const LOOP_MODE_VALUES: &[&str] = &["Series", "Parallel"];
/// Speaker-simulator cabinet (Table 8.1).
pub const SPEAKER_TYPE_VALUES: &[&str] = &[
    "Small",
    "Middle",
    "JC-120",
    "Built-In 1",
    "Built-In 2",
    "Built-In 3",
    "Built-In 4",
    "BG Stack 1",
    "BG Stack 2",
    "MS Stack 1",
    "MS Stack 2",
    "Metal Stack",
];
/// Noise-suppressor detection source (Table 9).
pub const NS_DETECT_VALUES: &[&str] = &["Guitar In", "NS In"];
/// Equalizer mid-band centre frequency (Table 7.1).
pub const EQ_MID_FREQ_VALUES: &[&str] = &[
    "100Hz", "125Hz", "160Hz", "200Hz", "250Hz", "315Hz", "400Hz", "500Hz", "630Hz", "800Hz",
    "1kHz", "1.25kHz", "1.6kHz", "2kHz", "2.5kHz", "3.15kHz", "4kHz", "5kHz", "6.3kHz", "8kHz",
    "10kHz",
];
/// Equalizer mid-band Q (Table 7.2).
pub const EQ_MID_Q_VALUES: &[&str] = &["0.5", "1", "2", "4", "8", "16"];
/// Modulation type (Table 10.1).
pub const MOD_TYPE_VALUES: &[&str] = &[
    "Flanger",
    "Phaser",
    "Pitch Shifter",
    "Harmonist",
    "Vibrato",
    "Ring Modulator",
    "Humanizer",
];
/// Phaser stage count (Modulation offset 0x02).
pub const PHASER_STAGE_VALUES: &[&str] = &["4-Stage", "6-Stage", "8-Stage", "10-Stage", "12-Stage"];
/// Vibrato trigger (Modulation offset 0x03).
pub const VIBRATO_TRIGGER_VALUES: &[&str] = &["Off", "On", "Auto"];
/// Humanizer type / trigger (Modulation offsets 0x05, 0x11).
pub const HUMANIZER_MODE_VALUES: &[&str] = &["Auto", "Pedal"];
/// Humanizer vowels (Modulation offsets 0x06, 0x07).
pub const VOWEL_VALUES: &[&str] = &["a", "e", "i", "o", "u"];
/// Pitch-shifter type (Modulation offset 0x16).
pub const PS_TYPE_VALUES: &[&str] = &["Slow", "Fast", "Mono"];
/// Tempo-synced delay interval (Table 11.1; Delay offset 0x0B).
pub const DELAY_INTERVAL_VALUES: &[&str] = &[
    "1/16",
    "1/8 Triplet",
    "Dotted 1/16",
    "1/8",
    "1/4 Triplet",
    "Dotted 1/8",
    "1/4",
    "Dotted 1/4",
    "1/2",
    "Dotted 1/2",
    "Whole",
];
/// Delay mode (Table 11).
pub const DELAY_MODE_VALUES: &[&str] = &["Normal", "Tempo"];
/// Chorus mode (Table 12).
pub const CHORUS_MODE_VALUES: &[&str] = &["Mono", "Stereo"];
/// Tremolo/pan mode (Table 13): `0:TREMOLO(TRI) 1:TREMOLO(SQR) 2:PAN(TRI) 3:PAN(SQR)`.
pub const TREMOLO_MODE_VALUES: &[&str] =
    &["Tremolo (Tri)", "Tremolo (Sqr)", "Pan (Tri)", "Pan (Sqr)"];
/// Reverb mode (Table 14).
pub const REVERB_MODE_VALUES: &[&str] = &["Room 1", "Room 2", "Hall 1", "Hall 2", "Plate"];
/// High-cut frequency (Table 15), shared by delay/chorus/reverb.
pub const HI_CUT_VALUES: &[&str] = &[
    "500Hz", "630Hz", "800Hz", "1kHz", "1.25kHz", "1.6kHz", "2kHz", "2.5kHz", "3.15kHz", "4kHz",
    "5kHz", "6.3kHz", "8kHz", "10kHz", "12.5kHz", "Flat",
];
/// Low-cut frequency (Table 16), shared by chorus/reverb.
pub const LOW_CUT_VALUES: &[&str] = &[
    "Flat", "55Hz", "110Hz", "165Hz", "220Hz", "280Hz", "340Hz", "400Hz", "500Hz", "640Hz", "800Hz",
];

/// One editable GX-700 parameter.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    key: &'static str,
    block: Block,
    offset: u8,
    kind: Kind,
}

impl Param {
    /// The stable kebab-case key used by the CLI and patch files.
    #[must_use]
    pub const fn key(self) -> &'static str {
        self.key
    }

    /// The effect block this parameter belongs to.
    #[must_use]
    pub const fn block(self) -> Block {
        self.block
    }

    /// A human-readable block label, for listings.
    #[must_use]
    pub const fn block_label(self) -> &'static str {
        self.block.label()
    }

    /// The parameter's one-byte offset within its block.
    #[must_use]
    pub const fn offset(self) -> u8 {
        self.offset
    }

    /// The parameter's value kind and raw range.
    #[must_use]
    pub const fn kind(self) -> Kind {
        self.kind
    }

    /// The 4-byte offset of this parameter within a patch (`00 00 <base> <offset>`).
    #[must_use]
    pub const fn patch_offset(self) -> [u8; 4] {
        [0x00, 0x00, self.block.base(), self.offset]
    }

    /// The 4-byte wire address for a *live* edit, in the individual temporary
    /// buffer (`08 00 <base> <offset>`). Writing here changes the current sound
    /// immediately; reading here returns its current value.
    #[must_use]
    pub const fn address(self) -> [u8; 4] {
        [
            TEMP_INDIVIDUAL_BASE[0],
            TEMP_INDIVIDUAL_BASE[1],
            self.block.base(),
            self.offset,
        ]
    }

    /// Look up a parameter by its [`Self::key`].
    #[must_use]
    pub fn from_key(key: &str) -> Option<Param> {
        ALL.iter().copied().find(|p| p.key == key)
    }
}

/// A bool (typically a block enable) parameter.
const fn b(key: &'static str, block: Block, offset: u8) -> Param {
    Param {
        key,
        block,
        offset,
        kind: Kind::Bool,
    }
}

/// An integer parameter with explicit raw range and default.
const fn i(key: &'static str, block: Block, offset: u8, min: i32, max: i32, default: i32) -> Param {
    Param {
        key,
        block,
        offset,
        kind: Kind::Int { min, max, default },
    }
}

/// A `0..=100` integer parameter, default 50 (the common GX-700 range).
const fn i100(key: &'static str, block: Block, offset: u8) -> Param {
    i(key, block, offset, 0, 100, 50)
}

/// An enum parameter (default index 0).
const fn e(key: &'static str, block: Block, offset: u8, values: &'static [&'static str]) -> Param {
    Param {
        key,
        block,
        offset,
        kind: Kind::Enum { values, default: 0 },
    }
}

use Block::{
    Chorus, Compressor, Delay, Distortion, Equalizer, LevelChain, Loop, Modulation,
    NoiseSuppressor, Preamp, Reverb, SpeakerSim, TremoloPan, Wah,
};

/// Every cataloged parameter, in chain order. Used for enumeration, mock seeding,
/// CLI listings, and patch capture/apply. Transcribed from the Roland *GX-700
/// MIDI Implementation* tables; ranges are raw device units.
pub const ALL: &[Param] = &[
    // Patch common.
    i100("output-level", LevelChain, 0x00),
    // Compressor (Table 2).
    b("comp-enable", Compressor, 0x00),
    e("comp-type", Compressor, 0x01, COMP_TYPE_VALUES),
    i100("comp-sustain", Compressor, 0x02),
    i100("comp-attack", Compressor, 0x03),
    i100("comp-threshold", Compressor, 0x04),
    i100("comp-release", Compressor, 0x05),
    i100("comp-tone", Compressor, 0x06), // raw 0..100 = -50..+50
    i100("comp-level", Compressor, 0x07),
    // Wah (Table 3).
    b("wah-enable", Wah, 0x00),
    e("wah-mode", Wah, 0x01, WAH_MODE_VALUES),
    i100("wah-pedal-freq", Wah, 0x02),
    e("wah-auto-polarity", Wah, 0x03, WAH_POLARITY_VALUES),
    i100("wah-auto-sens", Wah, 0x04),
    i100("wah-peak", Wah, 0x05),
    i("wah-pedal-source", Wah, 0x06, 0, 65, 0), // 0 fixed,1 exp,2 fc200,3..33 cc1..31,34..65 cc64..95
    i100("wah-pedal-min", Wah, 0x07),
    i100("wah-pedal-max", Wah, 0x08),
    i100("wah-auto-rate", Wah, 0x09),
    i100("wah-auto-depth", Wah, 0x0A),
    i100("wah-level", Wah, 0x0C),
    // Distortion (Table 4).
    b("dist-enable", Distortion, 0x00),
    e("dist-type", Distortion, 0x01, DIST_TYPE_VALUES),
    i100("dist-drive", Distortion, 0x02),
    i100("dist-bass", Distortion, 0x03), // raw 0..100 = -50..+50
    i100("dist-treble", Distortion, 0x04), // raw 0..100 = -50..+50
    i100("dist-level", Distortion, 0x05),
    // Preamp (Table 5) -- byte-exact-verified against hardware.
    b("preamp-enable", Preamp, 0x00),
    e("preamp-type", Preamp, 0x01, PREAMP_TYPE_VALUES),
    i100("preamp-volume", Preamp, 0x02),
    i100("preamp-bass", Preamp, 0x03),
    i100("preamp-middle", Preamp, 0x04),
    i100("preamp-treble", Preamp, 0x05),
    i100("preamp-presence", Preamp, 0x06),
    i100("preamp-master", Preamp, 0x07),
    b("preamp-bright", Preamp, 0x08),
    e("preamp-gain", Preamp, 0x09, PREAMP_GAIN_VALUES),
    // Loop (Table 6).
    b("loop-enable", Loop, 0x00),
    i100("loop-return-level", Loop, 0x01),
    i100("loop-send-level", Loop, 0x02),
    e("loop-mode", Loop, 0x03, LOOP_MODE_VALUES),
    // Equalizer (Table 7). Gains are raw 0..40 = -20..+20 dB, centre 20.
    b("eq-enable", Equalizer, 0x00),
    i("eq-low-gain", Equalizer, 0x01, 0, 40, 20),
    e("eq-mid-freq", Equalizer, 0x02, EQ_MID_FREQ_VALUES),
    i("eq-mid-gain", Equalizer, 0x03, 0, 40, 20),
    e("eq-mid-q", Equalizer, 0x04, EQ_MID_Q_VALUES),
    i("eq-high-gain", Equalizer, 0x05, 0, 40, 20),
    i("eq-level", Equalizer, 0x06, 0, 40, 20),
    // Speaker simulator (Table 8).
    b("speaker-enable", SpeakerSim, 0x00),
    e("speaker-type", SpeakerSim, 0x01, SPEAKER_TYPE_VALUES),
    i("speaker-mic-setting", SpeakerSim, 0x02, 1, 10, 1),
    i100("speaker-mic-level", SpeakerSim, 0x03),
    i100("speaker-direct-level", SpeakerSim, 0x04),
    // Noise suppressor (Table 9).
    b("ns-enable", NoiseSuppressor, 0x00),
    i100("ns-threshold", NoiseSuppressor, 0x01),
    i100("ns-release", NoiseSuppressor, 0x02),
    e("ns-detect", NoiseSuppressor, 0x03, NS_DETECT_VALUES),
    i100("ns-level", NoiseSuppressor, 0x04),
    // Modulation (Table 10). One block shared by seven effect types; many offsets
    // are reused across types (the doc names every type a field applies to).
    // Deferred: nibble-encoded resonance (0x0B/0x0C) and flanger separation
    // (0x0E/0x0F), and the 36-byte harmonist scale map (0x29..0x4C).
    b("mod-enable", Modulation, 0x00),
    e("mod-type", Modulation, 0x01, MOD_TYPE_VALUES),
    e("mod-phaser-stage", Modulation, 0x02, PHASER_STAGE_VALUES),
    e(
        "mod-vibrato-trigger",
        Modulation,
        0x03,
        VIBRATO_TRIGGER_VALUES,
    ),
    i100("mod-vibrato-rise-time", Modulation, 0x04),
    e(
        "mod-humanizer-type",
        Modulation,
        0x05,
        HUMANIZER_MODE_VALUES,
    ),
    e("mod-humanizer-vowel1", Modulation, 0x06, VOWEL_VALUES),
    e("mod-humanizer-vowel2", Modulation, 0x07, VOWEL_VALUES),
    i100("mod-rate", Modulation, 0x08), // Humanizer/Vibrato/Flanger/Phaser
    i100("mod-depth", Modulation, 0x09), // "
    i100("mod-manual", Modulation, 0x0A), // Flanger/Phaser
    i100("mod-phaser-step-rate", Modulation, 0x0D), // 0 = off, 1..100
    i100("mod-flanger-gate", Modulation, 0x10), // 0 = off, 1..100
    e(
        "mod-humanizer-trigger",
        Modulation,
        0x11,
        HUMANIZER_MODE_VALUES,
    ),
    i("mod-humanizer-pedal-source", Modulation, 0x12, 0, 64, 0),
    i100("mod-ring-frequency", Modulation, 0x13), // 0 = INT, 1..100
    i100("mod-ring-effect-level", Modulation, 0x14),
    i100("mod-ring-direct-level", Modulation, 0x15),
    e("mod-ps-type", Modulation, 0x16, PS_TYPE_VALUES),
    i("mod-ps-pitch1", Modulation, 0x17, 0, 50, 25), // raw 0..50 = -24..+24
    i("mod-ps-pitch2", Modulation, 0x18, 0, 50, 25),
    i("mod-ps-pitch3", Modulation, 0x19, 0, 50, 25),
    i("mod-ps-fine1", Modulation, 0x1A, 0, 100, 50), // raw 0..100 = -50..+50
    i("mod-ps-fine2", Modulation, 0x1B, 0, 100, 50),
    i("mod-ps-fine3", Modulation, 0x1C, 0, 100, 50),
    i("mod-harmonist-key", Modulation, 0x1D, 0, 24, 0),
    i("mod-harmonist-interval1", Modulation, 0x1E, 0, 48, 24), // raw 0..48 = -24..+24
    i("mod-harmonist-interval2", Modulation, 0x1F, 0, 48, 24),
    i("mod-harmonist-interval3", Modulation, 0x20, 0, 48, 24),
    i100("mod-pshr-pan1", Modulation, 0x21), // L100:R0 .. L0:R100
    i100("mod-pshr-pan2", Modulation, 0x22),
    i100("mod-pshr-pan3", Modulation, 0x23),
    i100("mod-pshr-level1", Modulation, 0x24),
    i100("mod-pshr-level2", Modulation, 0x25),
    i100("mod-pshr-level3", Modulation, 0x26),
    i100("mod-pshr-balance", Modulation, 0x27),
    i100("mod-pshr-total-level", Modulation, 0x28),
    // Delay (Table 11). Multi-byte tempo (0x03/0x04) and tap times (0x05..0x0A)
    // are deferred (14-bit values not yet modelled).
    b("delay-enable", Delay, 0x00),
    e("delay-mode", Delay, 0x01, DELAY_MODE_VALUES),
    e("delay-interval-c", Delay, 0x0B, DELAY_INTERVAL_VALUES),
    i100("delay-feedback", Delay, 0x0C),
    i100("delay-level-c", Delay, 0x0D),
    i100("delay-level-l", Delay, 0x0E),
    i100("delay-level-r", Delay, 0x0F),
    i("delay-high-damp", Delay, 0x10, 0, 50, 50), // raw 0..50 = -50..0
    e("delay-hi-cut", Delay, 0x11, HI_CUT_VALUES),
    b("delay-smooth", Delay, 0x12),
    i100("delay-effect-level", Delay, 0x13),
    i100("delay-direct-level", Delay, 0x14),
    // Chorus (Table 12).
    b("chorus-enable", Chorus, 0x00),
    e("chorus-mode", Chorus, 0x01, CHORUS_MODE_VALUES),
    i100("chorus-rate", Chorus, 0x02),
    i100("chorus-depth", Chorus, 0x03),
    i100("chorus-pre-delay", Chorus, 0x04), // raw 0..100 = ×0.5 ms (0.0..50.0 ms)
    e("chorus-low-cut", Chorus, 0x05, LOW_CUT_VALUES),
    e("chorus-hi-cut", Chorus, 0x06, HI_CUT_VALUES),
    i("chorus-mod-wave", Chorus, 0x07, 0, 10, 0),
    i100("chorus-effect-level", Chorus, 0x08),
    // Tremolo / Pan (Table 13).
    b("tremolo-enable", TremoloPan, 0x00),
    e("tremolo-mode", TremoloPan, 0x01, TREMOLO_MODE_VALUES),
    i100("tremolo-rate", TremoloPan, 0x02),
    i100("tremolo-depth", TremoloPan, 0x03),
    i100("tremolo-balance", TremoloPan, 0x04), // L100:R0 .. L0:R100
    // Reverb (Table 14).
    b("reverb-enable", Reverb, 0x00),
    e("reverb-mode", Reverb, 0x01, REVERB_MODE_VALUES),
    i("reverb-time", Reverb, 0x02, 1, 100, 50), // raw 1..100 = 0.1..10.0 s
    i100("reverb-pre-delay", Reverb, 0x03),     // raw 0..100 = 0..100 ms
    e("reverb-low-cut", Reverb, 0x04, LOW_CUT_VALUES),
    e("reverb-hi-cut", Reverb, 0x05, HI_CUT_VALUES),
    i("reverb-diffusion", Reverb, 0x06, 0, 10, 10),
    i100("reverb-effect-level", Reverb, 0x07),
    i100("reverb-direct-level", Reverb, 0x08),
];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keys_are_unique() {
        let mut seen = HashSet::new();
        for p in ALL {
            assert!(seen.insert(p.key()), "duplicate key: {}", p.key());
        }
    }

    #[test]
    fn addresses_are_unique() {
        let mut seen = HashSet::new();
        for p in ALL {
            assert!(
                seen.insert(p.address()),
                "duplicate address for {}",
                p.key()
            );
        }
    }

    #[test]
    fn address_is_temp_buffer_plus_offset() {
        let p = Param::from_key("preamp-gain").unwrap();
        assert_eq!(p.patch_offset(), [0x00, 0x00, 0x04, 0x09]);
        assert_eq!(p.address(), [0x08, 0x00, 0x04, 0x09]);
    }

    #[test]
    fn key_round_trips_through_from_key() {
        for p in ALL {
            assert_eq!(Param::from_key(p.key()).map(Param::key), Some(p.key()));
        }
        assert!(Param::from_key("nonsuch").is_none());
    }
}
