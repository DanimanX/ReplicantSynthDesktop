//! The clip slot: kind, pattern, MIDI notes, engine parameters and mixer strip.

use serde::{Deserialize, Serialize};

pub const PADS: usize = 64;
pub const COLS: usize = 8;
pub const ROWS: usize = 8;
pub const STEPS: usize = 16;
pub const DRUM_VOICES: usize = 11;
pub const PARAMS: usize = 32;
pub const MAX_MIDI_NOTES: usize = 96;

pub const WAVETABLE_BANKS: [&str; 8] = [
    "ANALOG MORPH",
    "CYBER NEURO",
    "DIGITAL FM",
    "VOCALOID",
    "CYBER ACID",
    "DYSTOPIAN STEEL",
    "PWM HYPER",
    "SUB ZERO",
];

pub const FILTER_MODES: [&str; 4] = ["LP 24", "LP 12", "BP 12", "HP 12"];
pub const CHORD_MODES: [&str; 5] = ["SINGLE", "FIFTH", "MINOR", "MAJOR", "OCTAVE"];
pub const DISTORTION_TYPES: [&str; 5] = ["POCO RAT", "TUBE", "HARD CLIP", "FUZZ", "WAVEFOLD"];
pub const DRUM_VOICE_NAMES: [&str; 11] = [
    "BD", "SD", "LT", "MT", "HT", "RS", "CP", "CH", "OH", "CC", "RC",
];

pub const QUANTIZE_OPTIONS: [i32; 6] = [0, 1, 4, 16, 32, 64];

/// Native kind ordinals must match `rep::ClipKind` in the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClipKind {
    #[default]
    Empty,
    Acid,
    Wave,
    Drums,
    Sample,
    Plugin,
}

