//! A fully-typed GX-700 patch model.
//!
//! [`Patch`] is a struct-per-block, named-field view of a patch: `patch.preamp.gain`,
//! `patch.delay.time_c`, etc. It is serde-(de)serialisable to readable JSON grouped
//! by block, has a [`Patch::clear`] that produces the silent INIT state, and
//! converts to/from the lossless [`RawPatch`] *byte-exactly*.
//!
//! The byte layout is **not** restated here: every field's offset and encoding come
//! from the parameter catalog ([`crate::param`]) via the field's key, so the catalog
//! stays the single source of truth and a field can never drift to the wrong byte.
//! The [`Patch::get`] / [`Patch::set`] bridge addresses any field by its catalog key
//! (the same string the CLI uses), e.g. `patch.set("preamp-gain", …)`.
//!
//! The struct fields and enum variants are intentionally undocumented: each is a
//! self-describing 1:1 mirror of a catalog key (`patch.preamp.volume` ⇒
//! `preamp-volume`), documented once in `param.rs` / `gx700-patch-data-format.adoc`.
#![allow(missing_docs)]

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::param::{Block, Kind, Param, Value};
use crate::patch::{PATCH_VERSION, RawPatch, decode_name, encode_name};

// ---------------------------------------------------------------------------
// Catalog-driven byte codec helpers. `dec`/`enc` look a parameter up by its key
// and use its offset + encoding from the catalog; the rest are typed wrappers.
// ---------------------------------------------------------------------------

fn dec(bytes: &[u8], key: &str) -> i32 {
    Param::from_key(key).map_or(0, |p| p.decode(bytes))
}
fn enc(buf: &mut [u8], key: &str, value: i32) {
    if let Some(p) = Param::from_key(key) {
        p.encode_into(value, buf);
    }
}
fn bool_at(bytes: &[u8], key: &str) -> bool {
    dec(bytes, key) != 0
}
fn u8_at(bytes: &[u8], key: &str) -> u8 {
    u8::try_from(dec(bytes, key)).unwrap_or(0)
}
fn u16_at(bytes: &[u8], key: &str) -> u16 {
    u16::try_from(dec(bytes, key)).unwrap_or(0)
}
/// Write raw bytes (chain order, name) that are not catalogued parameters.
fn put(buf: &mut [u8], at: usize, src: &[u8]) {
    if let Some(slot) = buf.get_mut(at..at + src.len()) {
        slot.copy_from_slice(src);
    }
}

