//! 303 VCF / VCA / accent envelopes.

use crate::dsp::clampf;

#[derive(Clone, Debug)]
pub struct Envelope303 {
    sample_rate: f64,
    decay_time_sec: f32,
    normal_decay_coeff: f32,
    accent_decay_coeff: f32,
    vca_hold_decay_coeff: f32,
    vca_release_coeff: f32,
    vcf_env: f32,
    vca_env: f32,
    accent_env: f32,
    gate: bool,
    accented: bool,
}

impl Default for Envelope303 {
    fn default() -> Self {
        let mut env = Self {
            sample_rate: 44_100.0,
            decay_time_sec: 0.8,
            normal_decay_coeff: 0.999,
            accent_decay_coeff: 0.999,
            vca_hold_decay_coeff: 0.9999,
            vca_release_coeff: 0.99,
            vcf_env: 0.0,
            vca_env: 0.0,
            accent_env: 0.0,
            gate: false,
            accented: false,
        };
        env.prepare(44_100.0);
        env
    }
}

impl Envelope303 {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1_000.0);
        self.update_coefficients();
    }

    pub fn reset(&mut self) {
        self.vcf_env = 0.0;
        self.vca_env = 0.0;
        self.accent_env = 0.0;
        self.gate = false;
        self.accented = false;
    }

    pub fn set_decay_time(&mut self, seconds: f32) {
        self.decay_time_sec = clampf(seconds, 0.15, 2.8);
        self.update_coefficients();
    }

    pub fn note_on(&mut self, accented: bool, is_slide: bool) {
        self.gate = true;
        self.accented = accented;

        if !is_slide {
            self.vcf_env = 1.0;
            self.vca_env = 1.0;
            if accented {
                self.accent_env = 1.0;
            }
        } else if accented && self.accent_env < 0.2 {
            self.accent_env = 0.8;
        }
    }

    pub fn note_off(&mut self) {
        self.gate = false;
    }

    #[inline]
    pub fn process(&mut self) -> (f32, f32, f32) {
        let active_decay = if self.accented {
            self.accent_decay_coeff
        } else {
            self.normal_decay_coeff
        };
        self.vcf_env *= active_decay;
        self.accent_env *= self.accent_decay_coeff;
        self.vca_env *= if self.gate {
            self.vca_hold_decay_coeff
        } else {
            self.vca_release_coeff
        };
        (self.vcf_env, self.vca_env, self.accent_env)
    }

    pub fn is_active(&self) -> bool {
        self.vca_env > 0.0001 || self.gate
    }

    fn update_coefficients(&mut self) {
        let dt = 1.0 / self.sample_rate as f32;
        self.normal_decay_coeff = (-dt / self.decay_time_sec).exp();
        self.accent_decay_coeff = (-dt / 0.18).exp();
        self.vca_hold_decay_coeff = (-dt / 4.0).exp();
        self.vca_release_coeff = (-dt / 0.025).exp();
    }
}