impl ClipKind {
    pub const ALL: [ClipKind; 6] = [
        ClipKind::Empty,
        ClipKind::Acid,
        ClipKind::Wave,
        ClipKind::Drums,
        ClipKind::Sample,
        ClipKind::Plugin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ClipKind::Empty => "EMPTY",
            ClipKind::Acid => "ACID 303",
            ClipKind::Wave => "WAVE",
            ClipKind::Drums => "DRUMS 909",
            ClipKind::Sample => "SAMPLE",
            ClipKind::Plugin => "PLUGIN",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            ClipKind::Empty => "--",
            ClipKind::Acid => "303",
            ClipKind::Wave => "WAVE",
            ClipKind::Drums => "909",
            ClipKind::Sample => "SMP",
            ClipKind::Plugin => "VST",
        }
    }

    pub fn ordinal(self) -> i32 {
        match self {
            ClipKind::Empty => 0,
            ClipKind::Acid => 1,
            ClipKind::Wave => 2,
            ClipKind::Drums => 3,
            ClipKind::Sample => 4,
            ClipKind::Plugin => 5,
        }
    }

    pub fn from_ordinal(value: i32) -> Self {
        match value {
            1 => ClipKind::Acid,
            2 => ClipKind::Wave,
            3 => ClipKind::Drums,
            4 => ClipKind::Sample,
            5 => ClipKind::Plugin,
            _ => ClipKind::Empty,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ClipState {
    #[default]
    Stopped,
    QueuedStart,
    Playing,
    QueuedStop,
}

impl ClipState {
    pub fn from_ordinal(value: i32) -> Self {
        match value {
            1 => ClipState::QueuedStart,
            2 => ClipState::Playing,
            3 => ClipState::QueuedStop,
            _ => ClipState::Stopped,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ClipState::Stopped => "STOPPED",
            ClipState::QueuedStart => "QUEUED",
            ClipState::Playing => "PLAYING",
            ClipState::QueuedStop => "STOPPING",
        }
    }
}

/// Parameter slots inside the 32-float clip block.
#[allow(non_snake_case)]
pub mod AcidParam {
    pub const WAVE: usize = 0;
    pub const CUTOFF: usize = 1;
    pub const RESONANCE: usize = 2;
    pub const ENV_MOD: usize = 3;
    pub const DECAY: usize = 4;
    pub const ACCENT: usize = 5;
    pub const DIST_TYPE: usize = 6;
    pub const DIST_DRIVE: usize = 7;
    pub const DIST_MIX: usize = 8;
    pub const DIST_TONE: usize = 9;
    pub const VOLUME: usize = 10;
}

#[allow(non_snake_case)]
pub mod WaveParam {
    pub const BANK: usize = 0;
    pub const POSITION: usize = 1;
    pub const UNISON: usize = 2;
    pub const DETUNE: usize = 3;
    pub const SPREAD: usize = 4;
    pub const CUTOFF: usize = 5;
    pub const RESONANCE: usize = 6;
    pub const DRIVE: usize = 7;
    pub const ATTACK: usize = 8;
    pub const DECAY: usize = 9;
    pub const SUSTAIN: usize = 10;
    pub const RELEASE: usize = 11;
    pub const SUB: usize = 12;
    pub const NOISE: usize = 13;
    pub const LEVEL: usize = 14;
    pub const FILTER_MODE: usize = 15;
    pub const CHORD: usize = 16;
}

#[allow(non_snake_case)]
pub mod DrumParam {
    pub const TUNE: usize = 0;
    pub const DECAY: usize = 1;
    pub const SNAPPY: usize = 2;
    pub const TONE: usize = 3;
    pub const ACCENT: usize = 4;
    pub const LEVEL: usize = 5;
    pub const CUTOFF: usize = 6;
    pub const RESONANCE: usize = 7;
}

#[allow(non_snake_case)]
pub mod SampleParam {
    pub const GAIN: usize = 0;
    pub const PITCH: usize = 1;
}

#[allow(non_snake_case)]
pub mod FxParam {
    pub const BASE: usize = 24;
    pub const ENABLED: usize = 30;
}

pub const FX_PARAM_BASE: usize = FxParam::BASE;
pub const FX_ENABLED_INDEX: usize = FxParam::ENABLED;

/// The two plugin formats the host can load.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PluginFormat {
    #[default]
    Clap,
    Vst3,
}

impl PluginFormat {
    pub fn label(self) -> &'static str {
        match self {
            PluginFormat::Clap => "CLAP",
            PluginFormat::Vst3 => "VST3",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label.to_ascii_uppercase().as_str() {
            "CLAP" => Some(PluginFormat::Clap),
            "VST3" | "VST" => Some(PluginFormat::Vst3),
            _ => None,
        }
    }
}

/// A hosted plugin attached to one pad.
///
/// `state` is the plugin's own opaque state blob, base64-encoded so the project
/// file stays valid JSON and diffable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginRef {
    pub format: PluginFormat,
    /// Absolute path of the `.clap` file or the `.vst3` bundle.
    pub path: String,
    /// CLAP plugin id, or the VST3 class id as hex.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub state: Option<String>,
}

impl PluginRef {
    /// Look up the engine ordinal for a plugin clip.
    pub fn is_instrument_hint(&self) -> bool {
        !self.id.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub gate: bool,
    pub note: u8,
    pub velocity: u8,
    pub accent: bool,
    pub slide: bool,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            gate: false,
            note: 60,
            velocity: 100,
            accent: false,
            slide: false,
        }
    }
}

/// A free MIDI note for the ReplicantWave piano roll. `start` and `length` are
/// 16th-note steps inside the clip, so notes can sustain across steps and
/// several notes can sound at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiNote {
    pub note: u8,
    pub start: u8,
    pub length: u8,
    pub velocity: u8,
}

impl MidiNote {
    pub fn new(note: i32, start: i32, length: i32, velocity: i32) -> Self {
        Self {
            note: note.clamp(0, 127) as u8,
            start: start.clamp(0, (STEPS - 1) as i32) as u8,
            length: length.clamp(1, STEPS as i32) as u8,
            velocity: velocity.clamp(1, 127) as u8,
        }
    }

    pub fn end(&self) -> i32 {
        self.start as i32 + self.length as i32
    }
}