/// Define a fieldless enum that (de)serialises as a human label and converts
/// to/from its raw device index (its declaration order).
macro_rules! raw_enum {
    ($name:ident { $first:ident = $flabel:literal $(, $variant:ident = $label:literal)* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
        pub enum $name {
            #[default]
            #[serde(rename = $flabel)] $first,
            $(#[serde(rename = $label)] $variant,)*
        }
        impl $name {
            const ORDER: &'static [$name] = &[$name::$first $(, $name::$variant)*];
            fn from_raw(raw: i32) -> Self {
                usize::try_from(raw)
                    .ok()
                    .and_then(|i| Self::ORDER.get(i).copied())
                    .unwrap_or($name::$first)
            }
            fn to_raw(self) -> i32 {
                Self::ORDER
                    .iter()
                    .position(|&v| v == self)
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(0)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Enums (type / mode selectors). Frequency and Q lookup tables stay raw `u8`.
// ---------------------------------------------------------------------------

raw_enum!(CompType { Compressor = "Compressor", Limiter = "Limiter" });
raw_enum!(WahMode { PedalWah = "Pedal Wah", SwPedalWah = "SW-Pedal Wah", AutoWah = "Auto Wah" });
raw_enum!(WahPolarity { Down = "Down", Up = "Up" });
raw_enum!(DistType {
    VintageOd = "Vintage OD", TurboOd = "Turbo OD", Blues = "Blues", Distortion = "Distortion",
    TurboDs = "Turbo DS", Metal = "Metal", Fuzz = "Fuzz"
});
raw_enum!(PreampModel {
    Jc120 = "JC-120", CleanTwin = "Clean Twin", MatchDrive = "Match Drive", BgLead = "BG Lead",
    Ms1959I = "MS1959 (I)", Ms1959Ii = "MS1959 (II)", Ms1959IPlusIi = "MS1959 (I+II)",
    SldnLead = "SLDN Lead", Metal5150 = "Metal 5150"
});
raw_enum!(PreampGain { Low = "Low", Mid = "Mid", Hi = "Hi" });
raw_enum!(LoopMode { Series = "Series", Parallel = "Parallel" });
raw_enum!(SpeakerType {
    Small = "Small", Middle = "Middle", Jc120 = "JC-120", BuiltIn1 = "Built-In 1",
    BuiltIn2 = "Built-In 2", BuiltIn3 = "Built-In 3", BuiltIn4 = "Built-In 4",
    BgStack1 = "BG Stack 1", BgStack2 = "BG Stack 2", MsStack1 = "MS Stack 1",
    MsStack2 = "MS Stack 2", MetalStack = "Metal Stack"
});
raw_enum!(NsDetect { GuitarIn = "Guitar In", NsIn = "NS In" });
raw_enum!(ModType {
    Flanger = "Flanger", Phaser = "Phaser", PitchShifter = "Pitch Shifter", Harmonist = "Harmonist",
    Vibrato = "Vibrato", RingModulator = "Ring Modulator", Humanizer = "Humanizer"
});
raw_enum!(PhaserStage {
    S4 = "4-Stage", S6 = "6-Stage", S8 = "8-Stage", S10 = "10-Stage", S12 = "12-Stage"
});
raw_enum!(VibratoTrigger { Off = "Off", On = "On", Auto = "Auto" });
raw_enum!(HumanizerMode { Auto = "Auto", Pedal = "Pedal" });
raw_enum!(Vowel { A = "a", E = "e", I = "i", O = "o", U = "u" });
raw_enum!(PsType { Slow = "Slow", Fast = "Fast", Mono = "Mono" });
raw_enum!(DelayMode { Normal = "Normal", Tempo = "Tempo" });
raw_enum!(DelayInterval {
    N16 = "1/16", T8 = "1/8 Triplet", D16 = "Dotted 1/16", N8 = "1/8", T4 = "1/4 Triplet",
    D8 = "Dotted 1/8", N4 = "1/4", D4 = "Dotted 1/4", N2 = "1/2", D2 = "Dotted 1/2", N1 = "Whole"
});
raw_enum!(ChorusMode { Mono = "Mono", Stereo = "Stereo" });
raw_enum!(TremoloMode {
    TremoloTri = "Tremolo (Tri)", TremoloSqr = "Tremolo (Sqr)", PanTri = "Pan (Tri)", PanSqr = "Pan (Sqr)"
});
raw_enum!(ReverbMode {
    Room1 = "Room 1", Room2 = "Room 2", Hall1 = "Hall 1", Hall2 = "Hall 2", Plate = "Plate"
});
raw_enum!(AssignMode { Normal = "Normal", Toggle = "Toggle" });

// ---------------------------------------------------------------------------
// Effect-block structs. Each decodes from / encodes to its sub-block bytes.
// ---------------------------------------------------------------------------

/// Compressor / limiter — sub-block `01`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Compressor {
    pub enable: bool,
    #[serde(rename = "type")]
    pub kind: CompType,
    pub sustain: u8,
    pub attack: u8,
    pub threshold: u8,
    pub release: u8,
    pub tone: u8,
    pub level: u8,
}
impl Compressor {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "comp-enable"),
            kind: CompType::from_raw(dec(b, "comp-type")),
            sustain: u8_at(b, "comp-sustain"),
            attack: u8_at(b, "comp-attack"),
            threshold: u8_at(b, "comp-threshold"),
            release: u8_at(b, "comp-release"),
            tone: u8_at(b, "comp-tone"),
            level: u8_at(b, "comp-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Compressor.byte_len()];
        enc(&mut b, "comp-enable", i32::from(self.enable));
        enc(&mut b, "comp-type", self.kind.to_raw());
        enc(&mut b, "comp-sustain", i32::from(self.sustain));
        enc(&mut b, "comp-attack", i32::from(self.attack));
        enc(&mut b, "comp-threshold", i32::from(self.threshold));
        enc(&mut b, "comp-release", i32::from(self.release));
        enc(&mut b, "comp-tone", i32::from(self.tone));
        enc(&mut b, "comp-level", i32::from(self.level));
        b
    }
}

/// Wah — sub-block `02`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Wah {
    pub enable: bool,
    pub mode: WahMode,
    pub pedal_freq: u8,
    pub auto_polarity: WahPolarity,
    pub auto_sens: u8,
    pub auto_manual: u8,
    pub peak: u8,
    pub pedal_source: u8,
    pub pedal_min: u8,
    pub pedal_max: u8,
    pub auto_rate: u8,
    pub auto_depth: u8,
    pub level: u8,
}
impl Wah {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "wah-enable"),
            mode: WahMode::from_raw(dec(b, "wah-mode")),
            pedal_freq: u8_at(b, "wah-pedal-freq"),
            auto_polarity: WahPolarity::from_raw(dec(b, "wah-auto-polarity")),
            auto_sens: u8_at(b, "wah-auto-sens"),
            auto_manual: u8_at(b, "wah-auto-manual"),
            peak: u8_at(b, "wah-peak"),
            pedal_source: u8_at(b, "wah-pedal-source"),
            pedal_min: u8_at(b, "wah-pedal-min"),
            pedal_max: u8_at(b, "wah-pedal-max"),
            auto_rate: u8_at(b, "wah-auto-rate"),
            auto_depth: u8_at(b, "wah-auto-depth"),
            level: u8_at(b, "wah-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Wah.byte_len()];
        enc(&mut b, "wah-enable", i32::from(self.enable));
        enc(&mut b, "wah-mode", self.mode.to_raw());
        enc(&mut b, "wah-pedal-freq", i32::from(self.pedal_freq));
        enc(&mut b, "wah-auto-polarity", self.auto_polarity.to_raw());
        enc(&mut b, "wah-auto-sens", i32::from(self.auto_sens));
        enc(&mut b, "wah-auto-manual", i32::from(self.auto_manual));
        enc(&mut b, "wah-peak", i32::from(self.peak));
        enc(&mut b, "wah-pedal-source", i32::from(self.pedal_source));
        enc(&mut b, "wah-pedal-min", i32::from(self.pedal_min));
        enc(&mut b, "wah-pedal-max", i32::from(self.pedal_max));
        enc(&mut b, "wah-auto-rate", i32::from(self.auto_rate));
        enc(&mut b, "wah-auto-depth", i32::from(self.auto_depth));
        enc(&mut b, "wah-level", i32::from(self.level));
        b
    }
}

/// Distortion / overdrive — sub-block `03`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Distortion {
    pub enable: bool,
    #[serde(rename = "type")]
    pub kind: DistType,
    pub drive: u8,
    pub bass: u8,
    pub treble: u8,
    pub level: u8,
}
impl Distortion {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "dist-enable"),
            kind: DistType::from_raw(dec(b, "dist-type")),
            drive: u8_at(b, "dist-drive"),
            bass: u8_at(b, "dist-bass"),
            treble: u8_at(b, "dist-treble"),
            level: u8_at(b, "dist-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Distortion.byte_len()];
        enc(&mut b, "dist-enable", i32::from(self.enable));
        enc(&mut b, "dist-type", self.kind.to_raw());
        enc(&mut b, "dist-drive", i32::from(self.drive));
        enc(&mut b, "dist-bass", i32::from(self.bass));
        enc(&mut b, "dist-treble", i32::from(self.treble));
        enc(&mut b, "dist-level", i32::from(self.level));
        b
    }
}

/// Preamp / amp model — sub-block `04`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Preamp {
    pub enable: bool,
    #[serde(rename = "type")]
    pub model: PreampModel,
    pub volume: u8,
    pub bass: u8,
    pub middle: u8,
    pub treble: u8,
    pub presence: u8,
    pub master: u8,
    pub bright: bool,
    pub gain: PreampGain,
}
impl Preamp {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "preamp-enable"),
            model: PreampModel::from_raw(dec(b, "preamp-type")),
            volume: u8_at(b, "preamp-volume"),
            bass: u8_at(b, "preamp-bass"),
            middle: u8_at(b, "preamp-middle"),
            treble: u8_at(b, "preamp-treble"),
            presence: u8_at(b, "preamp-presence"),
            master: u8_at(b, "preamp-master"),
            bright: bool_at(b, "preamp-bright"),
            gain: PreampGain::from_raw(dec(b, "preamp-gain")),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Preamp.byte_len()];
        enc(&mut b, "preamp-enable", i32::from(self.enable));
        enc(&mut b, "preamp-type", self.model.to_raw());
        enc(&mut b, "preamp-volume", i32::from(self.volume));
        enc(&mut b, "preamp-bass", i32::from(self.bass));
        enc(&mut b, "preamp-middle", i32::from(self.middle));
        enc(&mut b, "preamp-treble", i32::from(self.treble));
        enc(&mut b, "preamp-presence", i32::from(self.presence));
        enc(&mut b, "preamp-master", i32::from(self.master));
        enc(&mut b, "preamp-bright", i32::from(self.bright));
        enc(&mut b, "preamp-gain", self.gain.to_raw());
        b
    }
}

