//! Shared, dependency-free DSP primitives.
//!
//! Ported one-to-one from the Android engine's `core/Dsp.h` so the desktop
//! build sounds identical to the phone build.

pub mod acid;
pub mod drums;
pub mod fx;
pub mod wavetable;

use std::f32::consts::PI;

pub const PI_F: f32 = PI;
pub const TWO_PI_F: f32 = 2.0 * PI;

pub use acid::Acid303;
pub use drums::DrumKit909;
pub use fx::{fx_catalog, fx_info, Effect, FxInfo, FX_COUNT};
pub use wavetable::{shared_wavetable_banks, WaveParams, WaveSynth, WavetableBanks};

#[inline]
pub fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

#[inline]
pub fn clampi(v: i32, lo: i32, hi: i32) -> i32 {
    v.max(lo).min(hi)
}

#[inline]
pub fn lerpf(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
pub fn midi_to_freq(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

#[inline]
pub fn freq_to_midi(hz: f32) -> f32 {
    69.0 + 12.0 * (hz.max(1.0) / 440.0).log2()
}

#[inline]
pub fn fast_tanh(x: f32) -> f32 {
    if x < -3.0 {
        return -1.0;
    }
    if x > 3.0 {
        return 1.0;
    }
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if x <= -1.5 {
        return -1.0;
    }
    if x >= 1.5 {
        return 1.0;
    }
    x - (4.0 / 27.0) * (x * x * x)
}

#[inline]
pub fn sanitize(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    if x.abs() < 1.0e-20 {
        0.0
    } else {
        x
    }
}

/// Transparent peak limiter. Everything below `threshold` passes untouched, so
/// drums keep their transients; only overload is bent towards 1.0 with a
/// continuous first derivative.
#[inline]
pub fn soft_limit(x: f32, threshold: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let ax = x.abs();
    if ax <= threshold {
        return x;
    }
    let range = (1.0 - threshold).max(1.0e-4);
    let over = ax - threshold;
    let shaped = threshold + range * (1.0 - (-over / range).exp());
    shaped.copysign(x)
}

#[inline]
pub fn fractf(x: f32) -> f32 {
    x - x.floor()
}

#[inline]
pub fn wrap01(x: f32) -> f32 {
    let v = x % 1.0;
    if v < 0.0 {
        v + 1.0
    } else {
        v
    }
}

/// Xorshift32: allocation-free and deterministic.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    pub fn seed(&mut self, s: u32) {
        self.state = if s == 0 { 0x9E37_79B9 } else { s };
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// -1 .. +1
    #[inline]
    pub fn noise(&mut self) -> f32 {
        self.next_u32() as i32 as f32 * (1.0 / 2_147_483_648.0)
    }

    #[inline]
    pub fn uniform(&mut self) -> f32 {
        self.next_u32() as f32 * (1.0 / 4_294_967_296.0)
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::new(0x9E37_79B9)
    }
}

/// One-pole low pass / parameter smoother. `coeff` is the per-sample step.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    z: f32,
}

impl OnePole {
    pub fn reset(&mut self, value: f32) {
        self.z = value;
    }

    #[inline]
    pub fn process(&mut self, x: f32, coeff: f32) -> f32 {
        self.z += (x - self.z) * clampf(coeff, 0.0, 1.0);
        self.z
    }

    pub fn value(&self) -> f32 {
        self.z
    }
}

/// Andrew Simper's zero-delay-feedback state variable filter.
#[derive(Clone, Copy, Debug)]
pub struct ZdfSvf {
    sample_rate: f32,
    ic1: f32,
    ic2: f32,
}

impl Default for ZdfSvf {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            ic1: 0.0,
            ic2: 0.0,
        }
    }
}

impl ZdfSvf {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8_000.0);
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    /// mode: 0 LP, 1 BP, 2 HP, 3 notch. `drive` >= 1 adds pre-filter saturation.
    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, q: f32, mode: i32, drive: f32) -> f32 {
        let fc = clampf(cutoff_hz, 20.0, 20_000.0);
        let g = (PI_F * fc.min(0.45 * self.sample_rate) / self.sample_rate).tan();
        let k = 1.0 / q.max(0.35);
        let drive_gain = drive.max(0.1);
        let x = fast_tanh(input * drive_gain) / drive_gain.sqrt();

        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        match mode {
            1 => v1,
            2 => x - k * v1 - v2,
            3 => x - k * v1,
            _ => v2,
        }
    }
}