/// One clip slot. Plain data: the UI mutates it, then re-syncs the engine.
#[derive(Clone, Debug)]
pub struct Clip {
    pub index: usize,
    pub kind: ClipKind,
    pub name: String,
    pub root: i32,
    pub length: i32,
    pub gain: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub steps: [Step; STEPS],
    pub midi_notes: Vec<MidiNote>,
    pub drum_mask: [u16; DRUM_VOICES],
    pub drum_vel: [[u8; STEPS]; DRUM_VOICES],
    pub sample_loop: bool,
    pub sample_name: Option<String>,
    pub sample_path: Option<String>,
    pub sample_missing: bool,
    pub fx_type: usize,
    pub fx_enabled: bool,
    pub params: [f32; PARAMS],
    /// Set for `ClipKind::Plugin`: which plugin this pad hosts.
    pub plugin: Option<PluginRef>,
}

impl Clip {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            kind: ClipKind::Empty,
            name: String::new(),
            root: 60,
            length: STEPS as i32,
            gain: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            steps: [Step::default(); STEPS],
            midi_notes: Vec::new(),
            drum_mask: [0; DRUM_VOICES],
            drum_vel: [[100; STEPS]; DRUM_VOICES],
            sample_loop: true,
            sample_name: None,
            sample_path: None,
            sample_missing: false,
            fx_type: 0,
            fx_enabled: false,
            params: [0.0; PARAMS],
            plugin: None,
        }
    }

    pub fn row(&self) -> usize {
        self.index / COLS
    }

    pub fn col(&self) -> usize {
        self.index % COLS
    }

    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            // Callers that need an owned default use `default_name`.
            ""
        } else {
            &self.name
        }
    }

    pub fn param(&self, slot: usize) -> f32 {
        self.params.get(slot).copied().unwrap_or(0.0)
    }

    pub fn set_param(&mut self, slot: usize, value: f32) {
        if slot < PARAMS {
            self.params[slot] = value;
        }
    }

    pub fn set_step(&mut self, index: usize, step: Step) {
        if index < STEPS {
            self.steps[index] = step;
        }
    }

    pub fn midi_note_at(&self, note: i32, start: i32) -> Option<&MidiNote> {
        self.midi_notes
            .iter()
            .find(|n| n.note as i32 == note && n.start as i32 == start)
    }

    pub fn add_midi_note(&mut self, note: i32, start: i32, length: i32, velocity: i32) -> MidiNote {
        let loop_len = self.length.clamp(1, STEPS as i32);
        let safe_start = start.clamp(0, loop_len - 1);
        let safe_length = length.clamp(1, loop_len - safe_start);
        let entry = MidiNote::new(note, safe_start, safe_length, velocity);
        self.midi_notes
            .retain(|n| !(n.note == entry.note && n.start == entry.start));
        self.midi_notes.push(entry);
        self.midi_notes.sort_by_key(|n| (n.start, n.note));
        entry
    }

    pub fn remove_midi_note(&mut self, note: i32, start: i32) {
        self.midi_notes
            .retain(|n| !(n.note as i32 == note && n.start as i32 == start));
    }

    pub fn drum_on(&self, voice: usize, step: usize) -> bool {
        voice < DRUM_VOICES && step < STEPS && (self.drum_mask[voice] >> step) & 1 == 1
    }

    pub fn set_drum_step(&mut self, voice: usize, step: usize, on: bool) {
        if voice >= DRUM_VOICES || step >= STEPS {
            return;
        }
        let bit = 1u16 << step;
        if on {
            self.drum_mask[voice] |= bit;
        } else {
            self.drum_mask[voice] &= !bit;
        }
    }

    pub fn drum_velocity(&self, voice: usize, step: usize) -> u8 {
        if voice >= DRUM_VOICES || step >= STEPS {
            return 100;
        }
        self.drum_vel[voice][step]
    }

    pub fn set_drum_velocity(&mut self, voice: usize, step: usize, velocity: i32) {
        if voice < DRUM_VOICES && step < STEPS {
            self.drum_vel[voice][step] = velocity.clamp(1, 127) as u8;
        }
    }

    pub fn apply_fx_defaults(&mut self, fx_type: usize) {
        let info = crate::dsp::fx_info(fx_type);
        self.fx_type = fx_type.min(crate::dsp::FX_COUNT - 1);
        for i in 0..6 {
            self.params[FxParam::BASE + i] = info.defaults[i];
        }
        self.params[FxParam::ENABLED] = if self.fx_type == 0 { 0.0 } else { 1.0 };
        self.fx_enabled = self.fx_type != 0;
    }

    pub fn reset_params(&mut self) {
        self.params = [0.0; PARAMS];
    }

    pub fn fx_params(&self) -> [f32; 6] {
        let mut out = [0.0; 6];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.params[FxParam::BASE + i];
        }
        out
    }
}