/// External effects loop — sub-block `05`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Loop {
    pub enable: bool,
    pub return_level: u8,
    pub send_level: u8,
    pub mode: LoopMode,
}
impl Loop {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "loop-enable"),
            return_level: u8_at(b, "loop-return-level"),
            send_level: u8_at(b, "loop-send-level"),
            mode: LoopMode::from_raw(dec(b, "loop-mode")),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Loop.byte_len()];
        enc(&mut b, "loop-enable", i32::from(self.enable));
        enc(&mut b, "loop-return-level", i32::from(self.return_level));
        enc(&mut b, "loop-send-level", i32::from(self.send_level));
        enc(&mut b, "loop-mode", self.mode.to_raw());
        b
    }
}

/// 3-band equalizer — sub-block `06`. Frequency / Q are raw table indices.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Equalizer {
    pub enable: bool,
    pub low_gain: u8,
    pub mid_freq: u8,
    pub mid_gain: u8,
    pub mid_q: u8,
    pub high_gain: u8,
    pub level: u8,
}
impl Equalizer {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "eq-enable"),
            low_gain: u8_at(b, "eq-low-gain"),
            mid_freq: u8_at(b, "eq-mid-freq"),
            mid_gain: u8_at(b, "eq-mid-gain"),
            mid_q: u8_at(b, "eq-mid-q"),
            high_gain: u8_at(b, "eq-high-gain"),
            level: u8_at(b, "eq-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Equalizer.byte_len()];
        enc(&mut b, "eq-enable", i32::from(self.enable));
        enc(&mut b, "eq-low-gain", i32::from(self.low_gain));
        enc(&mut b, "eq-mid-freq", i32::from(self.mid_freq));
        enc(&mut b, "eq-mid-gain", i32::from(self.mid_gain));
        enc(&mut b, "eq-mid-q", i32::from(self.mid_q));
        enc(&mut b, "eq-high-gain", i32::from(self.high_gain));
        enc(&mut b, "eq-level", i32::from(self.level));
        b
    }
}

/// Speaker simulator — sub-block `07`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SpeakerSim {
    pub enable: bool,
    #[serde(rename = "type")]
    pub kind: SpeakerType,
    pub mic_setting: u8,
    pub mic_level: u8,
    pub direct_level: u8,
}
impl SpeakerSim {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "speaker-enable"),
            kind: SpeakerType::from_raw(dec(b, "speaker-type")),
            mic_setting: u8_at(b, "speaker-mic-setting"),
            mic_level: u8_at(b, "speaker-mic-level"),
            direct_level: u8_at(b, "speaker-direct-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::SpeakerSim.byte_len()];
        enc(&mut b, "speaker-enable", i32::from(self.enable));
        enc(&mut b, "speaker-type", self.kind.to_raw());
        enc(&mut b, "speaker-mic-setting", i32::from(self.mic_setting));
        enc(&mut b, "speaker-mic-level", i32::from(self.mic_level));
        enc(&mut b, "speaker-direct-level", i32::from(self.direct_level));
        b
    }
}

/// Noise suppressor — sub-block `08`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NoiseSuppressor {
    pub enable: bool,
    pub threshold: u8,
    pub release: u8,
    pub detect: NsDetect,
    pub level: u8,
}
impl NoiseSuppressor {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "ns-enable"),
            threshold: u8_at(b, "ns-threshold"),
            release: u8_at(b, "ns-release"),
            detect: NsDetect::from_raw(dec(b, "ns-detect")),
            level: u8_at(b, "ns-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::NoiseSuppressor.byte_len()];
        enc(&mut b, "ns-enable", i32::from(self.enable));
        enc(&mut b, "ns-threshold", i32::from(self.threshold));
        enc(&mut b, "ns-release", i32::from(self.release));
        enc(&mut b, "ns-detect", self.detect.to_raw());
        enc(&mut b, "ns-level", i32::from(self.level));
        b
    }
}