/// Simple delay line with linear interpolation.
#[derive(Clone, Debug, Default)]
pub struct DelayLine {
    buf: Vec<f32>,
    write: usize,
}

impl DelayLine {
    pub fn prepare(&mut self, max_samples: usize) {
        self.buf = vec![0.0; max_samples.max(8)];
        self.write = 0;
    }

    pub fn reset(&mut self) {
        self.buf.fill(0.0);
        self.write = 0;
    }

    #[inline]
    pub fn write(&mut self, x: f32) {
        if self.buf.is_empty() {
            return;
        }
        self.buf[self.write] = sanitize(x);
        self.write = (self.write + 1) % self.buf.len();
    }

    /// Delay in samples, measured back from the current write position.
    #[inline]
    pub fn read(&self, delay_samples: f32) -> f32 {
        if self.buf.is_empty() {
            return 0.0;
        }
        let size = self.buf.len();
        let d = clampf(delay_samples, 1.0, (size - 4) as f32);
        let di = d as usize;
        let frac = d - di as f32;
        let i0 = (self.write + size * 2 - di) % size;
        let i1 = (i0 + size - 1) % size;
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }

    pub fn size(&self) -> usize {
        self.buf.len()
    }
}

/// ADSR with a linear attack and exponential decay/release.
#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    sample_rate: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    attack_step: f32,
    decay_coeff: f32,
    release_coeff: f32,
    level: f32,
    stage: Stage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Default for Adsr {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            attack: 0.005,
            decay: 0.25,
            sustain: 0.7,
            release: 0.3,
            attack_step: 0.001,
            decay_coeff: 0.001,
            release_coeff: 0.001,
            level: 0.0,
            stage: Stage::Idle,
        }
    }
}

impl Adsr {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1_000.0);
        self.update();
    }

    pub fn set_attack(&mut self, s: f32) {
        self.attack = clampf(s, 0.001, 8.0);
        self.update();
    }

    pub fn set_decay(&mut self, s: f32) {
        self.decay = clampf(s, 0.001, 8.0);
        self.update();
    }

    pub fn set_sustain(&mut self, v: f32) {
        self.sustain = clampf(v, 0.0, 1.0);
    }

    pub fn set_release(&mut self, s: f32) {
        self.release = clampf(s, 0.001, 8.0);
        self.update();
    }

    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    pub fn note_on(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn note_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    pub fn active(&self) -> bool {
        self.stage != Stage::Idle
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += self.attack_step;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level += (self.sustain - self.level) * self.decay_coeff;
                if self.level - self.sustain < 0.0005 {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                self.level = self.sustain;
            }
            Stage::Release => {
                self.level += (0.0 - self.level) * self.release_coeff;
                if self.level < 0.0002 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
            Stage::Idle => {
                self.level = 0.0;
            }
        }
        self.level
    }

    fn update(&mut self) {
        self.attack_step = 1.0 / (self.attack * self.sample_rate);
        self.decay_coeff = 1.0 - (-1.0 / (self.decay * self.sample_rate * 0.35)).exp();
        self.release_coeff = 1.0 - (-1.0 / (self.release * self.sample_rate * 0.35)).exp();
    }
}

/// Parameter smoothing for UI knobs: 5 ms one-pole.
#[derive(Clone, Copy, Debug, Default)]
pub struct SmoothedValue {
    pub current: f32,
    pub target: f32,
    pub coeff: f32,
}

impl SmoothedValue {
    pub fn set_sample_rate(&mut self, sr: f32) {
        self.coeff = 1.0 - (-1.0 / (0.005 * sr)).exp();
    }

    pub fn set(&mut self, v: f32) {
        self.target = v;
    }

    pub fn snap(&mut self, v: f32) {
        self.target = v;
        self.current = v;
    }

    /// Step the smoother one sample and return the new value.
    #[inline]
    pub fn step(&mut self) -> f32 {
        self.current += (self.target - self.current) * self.coeff;
        self.current
    }
}

/// Convert a decibel value to a linear gain.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert a linear gain to decibels, clamped to silence.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 1.0e-5 {
        -96.0
    } else {
        20.0 * gain.log10()
    }
}
