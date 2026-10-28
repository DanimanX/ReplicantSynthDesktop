//! Five-model 303 distortion: Poco Rat, Tube Drive, Hard Clip, Germanium Fuzz
//! and Wavefolder, with the pre high-pass, post tone low-pass and dry/wet mix.

use crate::dsp::{clampf, fast_tanh, soft_clip, PI_F};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistortionType {
    PocoRat = 0,
    TubeDrive = 1,
    HardClip = 2,
    GermaniumFuzz = 3,
    Wavefolder = 4,
}

impl DistortionType {
    pub fn from_index(index: i32) -> Self {
        match index.clamp(0, 4) {
            1 => Self::TubeDrive,
            2 => Self::HardClip,
            3 => Self::GermaniumFuzz,
            4 => Self::Wavefolder,
            _ => Self::PocoRat,
        }
    }

    pub const NAMES: [&'static str; 5] = ["POCO RAT", "TUBE", "HARD CLIP", "FUZZ", "WAVEFOLD"];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }
}

#[inline]
fn diode_clip(x: f32) -> f32 {
    x / (1.0 + x * x).sqrt()
}

#[inline]
fn hard_clip(x: f32, threshold: f32) -> f32 {
    x.clamp(-threshold, threshold)
}

#[inline]
fn wavefold(x: f32, fold_amount: f32) -> f32 {
    let gain = 1.0 + fold_amount * 5.0;
    let scaled = x * gain;
    let folded = 4.0 * (0.25 * scaled - (0.25 * scaled + 0.75).floor() + 0.25).abs() - 1.0;
    soft_clip(folded)
}

#[derive(Clone, Debug)]
pub struct DistortionUnit {
    sample_rate: f64,
    active: bool,
    current_type: DistortionType,
    drive: f32,
    tone: f32,
    mix: f32,
    pre_hp_state: f32,
    pre_hp_alpha: f32,
    post_lp_state: f32,
    post_lp_alpha: f32,
    slew_state: f32,
}

impl Default for DistortionUnit {
    fn default() -> Self {
        let mut unit = Self {
            sample_rate: 44_100.0,
            active: true,
            current_type: DistortionType::PocoRat,
            drive: 0.5,
            tone: 0.5,
            mix: 1.0,
            pre_hp_state: 0.0,
            pre_hp_alpha: 0.98,
            post_lp_state: 0.0,
            post_lp_alpha: 0.85,
            slew_state: 0.0,
        };
        unit.prepare(44_100.0);
        unit
    }
}

impl DistortionUnit {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1_000.0);
        let rc = 1.0 / (2.0 * PI_F * 450.0);
        let dt = 1.0 / self.sample_rate as f32;
        self.pre_hp_alpha = rc / (rc + dt);
        self.set_tone(self.tone);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.pre_hp_state = 0.0;
        self.post_lp_state = 0.0;
        self.slew_state = 0.0;
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn set_type(&mut self, kind: DistortionType) {
        self.current_type = kind;
    }

    pub fn set_drive(&mut self, drive01: f32) {
        self.drive = clampf(drive01, 0.0, 1.0);
    }

    pub fn set_mix(&mut self, mix01: f32) {
        self.mix = clampf(mix01, 0.0, 1.0);
    }

    pub fn set_tone(&mut self, tone01: f32) {
        self.tone = clampf(tone01, 0.0, 1.0);
        let cutoff_hz = 800.0 * 20.0_f32.powf(self.tone);
        let rc = 1.0 / (2.0 * PI_F * cutoff_hz);
        let dt = 1.0 / self.sample_rate as f32;
        self.post_lp_alpha = dt / (rc + dt);
    }

    #[inline]
    fn process_poco_rat(&mut self, input: f32) -> f32 {
        let hp_out = self.pre_hp_alpha * (self.pre_hp_state + input);
        self.pre_hp_state = hp_out - input;

        let gain = 1.0 + self.drive * self.drive * 40.0;
        let amplified = hp_out * gain;

        let max_slew = 0.18 + (1.0 - self.drive) * 0.40;
        let diff = amplified - self.slew_state;
        self.slew_state += diff.clamp(-max_slew, max_slew);

        fast_tanh(self.slew_state * 1.6) * 0.85
    }

    #[inline]
    fn process_tube_drive(&self, input: f32) -> f32 {
        let gain = 1.0 + self.drive * 24.0;
        let mut sat = soft_clip(input * gain);
        if sat > 0.0 {
            sat = sat.powf(0.94);
        }
        sat * 0.90
    }

    #[inline]
    fn process_hard_clip(&self, input: f32) -> f32 {
        let gain = 1.0 + self.drive * 35.0;
        let clipped = hard_clip(input * gain * 0.8, 0.95);
        fast_tanh(clipped * 1.2) * 0.85
    }

    #[inline]
    fn process_germanium_fuzz(&self, input: f32) -> f32 {
        let gain = 1.0 + self.drive * 45.0;
        let biased = input * gain + 0.18 * (self.drive * self.drive);
        let clipped = if biased > 0.0 {
            diode_clip(biased * 1.4)
        } else {
            fast_tanh(biased * 2.1)
        };
        (clipped - 0.12 * self.drive) * 0.80
    }

    #[inline]
    fn process_wavefolder(&self, input: f32) -> f32 {
        wavefold(input, self.drive) * 0.80
    }

    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        if !self.active {
            return input;
        }

        let mut wet = match self.current_type {
            DistortionType::PocoRat => self.process_poco_rat(input),
            DistortionType::TubeDrive => self.process_tube_drive(input),
            DistortionType::HardClip => self.process_hard_clip(input),
            DistortionType::GermaniumFuzz => self.process_germanium_fuzz(input),
            DistortionType::Wavefolder => self.process_wavefolder(input),
        };

        self.post_lp_state += self.post_lp_alpha * (wet - self.post_lp_state);
        wet = self.post_lp_state;
        (1.0 - self.mix) * input + self.mix * wet
    }
}