/// Modulation — sub-block `09`. One block shared by seven effect types; the
/// pitch-shifter / harmonist voices and the 36-byte scale map are arrays.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modulation {
    pub enable: bool,
    #[serde(rename = "type")]
    pub kind: ModType,
    pub phaser_stage: PhaserStage,
    pub vibrato_trigger: VibratoTrigger,
    pub vibrato_rise_time: u8,
    pub humanizer_type: HumanizerMode,
    pub humanizer_vowel: [Vowel; 2],
    pub rate: u8,
    pub depth: u8,
    pub manual: u8,
    pub resonance: u8,
    pub phaser_step_rate: u8,
    pub flanger_separation: u8,
    pub flanger_gate: u8,
    pub humanizer_trigger: HumanizerMode,
    pub humanizer_pedal_source: u8,
    pub ring_frequency: u8,
    pub ring_effect_level: u8,
    pub ring_direct_level: u8,
    pub ps_type: PsType,
    pub ps_pitch: [u8; 3],
    pub ps_fine: [u8; 3],
    pub harmonist_key: u8,
    pub harmonist_interval: [u8; 3],
    pub pshr_pan: [u8; 3],
    pub pshr_level: [u8; 3],
    pub pshr_balance: u8,
    pub pshr_total_level: u8,
    /// Intelligent-harmony scale map: 3 voices × 12 chromatic notes
    /// (C, Db, D, …, B), each a raw interval `0..=48`. Always 36 elements.
    pub hr_scale: Vec<u8>,
}
/// Offset of the first harmonist-scale byte (`mod-hr-scale-c1`), from the catalog.
fn hr_scale_base() -> usize {
    Param::from_key("mod-hr-scale-c1").map_or(0x29, |p| usize::from(p.offset()))
}
impl Modulation {
    fn decode(b: &[u8]) -> Self {
        let base = hr_scale_base();
        let mut hr_scale = vec![0u8; 36];
        for (i, v) in hr_scale.iter_mut().enumerate() {
            *v = b.get(base + i).copied().unwrap_or(0);
        }
        Self {
            enable: bool_at(b, "mod-enable"),
            kind: ModType::from_raw(dec(b, "mod-type")),
            phaser_stage: PhaserStage::from_raw(dec(b, "mod-phaser-stage")),
            vibrato_trigger: VibratoTrigger::from_raw(dec(b, "mod-vibrato-trigger")),
            vibrato_rise_time: u8_at(b, "mod-vibrato-rise-time"),
            humanizer_type: HumanizerMode::from_raw(dec(b, "mod-humanizer-type")),
            humanizer_vowel: [
                Vowel::from_raw(dec(b, "mod-humanizer-vowel1")),
                Vowel::from_raw(dec(b, "mod-humanizer-vowel2")),
            ],
            rate: u8_at(b, "mod-rate"),
            depth: u8_at(b, "mod-depth"),
            manual: u8_at(b, "mod-manual"),
            resonance: u8_at(b, "mod-resonance"),
            phaser_step_rate: u8_at(b, "mod-phaser-step-rate"),
            flanger_separation: u8_at(b, "mod-flanger-separation"),
            flanger_gate: u8_at(b, "mod-flanger-gate"),
            humanizer_trigger: HumanizerMode::from_raw(dec(b, "mod-humanizer-trigger")),
            humanizer_pedal_source: u8_at(b, "mod-humanizer-pedal-source"),
            ring_frequency: u8_at(b, "mod-ring-frequency"),
            ring_effect_level: u8_at(b, "mod-ring-effect-level"),
            ring_direct_level: u8_at(b, "mod-ring-direct-level"),
            ps_type: PsType::from_raw(dec(b, "mod-ps-type")),
            ps_pitch: [
                u8_at(b, "mod-ps-pitch1"),
                u8_at(b, "mod-ps-pitch2"),
                u8_at(b, "mod-ps-pitch3"),
            ],
            ps_fine: [
                u8_at(b, "mod-ps-fine1"),
                u8_at(b, "mod-ps-fine2"),
                u8_at(b, "mod-ps-fine3"),
            ],
            harmonist_key: u8_at(b, "mod-harmonist-key"),
            harmonist_interval: [
                u8_at(b, "mod-harmonist-interval1"),
                u8_at(b, "mod-harmonist-interval2"),
                u8_at(b, "mod-harmonist-interval3"),
            ],
            pshr_pan: [
                u8_at(b, "mod-pshr-pan1"),
                u8_at(b, "mod-pshr-pan2"),
                u8_at(b, "mod-pshr-pan3"),
            ],
            pshr_level: [
                u8_at(b, "mod-pshr-level1"),
                u8_at(b, "mod-pshr-level2"),
                u8_at(b, "mod-pshr-level3"),
            ],
            pshr_balance: u8_at(b, "mod-pshr-balance"),
            pshr_total_level: u8_at(b, "mod-pshr-total-level"),
            hr_scale,
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Modulation.byte_len()];
        enc(&mut b, "mod-enable", i32::from(self.enable));
        enc(&mut b, "mod-type", self.kind.to_raw());
        enc(&mut b, "mod-phaser-stage", self.phaser_stage.to_raw());
        enc(&mut b, "mod-vibrato-trigger", self.vibrato_trigger.to_raw());
        enc(
            &mut b,
            "mod-vibrato-rise-time",
            i32::from(self.vibrato_rise_time),
        );
        enc(&mut b, "mod-humanizer-type", self.humanizer_type.to_raw());
        enc(
            &mut b,
            "mod-humanizer-vowel1",
            self.humanizer_vowel[0].to_raw(),
        );
        enc(
            &mut b,
            "mod-humanizer-vowel2",
            self.humanizer_vowel[1].to_raw(),
        );
        enc(&mut b, "mod-rate", i32::from(self.rate));
        enc(&mut b, "mod-depth", i32::from(self.depth));
        enc(&mut b, "mod-manual", i32::from(self.manual));
        enc(&mut b, "mod-resonance", i32::from(self.resonance));
        enc(
            &mut b,
            "mod-phaser-step-rate",
            i32::from(self.phaser_step_rate),
        );
        enc(
            &mut b,
            "mod-flanger-separation",
            i32::from(self.flanger_separation),
        );
        enc(&mut b, "mod-flanger-gate", i32::from(self.flanger_gate));
        enc(
            &mut b,
            "mod-humanizer-trigger",
            self.humanizer_trigger.to_raw(),
        );
        enc(
            &mut b,
            "mod-humanizer-pedal-source",
            i32::from(self.humanizer_pedal_source),
        );
        enc(&mut b, "mod-ring-frequency", i32::from(self.ring_frequency));
        enc(
            &mut b,
            "mod-ring-effect-level",
            i32::from(self.ring_effect_level),
        );
        enc(
            &mut b,
            "mod-ring-direct-level",
            i32::from(self.ring_direct_level),
        );
        enc(&mut b, "mod-ps-type", self.ps_type.to_raw());
        for (i, v) in self.ps_pitch.iter().enumerate() {
            enc(&mut b, &format!("mod-ps-pitch{}", i + 1), i32::from(*v));
        }
        for (i, v) in self.ps_fine.iter().enumerate() {
            enc(&mut b, &format!("mod-ps-fine{}", i + 1), i32::from(*v));
        }
        enc(&mut b, "mod-harmonist-key", i32::from(self.harmonist_key));
        for (i, v) in self.harmonist_interval.iter().enumerate() {
            enc(
                &mut b,
                &format!("mod-harmonist-interval{}", i + 1),
                i32::from(*v),
            );
        }
        for (i, v) in self.pshr_pan.iter().enumerate() {
            enc(&mut b, &format!("mod-pshr-pan{}", i + 1), i32::from(*v));
        }
        for (i, v) in self.pshr_level.iter().enumerate() {
            enc(&mut b, &format!("mod-pshr-level{}", i + 1), i32::from(*v));
        }
        enc(&mut b, "mod-pshr-balance", i32::from(self.pshr_balance));
        enc(
            &mut b,
            "mod-pshr-total-level",
            i32::from(self.pshr_total_level),
        );
        let base = hr_scale_base();
        for (i, v) in self.hr_scale.iter().enumerate() {
            if let Some(slot) = b.get_mut(base + i) {
                *slot = *v;
            }
        }
        b
    }
}