pub fn default_name(kind: ClipKind, index: usize) -> String {
    let number = format!("{:02}", index + 1);
    match kind {
        ClipKind::Empty => String::new(),
        ClipKind::Acid => format!("ACID {number}"),
        ClipKind::Wave => format!("WAVE {number}"),
        ClipKind::Drums => format!("909 {number}"),
        ClipKind::Sample => format!("SMP {number}"),
        ClipKind::Plugin => format!("VST {number}"),
    }
}

/// Reset a clip to the factory defaults for `kind`, keeping the FX insert.
pub fn apply_kind_defaults(clip: &mut Clip, kind: ClipKind) {
    let previous_default = default_name(clip.kind, clip.index);
    let previous_fx = clip.fx_type;
    let previous_fx_enabled = clip.fx_enabled;

    clip.kind = kind;
    clip.length = STEPS as i32;
    clip.mute = false;
    clip.solo = false;
    clip.sample_missing = false;
    clip.midi_notes.clear();
    if clip.name.trim().is_empty() || clip.name == previous_default {
        clip.name = default_name(kind, clip.index);
    }

    match kind {
        ClipKind::Empty => {
            clip.reset_params();
            clip.root = 60;
            clip.steps = [Step::default(); STEPS];
            clip.drum_mask = [0; DRUM_VOICES];
            clip.drum_vel = [[100; STEPS]; DRUM_VOICES];
            clip.sample_name = None;
            clip.sample_path = None;
            clip.fx_type = 0;
            clip.fx_enabled = false;
        }
        ClipKind::Acid => {
            clip.root = 36;
            clip.reset_params();
            let p = &mut clip.params;
            p[AcidParam::WAVE] = 0.0;
            p[AcidParam::CUTOFF] = 420.0;
            p[AcidParam::RESONANCE] = 0.72;
            p[AcidParam::ENV_MOD] = 0.68;
            p[AcidParam::DECAY] = 0.55;
            p[AcidParam::ACCENT] = 0.82;
            p[AcidParam::DIST_TYPE] = 0.0;
            p[AcidParam::DIST_DRIVE] = 0.55;
            p[AcidParam::DIST_MIX] = 0.55;
            p[AcidParam::DIST_TONE] = 0.62;
            p[AcidParam::VOLUME] = 0.9;

            let notes = [0, 0, 12, 0, 7, 12];
            let gates = [0usize, 3, 6, 8, 11, 14];
            clip.steps = [Step::default(); STEPS];
            for (i, step_index) in gates.iter().enumerate() {
                let accent = i == 0 || i == 3;
                clip.steps[*step_index] = Step {
                    gate: true,
                    note: (clip.root + notes[i]).clamp(0, 127) as u8,
                    velocity: if accent { 118 } else { 92 },
                    accent,
                    slide: i == 5,
                };
            }
            clip.name = format!("ACID {:02}", clip.index + 1);
        }
        ClipKind::Wave => {
            clip.root = 48;
            clip.reset_params();
            let p = &mut clip.params;
            p[WaveParam::BANK] = 0.0;
            p[WaveParam::POSITION] = 0.3;
            p[WaveParam::UNISON] = 3.0;
            p[WaveParam::DETUNE] = 16.0;
            p[WaveParam::SPREAD] = 0.5;
            p[WaveParam::CUTOFF] = 2200.0;
            p[WaveParam::RESONANCE] = 0.7;
            p[WaveParam::DRIVE] = 1.2;
            p[WaveParam::ATTACK] = 0.01;
            p[WaveParam::DECAY] = 0.4;
            p[WaveParam::SUSTAIN] = 0.6;
            p[WaveParam::RELEASE] = 0.5;
            p[WaveParam::SUB] = 0.2;
            p[WaveParam::NOISE] = 0.0;
            p[WaveParam::LEVEL] = 0.8;
            p[WaveParam::FILTER_MODE] = 0.0;
            p[WaveParam::CHORD] = 2.0;

            clip.steps = [Step::default(); STEPS];
            let offsets = [0, 3, 7, 12];
            let starts = [0, 4, 8, 12];
            clip.midi_notes = starts
                .iter()
                .enumerate()
                .map(|(i, step)| MidiNote::new(clip.root + offsets[i], *step, 4, 100))
                .collect();
            clip.name = format!("WAVE {:02}", clip.index + 1);
        }
        ClipKind::Drums => {
            clip.root = 36;
            clip.reset_params();
            let p = &mut clip.params;
            p[DrumParam::TUNE] = 0.5;
            p[DrumParam::DECAY] = 0.5;
            p[DrumParam::SNAPPY] = 0.5;
            p[DrumParam::TONE] = 0.5;
            p[DrumParam::ACCENT] = 0.7;
            p[DrumParam::LEVEL] = 0.9;
            p[DrumParam::CUTOFF] = 18000.0;
            p[DrumParam::RESONANCE] = 0.6;

            clip.drum_mask = [0; DRUM_VOICES];
            clip.drum_vel = [[100; STEPS]; DRUM_VOICES];
            for step in [0usize, 4, 8, 12] {
                clip.drum_mask[0] |= 1 << step;
                clip.drum_vel[0][step] = 120;
            }
            for step in [4usize, 12] {
                clip.drum_mask[1] |= 1 << step;
                clip.drum_vel[1][step] = 112;
            }
            for step in (0..STEPS).step_by(2) {
                clip.drum_mask[7] |= 1 << step;
                clip.drum_vel[7][step] = if step % 4 == 0 { 92 } else { 70 };
            }
            clip.drum_mask[8] |= 1 << 14;
            clip.drum_vel[8][14] = 84;
            clip.drum_mask[6] |= 1 << 11;
            clip.drum_vel[6][11] = 96;
            clip.steps = [Step::default(); STEPS];
            clip.name = format!("909 {:02}", clip.index + 1);
        }
        ClipKind::Sample => {
            clip.root = 60;
            clip.reset_params();
            clip.params[SampleParam::GAIN] = 1.0;
            clip.params[SampleParam::PITCH] = 0.0;
            clip.steps = [Step::default(); STEPS];
            if clip.sample_name.is_none() {
                clip.sample_name = Some("NO SAMPLE".to_string());
            }
            if clip.name.trim().is_empty() {
                clip.name = format!("SMP {:02}", clip.index + 1);
            }
        }
        ClipKind::Plugin => {
            clip.root = 60;
            clip.reset_params();
            clip.steps = [Step::default(); STEPS];
            clip.sample_name = None;
            clip.sample_path = None;
            if clip.name.trim().is_empty() {
                clip.name = clip
                    .plugin
                    .as_ref()
                    .map(|plugin| plugin.name.clone())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| format!("VST {:02}", clip.index + 1));
            }
        }
    }

    // Only a plugin pad may carry a loaded plugin.
    if kind != ClipKind::Plugin {
        clip.plugin = None;
    }

    if kind != ClipKind::Empty && previous_fx != 0 {
        clip.apply_fx_defaults(previous_fx);
        clip.fx_enabled = previous_fx_enabled;
    }
}

pub struct ClipLimits;

impl ClipLimits {
    pub const PADS: usize = PADS;
    pub const STEPS: usize = STEPS;
    pub const DRUM_VOICES: usize = DRUM_VOICES;
    pub const PARAMS: usize = PARAMS;
    pub const MAX_MIDI_NOTES: usize = MAX_MIDI_NOTES;
    pub const COLS: usize = COLS;
    pub const ROWS: usize = ROWS;
}
