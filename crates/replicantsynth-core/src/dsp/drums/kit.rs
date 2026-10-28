//! The eleven-voice 909 kit: linear bus with a fixed headroom trim and a
//! transparent peak limiter instead of a saturating sum.

use super::voices::{BassDrum, Cymbal, CymbalType, HandClap, RimShot, SnareDrum, Tom, TomType};
use super::{DrumVoice, VoiceParams, VOICE_NAMES};
use crate::dsp::{clampf, soft_limit, ZdfSvf, PI_F};

pub const VOICE_COUNT: usize = 11;

// Voice order used by the editor and the pad sequencer. The names mirror the
// C++ kit so a reader can compare the two files line by line.
#[allow(dead_code)]
const BD: usize = 0;
#[allow(dead_code)]
const SD: usize = 1;
#[allow(dead_code)]
const LT: usize = 2;
#[allow(dead_code)]
const MT: usize = 3;
#[allow(dead_code)]
const HT: usize = 4;
#[allow(dead_code)]
const RS: usize = 5;
#[allow(dead_code)]
const CP: usize = 6;
const CH: usize = 7;
const OH: usize = 8;
#[allow(dead_code)]
const CC: usize = 9;
#[allow(dead_code)]
const RC: usize = 10;

pub struct DrumKit909 {
    voices: Vec<Box<dyn DrumVoice>>,
    base_level: [f32; VOICE_COUNT],
    pan: [f32; VOICE_COUNT],
    filter_l: ZdfSvf,
    filter_r: ZdfSvf,
    cutoff: f32,
    resonance: f32,
    level: f32,
    accent_scale: f32,
}

impl Default for DrumKit909 {
    fn default() -> Self {
        Self::new()
    }
}

impl DrumKit909 {
    pub fn new() -> Self {
        let voices: Vec<Box<dyn DrumVoice>> = vec![
            Box::new(BassDrum::new()),
            Box::new(SnareDrum::new()),
            Box::new(Tom::new(TomType::Low)),
            Box::new(Tom::new(TomType::Mid)),
            Box::new(Tom::new(TomType::Hi)),
            Box::new(RimShot::new()),
            Box::new(HandClap::new()),
            Box::new(Cymbal::new(CymbalType::ClosedHat)),
            Box::new(Cymbal::new(CymbalType::OpenHat)),
            Box::new(Cymbal::new(CymbalType::Crash)),
            Box::new(Cymbal::new(CymbalType::Ride)),
        ];
        let mut kit = Self {
            voices,
            base_level: [
                1.00, 0.82, 0.72, 0.70, 0.68, 0.62, 0.72, 0.58, 0.52, 0.60, 0.55,
            ],
            pan: [
                0.0, 0.02, -0.28, 0.0, 0.28, -0.45, 0.42, 0.34, -0.34, -0.55, 0.55,
            ],
            filter_l: ZdfSvf::default(),
            filter_r: ZdfSvf::default(),
            cutoff: 18_000.0,
            resonance: 0.6,
            level: 0.9,
            accent_scale: 0.7,
        };
        kit.prepare(48_000.0);
        kit
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        for voice in self.voices.iter_mut() {
            voice.prepare(sample_rate);
        }
        self.filter_l.prepare(sample_rate as f32);
        self.filter_r.prepare(sample_rate as f32);
        self.reset();
    }

    pub fn reset(&mut self) {
        for voice in self.voices.iter_mut() {
            voice.reset();
        }
        self.filter_l.reset();
        self.filter_r.reset();
    }

    /// tune/decay/snappy/tone/accent/level in 0..1, cutoff in Hz, res 0..1.
    #[allow(clippy::too_many_arguments)]
    pub fn set_params(
        &mut self,
        tune: f32,
        decay: f32,
        snappy: f32,
        tone: f32,
        accent: f32,
        level: f32,
        cutoff: f32,
        resonance: f32,
    ) {
        let t = clampf(tune, 0.0, 1.0);
        let d = clampf(decay, 0.0, 1.0);
        let s = clampf(snappy, 0.0, 1.0);
        let tn = clampf(tone, 0.0, 1.0);
        self.accent_scale = clampf(accent, 0.0, 1.0);
        self.level = clampf(level, 0.0, 1.5);
        self.cutoff = clampf(cutoff, 200.0, 20_000.0);
        self.resonance = clampf(resonance, 0.5, 12.0);

        for (i, voice) in self.voices.iter_mut().enumerate() {
            voice.set_params(VoiceParams {
                tune: t,
                decay: d,
                snappy: s,
                tone: tn,
                level: self.base_level[i],
                pan: 0.5 + self.pan[i] * 0.5,
                attack: 0.5,
            });
        }
    }

    pub fn trigger(&mut self, voice: usize, velocity: f32, accent: f32) {
        if voice >= VOICE_COUNT {
            return;
        }
        if voice == CH {
            // Hardware choke: a closed hat discharges the open-hat envelope.
            self.voices[OH].choke();
        }
        self.voices[voice].trigger(velocity, accent * self.accent_scale);
    }

    pub fn trigger_all(&mut self, velocity: f32) {
        for voice in 0..VOICE_COUNT {
            self.trigger(voice, velocity, 0.0);
        }
    }

    pub fn choke_open_hat(&mut self) {
        self.voices[OH].choke();
    }

    pub fn render(&mut self, out_l: &mut f32, out_r: &mut f32) {
        let mut l = 0.0f32;
        let mut r = 0.0f32;
        for i in 0..VOICE_COUNT {
            if !self.voices[i].is_active() {
                continue;
            }
            let s = self.voices[i].render_sample();
            let pan = clampf(self.pan[i], -1.0, 1.0);
            let angle = (pan + 1.0) * 0.25 * PI_F;
            l += s * angle.cos();
            r += s * angle.sin();
        }

        if self.cutoff < 19_000.0 {
            l = self
                .filter_l
                .process(l, self.cutoff, self.resonance, 0, 1.0);
            r = self
                .filter_r
                .process(r, self.cutoff, self.resonance, 0, 1.0);
        }

        // Keep the eleven-voice sum linear; only catch overload peaks.
        const KIT_TRIM: f32 = 0.75;
        let bus = self.level * KIT_TRIM;
        *out_l = soft_limit(l * bus, 0.90);
        *out_r = soft_limit(r * bus, 0.90);
    }

    pub fn any_active(&self) -> bool {
        self.voices.iter().any(|v| v.is_active())
    }

    pub fn voice_name(voice: usize) -> &'static str {
        if voice < VOICE_COUNT {
            VOICE_NAMES[voice]
        } else {
            "--"
        }
    }
}