/// Delay — sub-block `0A`. Tempo is nibblized 8-bit; the tap times are 14-bit.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Delay {
    pub enable: bool,
    pub mode: DelayMode,
    pub tempo_in_control: u8,
    pub tempo: u8,
    pub time_c: u16,
    pub time_l: u16,
    pub time_r: u16,
    pub interval_c: DelayInterval,
    pub feedback: u8,
    pub level_c: u8,
    pub level_l: u8,
    pub level_r: u8,
    pub high_damp: u8,
    pub hi_cut: u8,
    pub smooth: bool,
    pub effect_level: u8,
    pub direct_level: u8,
}
impl Delay {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "delay-enable"),
            mode: DelayMode::from_raw(dec(b, "delay-mode")),
            tempo_in_control: u8_at(b, "delay-tempo-in-control"),
            tempo: u8_at(b, "delay-tempo"),
            time_c: u16_at(b, "delay-time-c"),
            time_l: u16_at(b, "delay-time-l"),
            time_r: u16_at(b, "delay-time-r"),
            interval_c: DelayInterval::from_raw(dec(b, "delay-interval-c")),
            feedback: u8_at(b, "delay-feedback"),
            level_c: u8_at(b, "delay-level-c"),
            level_l: u8_at(b, "delay-level-l"),
            level_r: u8_at(b, "delay-level-r"),
            high_damp: u8_at(b, "delay-high-damp"),
            hi_cut: u8_at(b, "delay-hi-cut"),
            smooth: bool_at(b, "delay-smooth"),
            effect_level: u8_at(b, "delay-effect-level"),
            direct_level: u8_at(b, "delay-direct-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Delay.byte_len()];
        enc(&mut b, "delay-enable", i32::from(self.enable));
        enc(&mut b, "delay-mode", self.mode.to_raw());
        enc(
            &mut b,
            "delay-tempo-in-control",
            i32::from(self.tempo_in_control),
        );
        enc(&mut b, "delay-tempo", i32::from(self.tempo));
        enc(&mut b, "delay-time-c", i32::from(self.time_c));
        enc(&mut b, "delay-time-l", i32::from(self.time_l));
        enc(&mut b, "delay-time-r", i32::from(self.time_r));
        enc(&mut b, "delay-interval-c", self.interval_c.to_raw());
        enc(&mut b, "delay-feedback", i32::from(self.feedback));
        enc(&mut b, "delay-level-c", i32::from(self.level_c));
        enc(&mut b, "delay-level-l", i32::from(self.level_l));
        enc(&mut b, "delay-level-r", i32::from(self.level_r));
        enc(&mut b, "delay-high-damp", i32::from(self.high_damp));
        enc(&mut b, "delay-hi-cut", i32::from(self.hi_cut));
        enc(&mut b, "delay-smooth", i32::from(self.smooth));
        enc(&mut b, "delay-effect-level", i32::from(self.effect_level));
        enc(&mut b, "delay-direct-level", i32::from(self.direct_level));
        b
    }
}

/// Chorus — sub-block `0B`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Chorus {
    pub enable: bool,
    pub mode: ChorusMode,
    pub rate: u8,
    pub depth: u8,
    pub pre_delay: u8,
    pub low_cut: u8,
    pub hi_cut: u8,
    pub mod_wave: u8,
    pub effect_level: u8,
}
impl Chorus {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "chorus-enable"),
            mode: ChorusMode::from_raw(dec(b, "chorus-mode")),
            rate: u8_at(b, "chorus-rate"),
            depth: u8_at(b, "chorus-depth"),
            pre_delay: u8_at(b, "chorus-pre-delay"),
            low_cut: u8_at(b, "chorus-low-cut"),
            hi_cut: u8_at(b, "chorus-hi-cut"),
            mod_wave: u8_at(b, "chorus-mod-wave"),
            effect_level: u8_at(b, "chorus-effect-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Chorus.byte_len()];
        enc(&mut b, "chorus-enable", i32::from(self.enable));
        enc(&mut b, "chorus-mode", self.mode.to_raw());
        enc(&mut b, "chorus-rate", i32::from(self.rate));
        enc(&mut b, "chorus-depth", i32::from(self.depth));
        enc(&mut b, "chorus-pre-delay", i32::from(self.pre_delay));
        enc(&mut b, "chorus-low-cut", i32::from(self.low_cut));
        enc(&mut b, "chorus-hi-cut", i32::from(self.hi_cut));
        enc(&mut b, "chorus-mod-wave", i32::from(self.mod_wave));
        enc(&mut b, "chorus-effect-level", i32::from(self.effect_level));
        b
    }
}

/// Tremolo / pan — sub-block `0C`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TremoloPan {
    pub enable: bool,
    pub mode: TremoloMode,
    pub rate: u8,
    pub depth: u8,
    pub balance: u8,
}
impl TremoloPan {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "tremolo-enable"),
            mode: TremoloMode::from_raw(dec(b, "tremolo-mode")),
            rate: u8_at(b, "tremolo-rate"),
            depth: u8_at(b, "tremolo-depth"),
            balance: u8_at(b, "tremolo-balance"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::TremoloPan.byte_len()];
        enc(&mut b, "tremolo-enable", i32::from(self.enable));
        enc(&mut b, "tremolo-mode", self.mode.to_raw());
        enc(&mut b, "tremolo-rate", i32::from(self.rate));
        enc(&mut b, "tremolo-depth", i32::from(self.depth));
        enc(&mut b, "tremolo-balance", i32::from(self.balance));
        b
    }
}

/// Reverb — sub-block `0D`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Reverb {
    pub enable: bool,
    pub mode: ReverbMode,
    pub time: u8,
    pub pre_delay: u8,
    pub low_cut: u8,
    pub hi_cut: u8,
    pub diffusion: u8,
    pub effect_level: u8,
    pub direct_level: u8,
}
impl Reverb {
    fn decode(b: &[u8]) -> Self {
        Self {
            enable: bool_at(b, "reverb-enable"),
            mode: ReverbMode::from_raw(dec(b, "reverb-mode")),
            time: u8_at(b, "reverb-time"),
            pre_delay: u8_at(b, "reverb-pre-delay"),
            low_cut: u8_at(b, "reverb-low-cut"),
            hi_cut: u8_at(b, "reverb-hi-cut"),
            diffusion: u8_at(b, "reverb-diffusion"),
            effect_level: u8_at(b, "reverb-effect-level"),
            direct_level: u8_at(b, "reverb-direct-level"),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; Block::Reverb.byte_len()];
        enc(&mut b, "reverb-enable", i32::from(self.enable));
        enc(&mut b, "reverb-mode", self.mode.to_raw());
        enc(&mut b, "reverb-time", i32::from(self.time));
        enc(&mut b, "reverb-pre-delay", i32::from(self.pre_delay));
        enc(&mut b, "reverb-low-cut", i32::from(self.low_cut));
        enc(&mut b, "reverb-hi-cut", i32::from(self.hi_cut));
        enc(&mut b, "reverb-diffusion", i32::from(self.diffusion));
        enc(&mut b, "reverb-effect-level", i32::from(self.effect_level));
        enc(&mut b, "reverb-direct-level", i32::from(self.direct_level));
        b
    }
}

