//! Diode ladder low-pass with a saturated feedback path.

use crate::dsp::{clampf, fast_tanh, PI_F};

#[derive(Clone, Debug)]
pub struct DiodeLadderFilter {
    sample_rate: f64,
    base_cutoff_hz: f32,
    resonance: f32,
    stage: [f32; 4],
    hp_state: f32,
    hp_alpha: f32,
}

impl Default for DiodeLadderFilter {
    fn default() -> Self {
        let mut filter = Self {
            sample_rate: 44_100.0,
            base_cutoff_hz: 400.0,
            resonance: 0.5,
            stage: [0.0; 4],
            hp_state: 0.0,
            hp_alpha: 0.995,
        };
        filter.prepare(44_100.0);
        filter
    }
}

impl DiodeLadderFilter {
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1_000.0);
        let rc = 1.0 / (2.0 * PI_F * 40.0);
        let dt = 1.0 / self.sample_rate as f32;
        self.hp_alpha = rc / (rc + dt);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.stage = [0.0; 4];
        self.hp_state = 0.0;
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        self.base_cutoff_hz = clampf(cutoff_hz, 25.0, 4_800.0);
    }

    pub fn set_resonance(&mut self, res01: f32) {
        self.resonance = clampf(res01, 0.0, 1.0);
    }

    #[inline]
    pub fn process(&mut self, input: f32, env_mod01: f32, env_val: f32, accent_val: f32) -> f32 {
        let octaves_mod = env_mod01 * env_val * 5.2 + accent_val * 2.2;
        let mut mod_cutoff = self.base_cutoff_hz * 2.0_f32.powf(octaves_mod);
        mod_cutoff = clampf(mod_cutoff, 25.0, self.sample_rate as f32 * 0.42);

        let wd = 2.0 * PI_F * mod_cutoff;
        let t = 1.0 / self.sample_rate as f32;
        let wa = (2.0 / t) * (wd * t * 0.5).tan();
        let mut g = wa * t * 0.5;
        g = clampf(g, 0.001, 0.95);

        let k = self.resonance * 17.2;
        let drive = 1.0 + self.resonance * 0.4;
        let sub_g = g * 0.5;

        let mut out = 0.0;
        for _ in 0..2 {
            let fb_signal = self.stage[3];
            let hp_out = self.hp_alpha * (self.hp_state + fb_signal);
            self.hp_state = hp_out - fb_signal;

            let saturated_feedback = fast_tanh(k * hp_out);
            let u = fast_tanh(input * drive - saturated_feedback);

            let du0 = sub_g * (u - fast_tanh(self.stage[0]));
            self.stage[0] = clampf(self.stage[0] + du0, -4.0, 4.0);
            let du1 = sub_g * (fast_tanh(self.stage[0]) - fast_tanh(self.stage[1]));
            self.stage[1] = clampf(self.stage[1] + du1, -4.0, 4.0);
            let du2 = sub_g * (fast_tanh(self.stage[1]) - fast_tanh(self.stage[2]));
            self.stage[2] = clampf(self.stage[2] + du2, -4.0, 4.0);
            let du3 = sub_g * (fast_tanh(self.stage[2]) - fast_tanh(self.stage[3]));
            self.stage[3] = clampf(self.stage[3] + du3, -4.0, 4.0);
            out = self.stage[3];
        }

        fast_tanh(out * 1.5)
    }
}
