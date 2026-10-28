//! polyBLEP saw / square oscillator with the 303's RC droop high-pass.

use crate::dsp::{clampf, PI_F};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    Sawtooth,
    Square,
}

#[derive(Clone, Debug)]
pub struct Oscillator303 {
    sample_rate: f64,
    waveform: Waveform,
    target_freq: f32,
    actual_freq: f32,
    tuning_semi: f32,
    phase: f32,
    phase_inc: f32,
    square_hp_state: f32,
    square_hp_coeff: f32,
}

impl Default for Oscillator303 {
    fn default() -> Self {
        let mut osc = Self {
            sample_rate: 44_100.0,
            waveform: Waveform::Sawtooth,
            target_freq: 110.0,
            actual_freq: 110.0,
            tuning_semi: 0.0,
            phase: 0.0,
            phase_inc: 0.0,
            square_hp_state: 0.0,
            square_hp_coeff: 0.992,
        };
        osc.prepare(44_100.0);
        osc
    }
}

impl Oscillator303 {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1_000.0);
        let rc = 1.0 / (2.0 * PI_F * 25.0);
        let dt = 1.0 / self.sample_rate as f32;
        self.square_hp_coeff = rc / (rc + dt);
        self.set_frequency(self.target_freq);
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.square_hp_state = 0.0;
    }

    pub fn set_waveform(&mut self, wave: Waveform) {
        self.waveform = wave;
    }

    pub fn set_frequency(&mut self, freq_hz: f32) {
        self.target_freq = freq_hz;
        let tuned = self.target_freq * 2.0_f32.powf(self.tuning_semi / 12.0);
        self.actual_freq = clampf(tuned, 10.0, self.sample_rate as f32 * 0.45);
        self.phase_inc = self.actual_freq / self.sample_rate as f32;
    }

    pub fn set_tuning_semi(&mut self, semi: f32) {
        self.tuning_semi = semi;
        self.set_frequency(self.target_freq);
    }

    #[inline]
    fn poly_blep(t: f32, dt: f32) -> f32 {
        if dt <= 0.0 {
            return 0.0;
        }
        if t < dt {
            let x = t / dt;
            x + x - x * x - 1.0
        } else if t > 1.0 - dt {
            let x = (t - 1.0) / dt;
            x * x + x + x + 1.0
        } else {
            0.0
        }
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let dt = self.phase_inc;

        if self.waveform == Waveform::Sawtooth {
            let naive_saw = 2.0 * self.phase - 1.0;
            let curved_saw = naive_saw - 0.15 * (naive_saw * naive_saw - 1.0);
            return (curved_saw - Self::poly_blep(self.phase, dt)) * 0.95;
        }

        let mut naive_square = if self.phase < 0.485 { 1.0 } else { -1.0 };
        naive_square += Self::poly_blep(self.phase, dt);
        let mut shifted = self.phase - 0.485;
        if shifted < 0.0 {
            shifted += 1.0;
        }
        naive_square -= Self::poly_blep(shifted, dt);

        let hp_out = self.square_hp_coeff * (self.square_hp_state + naive_square);
        self.square_hp_state = hp_out - naive_square;
        hp_out * 0.90
    }
}