/// One control assign (Level/Chain offsets `1A`+, ten bytes). `n` is `1..=4`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Assign {
    pub target: u8,
    pub min: u16,
    pub max: u16,
    pub source: u8,
    pub mode: AssignMode,
    pub act_lo: u8,
    pub act_hi: u8,
}
impl Assign {
    fn decode(b: &[u8], n: usize) -> Self {
        let k = |s: &str| format!("assign{n}-{s}");
        Self {
            target: u8_at(b, &k("target")),
            min: u16_at(b, &k("min")),
            max: u16_at(b, &k("max")),
            source: u8_at(b, &k("source")),
            mode: AssignMode::from_raw(dec(b, &k("mode"))),
            act_lo: u8_at(b, &k("act-lo")),
            act_hi: u8_at(b, &k("act-hi")),
        }
    }
    fn encode_into(&self, buf: &mut [u8], n: usize) {
        let k = |s: &str| format!("assign{n}-{s}");
        enc(buf, &k("target"), i32::from(self.target));
        enc(buf, &k("min"), i32::from(self.min));
        enc(buf, &k("max"), i32::from(self.max));
        enc(buf, &k("source"), i32::from(self.source));
        enc(buf, &k("mode"), self.mode.to_raw());
        enc(buf, &k("act-lo"), i32::from(self.act_lo));
        enc(buf, &k("act-hi"), i32::from(self.act_hi));
    }
}

// ---------------------------------------------------------------------------
// The whole patch.
// ---------------------------------------------------------------------------

/// A fully-typed GX-700 patch, with byte-exact conversion to/from [`RawPatch`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Patch {
    /// Patch name (up to 12 characters).
    pub name: String,
    /// Master output level (`0..=100`).
    pub output_level: u8,
    /// Signal-chain order: 13 effect-block ids (`1`=Compressor .. `13`=Reverb).
    pub chain: [u8; 13],
    /// The four control assigns.
    pub assigns: [Assign; 4],
    pub compressor: Compressor,
    pub wah: Wah,
    pub distortion: Distortion,
    pub preamp: Preamp,
    pub fx_loop: Loop,
    pub eq: Equalizer,
    pub speaker: SpeakerSim,
    pub noise_suppressor: NoiseSuppressor,
    pub modulation: Modulation,
    pub delay: Delay,
    pub chorus: Chorus,
    pub tremolo: TremoloPan,
    pub reverb: Reverb,
}

impl Patch {
    /// Decode a [`RawPatch`] into the typed model.
    #[must_use]
    pub fn from_raw(raw: &RawPatch) -> Self {
        let block = |b: Block| -> Vec<u8> {
            raw.blocks
                .get(&b.base())
                .map(|h| from_hex(h))
                .unwrap_or_default()
        };
        let lc = block(Block::LevelChain);
        let mut chain = [0u8; 13];
        for (i, c) in chain.iter_mut().enumerate() {
            *c = lc.get(1 + i).copied().unwrap_or(0);
        }
        Self {
            name: raw.name.clone(),
            output_level: u8_at(&lc, "output-level"),
            chain,
            assigns: [
                Assign::decode(&lc, 1),
                Assign::decode(&lc, 2),
                Assign::decode(&lc, 3),
                Assign::decode(&lc, 4),
            ],
            compressor: Compressor::decode(&block(Block::Compressor)),
            wah: Wah::decode(&block(Block::Wah)),
            distortion: Distortion::decode(&block(Block::Distortion)),
            preamp: Preamp::decode(&block(Block::Preamp)),
            fx_loop: Loop::decode(&block(Block::Loop)),
            eq: Equalizer::decode(&block(Block::Equalizer)),
            speaker: SpeakerSim::decode(&block(Block::SpeakerSim)),
            noise_suppressor: NoiseSuppressor::decode(&block(Block::NoiseSuppressor)),
            modulation: Modulation::decode(&block(Block::Modulation)),
            delay: Delay::decode(&block(Block::Delay)),
            chorus: Chorus::decode(&block(Block::Chorus)),
            tremolo: TremoloPan::decode(&block(Block::TremoloPan)),
            reverb: Reverb::decode(&block(Block::Reverb)),
        }
    }

    /// Re-encode to a [`RawPatch`], byte-for-byte.
    #[must_use]
    pub fn to_raw(&self) -> RawPatch {
        let mut blocks = std::collections::BTreeMap::new();
        for base in 0u8..=13 {
            if let Some(b) = Block::from_base(base) {
                blocks.insert(base, to_hex(&self.block_bytes(b)));
            }
        }
        RawPatch {
            version: PATCH_VERSION,
            name: self.name.clone(),
            blocks,
        }
    }

    /// Reset to the silent INIT state: name "Empty", level 0, default chain, and
    /// every effect block bypassed.
    pub fn clear(&mut self) {
        "Empty".clone_into(&mut self.name);
        self.output_level = 0;
        for (i, c) in self.chain.iter_mut().enumerate() {
            *c = u8::try_from(i + 1).unwrap_or(1);
        }
        self.compressor.enable = false;
        self.wah.enable = false;
        self.distortion.enable = false;
        self.preamp.enable = false;
        self.fx_loop.enable = false;
        self.eq.enable = false;
        self.speaker.enable = false;
        self.noise_suppressor.enable = false;
        self.modulation.enable = false;
        self.delay.enable = false;
        self.chorus.enable = false;
        self.tremolo.enable = false;
        self.reverb.enable = false;
    }

    /// A fresh INIT patch for an empty slot: a *valid* default chain (each effect
    /// block once, in order) with every block bypassed and a blank name. Use this
    /// rather than `Default`, whose chain is all-zero — an invalid routing that maps
    /// every chain entry to [`Block::LevelChain`].
    #[must_use]
    pub fn init() -> Self {
        let mut p = Self::default();
        p.clear();
        p.name.clear(); // blank (clear() names it "Empty"); an empty slot reads as INIT
        p
    }

    /// Overwrite `self`'s effect `block` with the same block from `other`,
    /// transplanting one whole effect block (every parameter it holds) between
    /// patches. [`Block::LevelChain`] (name / level / chain / assigns) is left
    /// untouched, so this never disturbs the patch's identity or routing.
    pub fn copy_block_from(&mut self, other: &Self, block: Block) {
        match block {
            Block::Compressor => self.compressor = other.compressor.clone(),
            Block::Wah => self.wah = other.wah.clone(),
            Block::Distortion => self.distortion = other.distortion.clone(),
            Block::Preamp => self.preamp = other.preamp.clone(),
            Block::Loop => self.fx_loop = other.fx_loop.clone(),
            Block::Equalizer => self.eq = other.eq.clone(),
            Block::SpeakerSim => self.speaker = other.speaker.clone(),
            Block::NoiseSuppressor => self.noise_suppressor = other.noise_suppressor.clone(),
            Block::Modulation => self.modulation = other.modulation.clone(),
            Block::Delay => self.delay = other.delay.clone(),
            Block::Chorus => self.chorus = other.chorus.clone(),
            Block::TremoloPan => self.tremolo = other.tremolo.clone(),
            Block::Reverb => self.reverb = other.reverb.clone(),
            Block::LevelChain => {}
        }
    }

    /// Read any parameter by its catalog key (the same string the CLI uses), e.g.
    /// `patch.get("preamp-gain")`. `None` for an unknown key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Value> {
        let p = Param::from_key(key)?;
        let raw = p.decode(&self.block_bytes(p.block()));
        Some(match p.kind() {
            Kind::Bool => Value::Bool(raw != 0),
            Kind::Enum { .. } => Value::Enum(raw),
            _ => Value::Int(raw),
        })
    }

    /// Set any parameter by its catalog key.
    ///
    /// # Errors
    /// [`Error::Patch`] for an unknown key or a value whose kind/range does not
    /// match the parameter.
    pub fn set(&mut self, key: &str, value: Value) -> Result<()> {
        let p =
            Param::from_key(key).ok_or_else(|| Error::Patch(format!("unknown parameter {key}")))?;
        let raw = validate(p, value)?;
        let mut bytes = self.block_bytes(p.block());
        p.encode_into(raw, &mut bytes);
        self.set_block_bytes(p.block(), &bytes);
        Ok(())
    }

    /// The exact device bytes of one sub-block (the bridge between the typed
    /// sub-structs and the catalog).
    fn block_bytes(&self, block: Block) -> Vec<u8> {
        match block {
            Block::LevelChain => {
                let mut b = vec![0u8; Block::LevelChain.byte_len()];
                enc(&mut b, "output-level", i32::from(self.output_level));
                put(&mut b, 1, &self.chain);
                put(&mut b, 14, &encode_name(&self.name));
                for (i, a) in self.assigns.iter().enumerate() {
                    a.encode_into(&mut b, i + 1);
                }
                b
            }
            Block::Compressor => self.compressor.encode(),
            Block::Wah => self.wah.encode(),
            Block::Distortion => self.distortion.encode(),
            Block::Preamp => self.preamp.encode(),
            Block::Loop => self.fx_loop.encode(),
            Block::Equalizer => self.eq.encode(),
            Block::SpeakerSim => self.speaker.encode(),
            Block::NoiseSuppressor => self.noise_suppressor.encode(),
            Block::Modulation => self.modulation.encode(),
            Block::Delay => self.delay.encode(),
            Block::Chorus => self.chorus.encode(),
            Block::TremoloPan => self.tremolo.encode(),
            Block::Reverb => self.reverb.encode(),
        }
    }

    /// Decode a sub-block's bytes back into the matching typed sub-struct.
    fn set_block_bytes(&mut self, block: Block, bytes: &[u8]) {
        match block {
            Block::LevelChain => {
                self.output_level = u8_at(bytes, "output-level");
                let name = decode_name(bytes.get(14..26).unwrap_or(&[]));
                self.name = name;
                for (i, c) in self.chain.iter_mut().enumerate() {
                    *c = bytes.get(1 + i).copied().unwrap_or(0);
                }
                for (i, a) in self.assigns.iter_mut().enumerate() {
                    *a = Assign::decode(bytes, i + 1);
                }
            }
            Block::Compressor => self.compressor = Compressor::decode(bytes),
            Block::Wah => self.wah = Wah::decode(bytes),
            Block::Distortion => self.distortion = Distortion::decode(bytes),
            Block::Preamp => self.preamp = Preamp::decode(bytes),
            Block::Loop => self.fx_loop = Loop::decode(bytes),
            Block::Equalizer => self.eq = Equalizer::decode(bytes),
            Block::SpeakerSim => self.speaker = SpeakerSim::decode(bytes),
            Block::NoiseSuppressor => self.noise_suppressor = NoiseSuppressor::decode(bytes),
            Block::Modulation => self.modulation = Modulation::decode(bytes),
            Block::Delay => self.delay = Delay::decode(bytes),
            Block::Chorus => self.chorus = Chorus::decode(bytes),
            Block::TremoloPan => self.tremolo = TremoloPan::decode(bytes),
            Block::Reverb => self.reverb = Reverb::decode(bytes),
        }
    }
}

/// One effect block's settings, self-describing (the variant names the block).
/// A portable snapshot for an on-disk per-block library: serialize one of these
/// to a file, then apply it onto any patch's matching block. `LevelChain` is not
/// representable — it is the patch's identity (name / level / chain), not an
/// effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockData {
    Compressor(Compressor),
    Wah(Wah),
    Distortion(Distortion),
    Preamp(Preamp),
    Loop(Loop),
    Equalizer(Equalizer),
    SpeakerSim(SpeakerSim),
    NoiseSuppressor(NoiseSuppressor),
    Modulation(Modulation),
    Delay(Delay),
    Chorus(Chorus),
    TremoloPan(TremoloPan),
    Reverb(Reverb),
}

impl BlockData {
    /// Snapshot `block` out of `patch`. `None` for [`Block::LevelChain`].
    #[must_use]
    pub fn from_patch(patch: &Patch, block: Block) -> Option<Self> {
        Some(match block {
            Block::Compressor => Self::Compressor(patch.compressor.clone()),
            Block::Wah => Self::Wah(patch.wah.clone()),
            Block::Distortion => Self::Distortion(patch.distortion.clone()),
            Block::Preamp => Self::Preamp(patch.preamp.clone()),
            Block::Loop => Self::Loop(patch.fx_loop.clone()),
            Block::Equalizer => Self::Equalizer(patch.eq.clone()),
            Block::SpeakerSim => Self::SpeakerSim(patch.speaker.clone()),
            Block::NoiseSuppressor => Self::NoiseSuppressor(patch.noise_suppressor.clone()),
            Block::Modulation => Self::Modulation(patch.modulation.clone()),
            Block::Delay => Self::Delay(patch.delay.clone()),
            Block::Chorus => Self::Chorus(patch.chorus.clone()),
            Block::TremoloPan => Self::TremoloPan(patch.tremolo.clone()),
            Block::Reverb => Self::Reverb(patch.reverb.clone()),
            Block::LevelChain => return None,
        })
    }

    /// Which effect block this data belongs to.
    #[must_use]
    pub fn block(&self) -> Block {
        match self {
            Self::Compressor(_) => Block::Compressor,
            Self::Wah(_) => Block::Wah,
            Self::Distortion(_) => Block::Distortion,
            Self::Preamp(_) => Block::Preamp,
            Self::Loop(_) => Block::Loop,
            Self::Equalizer(_) => Block::Equalizer,
            Self::SpeakerSim(_) => Block::SpeakerSim,
            Self::NoiseSuppressor(_) => Block::NoiseSuppressor,
            Self::Modulation(_) => Block::Modulation,
            Self::Delay(_) => Block::Delay,
            Self::Chorus(_) => Block::Chorus,
            Self::TremoloPan(_) => Block::TremoloPan,
            Self::Reverb(_) => Block::Reverb,
        }
    }

    /// Apply this block onto `patch`'s matching block (the rest is untouched).
    pub fn apply_to(&self, patch: &mut Patch) {
        match self {
            Self::Compressor(b) => patch.compressor = b.clone(),
            Self::Wah(b) => patch.wah = b.clone(),
            Self::Distortion(b) => patch.distortion = b.clone(),
            Self::Preamp(b) => patch.preamp = b.clone(),
            Self::Loop(b) => patch.fx_loop = b.clone(),
            Self::Equalizer(b) => patch.eq = b.clone(),
            Self::SpeakerSim(b) => patch.speaker = b.clone(),
            Self::NoiseSuppressor(b) => patch.noise_suppressor = b.clone(),
            Self::Modulation(b) => patch.modulation = b.clone(),
            Self::Delay(b) => patch.delay = b.clone(),
            Self::Chorus(b) => patch.chorus = b.clone(),
            Self::TremoloPan(b) => patch.tremolo = b.clone(),
            Self::Reverb(b) => patch.reverb = b.clone(),
        }
    }
}

/// Validate `value` against `param` and return the raw device value.
fn validate(param: Param, value: Value) -> Result<i32> {
    let key = param.key();
    let raw = match (param.kind(), value) {
        (Kind::Bool, Value::Bool(b)) => i32::from(b),
        (Kind::Int { min, max, .. }, Value::Int(v)) => {
            if v < min || v > max {
                return Err(Error::Patch(format!(
                    "{key}: {v} out of range {min}..={max}"
                )));
            }
            v
        }
        (Kind::Enum { values, .. }, Value::Enum(v)) => {
            let max = i32::try_from(values.len().saturating_sub(1)).unwrap_or(0);
            if v < 0 || v > max {
                return Err(Error::Patch(format!("{key}: enum index {v} out of range")));
            }
            v
        }
        _ => return Err(Error::Patch(format!("{key}: value kind mismatch"))),
    };
    Ok(raw)
}

fn from_hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .filter_map(|t| u8::from_str_radix(t, 16).ok())
        .collect()
}
fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;
    use crate::param;
    use std::path::PathBuf;

    fn bank() -> Vec<(String, RawPatch)> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bank");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("read fixtures/bank") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let raw: RawPatch =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            out.push((name, raw));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    #[test]
    fn round_trips_every_bank_patch_byte_for_byte() {
        let patches = bank();
        assert!(patches.len() >= 100, "found {}", patches.len());
        for (name, raw) in &patches {
            let back = Patch::from_raw(raw).to_raw();
            assert_eq!(
                back.blocks, raw.blocks,
                "{name}: typed round-trip is not byte-exact"
            );
            assert_eq!(back.name, raw.name, "{name}: name");
        }
    }

    #[test]
    fn get_set_bridge_covers_every_catalog_key() {
        let mut p = Patch::from_raw(&bank()[0].1);
        for param in param::ALL {
            // A representative in-range value for this parameter's kind.
            let v = match param.kind() {
                Kind::Bool => Value::Bool(true),
                Kind::Int { min, .. } => Value::Int(min),
                Kind::Enum { .. } => Value::Enum(0),
            };
            p.set(param.key(), v).expect("set by key");
            assert_eq!(p.get(param.key()), Some(v), "{}", param.key());
        }
    }

    #[test]
    fn clear_is_silent_init() {
        let mut p = Patch::from_raw(&bank()[0].1);
        p.clear();
        assert_eq!(p.name, "Empty");
        assert_eq!(p.output_level, 0);
        assert_eq!(p.chain, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
        assert!(!p.preamp.enable && !p.delay.enable && !p.reverb.enable);
        // Still a valid, byte-exact round-tripping patch.
        assert_eq!(Patch::from_raw(&p.to_raw()), p);
    }

    #[test]
    fn init_has_a_valid_chain_unlike_default() {
        let p = Patch::init();
        assert_eq!(p.chain, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
        assert!(p.name.is_empty()); // blank, so an empty slot reads as INIT
        assert!(!p.preamp.enable && !p.reverb.enable); // bypassed
        // The derive default's chain is all-zero (every entry maps to LevelChain),
        // which is the bug init() exists to avoid.
        assert_eq!(Patch::default().chain, [0; 13]);
    }

    #[test]
    fn copy_block_from_transplants_one_block() {
        let a = Patch::from_raw(&bank()[0].1);
        let b = Patch::from_raw(&bank()[1].1);
        let mut merged = a.clone();
        merged.copy_block_from(&b, Block::Delay);
        // The Delay block now matches b; everything else still matches a.
        assert_eq!(merged.delay, b.delay);
        assert_eq!(merged.reverb, a.reverb);
        assert_eq!(merged.name, a.name);
        assert_eq!(merged.chain, a.chain);
        // LevelChain is never touched, even if asked.
        let mut same = a.clone();
        same.copy_block_from(&b, Block::LevelChain);
        assert_eq!(same, a);
    }

    #[test]
    fn block_data_round_trips_and_applies() {
        let a = Patch::from_raw(&bank()[0].1);
        let b = Patch::from_raw(&bank()[1].1);
        let bd = BlockData::from_patch(&b, Block::Reverb).expect("reverb is an effect block");
        assert_eq!(bd.block(), Block::Reverb);
        // Self-describing serde round-trip (the JSON is tagged "reverb").
        let json = serde_json::to_string(&bd).unwrap();
        assert!(json.contains("reverb"), "{json}");
        let back: BlockData = serde_json::from_str(&json).unwrap();
        assert_eq!(back, bd);
        // Applying transplants only that block.
        let mut merged = a.clone();
        back.apply_to(&mut merged);
        assert_eq!(merged.reverb, b.reverb);
        assert_eq!(merged.delay, a.delay);
        // The Level/Chain "block" has no portable representation.
        assert!(BlockData::from_patch(&a, Block::LevelChain).is_none());
    }

    #[test]
    fn serializes_grouped_by_block() {
        let p = Patch::from_raw(&bank()[0].1);
        let json = serde_json::to_value(&p).unwrap();
        assert!(json.get("preamp").and_then(|b| b.get("gain")).is_some());
        assert!(json.get("delay").and_then(|b| b.get("time_c")).is_some());
    }
}
