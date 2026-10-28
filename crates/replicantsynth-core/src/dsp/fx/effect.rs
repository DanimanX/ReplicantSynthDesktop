//! The SP-404MKII match arms, ported from the Rust plug-in. Parameters are
//! smoothed, and each clip owns an independent delay line and comb bank.

use super::catalog::FX_COUNT;
use crate::dsp::{clampf, clampi, sanitize, wrap01, DelayLine, Rng, ZdfSvf, PI_F, TWO_PI_F};

pub const MAX_DELAY: usize = 72_000; // 1.5 s at 48 kHz
const COMB_VOICES: usize = 6;
const COMB_LEN: usize = 2048;
const FX_ID_BYPASS: usize = 0;

const CHORD_INTERVALS: [[f32; 6]; 9] = [
    [0.0, 0.0, 12.0, 12.0, 24.0, 24.0],   // Root
    [0.0, 7.0, 12.0, 19.0, 24.0, 31.0],   // P5
    [0.0, -12.0, 12.0, 24.0, -24.0, 0.0], // Oct
    [0.0, -12.0, 7.0, 12.0, 19.0, 24.0],  // UpDn
    [0.0, 3.0, 7.0, 12.0, 15.0, 19.0],    // m3
    [0.0, 3.0, 7.0, 10.0, 12.0, 15.0],    // m7
    [0.0, 4.0, 7.0, 12.0, 16.0, 19.0],    // M3
    [0.0, 4.0, 7.0, 11.0, 12.0, 16.0],    // M7
    [0.0, 4.0, 7.0, 11.0, 14.0, 19.0],    // M9
];

#[inline]
fn band_gain(v: f32, kill: f32) -> f32 {
    if kill >= 0.5 || v < 0.02 {
        return 0.0;
    }
    if v <= 0.5 {
        let x = v * 2.0;
        x * x
    } else {
        1.0 + (v - 0.5) * 6.0
    }
}

#[inline]
fn wrap(x: f32, limit: f32) -> f32 {
    if limit <= 0.0 {
        return 0.0;
    }
    let v = x % limit;
    if v < 0.0 {
        v + limit
    } else {
        v
    }
}

pub struct Effect {
    fx_id: usize,
    enabled: bool,
    sample_rate: f32,
    params: [f32; 6],
    smoothed: [f32; 6],

    svf_l: [ZdfSvf; 3],
    svf_r: [ZdfSvf; 3],
    low_l: f32,
    low_r: f32,
    high_l: f32,
    high_r: f32,
    post_l: f32,
    post_r: f32,

    delay_l: DelayLine,
    delay_r: DelayLine,
    looper_playhead: f32,
    looper_active: bool,

    comb: Vec<f32>,
    comb_pos: [usize; COMB_VOICES],
    comb_lp: [f32; COMB_VOICES],

    allpass_l: [f32; 12],
    allpass_r: [f32; 12],
    phaser_fb_l: f32,
    phaser_fb_r: f32,
    rev_comb_l: [f32; 4],
    rev_comb_r: [f32; 4],

    lfo_phase: f32,
    lfo_phase2: f32,
    env_follower: f32,
    comp_env: f32,
    srh_counter: f32,
    srh_l: f32,
    srh_r: f32,
    sub_sign_l: f32,
    sub_prev_l: f32,
    sub_lp: f32,
    stopper_speed: f32,
    stopper_phase: f32,
    rng: Rng,
}

impl Default for Effect {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect {
    pub fn new() -> Self {
        let mut effect = Self {
            fx_id: 0,
            enabled: true,
            sample_rate: 48_000.0,
            params: [0.5; 6],
            smoothed: [0.5; 6],
            svf_l: [ZdfSvf::default(); 3],
            svf_r: [ZdfSvf::default(); 3],
            low_l: 0.0,
            low_r: 0.0,
            high_l: 0.0,
            high_r: 0.0,
            post_l: 0.0,
            post_r: 0.0,
            delay_l: DelayLine::default(),
            delay_r: DelayLine::default(),
            looper_playhead: 0.0,
            looper_active: false,
            comb: vec![0.0; COMB_VOICES * COMB_LEN],
            comb_pos: [0; COMB_VOICES],
            comb_lp: [0.0; COMB_VOICES],
            allpass_l: [0.0; 12],
            allpass_r: [0.0; 12],
            phaser_fb_l: 0.0,
            phaser_fb_r: 0.0,
            rev_comb_l: [0.0; 4],
            rev_comb_r: [0.0; 4],
            lfo_phase: 0.0,
            lfo_phase2: 0.0,
            env_follower: 0.0,
            comp_env: 0.0,
            srh_counter: 0.0,
            srh_l: 0.0,
            srh_r: 0.0,
            sub_sign_l: 1.0,
            sub_prev_l: 0.0,
            sub_lp: 0.0,
            stopper_speed: 1.0,
            stopper_phase: 0.0,
            rng: Rng::new(0x0404_0303),
        };
        effect.prepare(48_000.0);
        effect
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8_000.0);
        for i in 0..3 {
            self.svf_l[i].prepare(self.sample_rate);
            self.svf_r[i].prepare(self.sample_rate);
        }
        self.delay_l.prepare(MAX_DELAY);
        self.delay_r.prepare(MAX_DELAY);
        self.reset();
    }

    pub fn reset(&mut self) {
        for i in 0..3 {
            self.svf_l[i].reset();
            self.svf_r[i].reset();
        }
        self.low_l = 0.0;
        self.low_r = 0.0;
        self.high_l = 0.0;
        self.high_r = 0.0;
        self.post_l = 0.0;
        self.post_r = 0.0;
        self.delay_l.reset();
        self.delay_r.reset();
        self.looper_playhead = 0.0;
        self.looper_active = false;
        self.comb.fill(0.0);
        self.comb_pos = [0; COMB_VOICES];
        self.comb_lp = [0.0; COMB_VOICES];
        self.allpass_l = [0.0; 12];
        self.allpass_r = [0.0; 12];
        self.phaser_fb_l = 0.0;
        self.phaser_fb_r = 0.0;
        self.rev_comb_l = [0.0; 4];
        self.rev_comb_r = [0.0; 4];
        self.lfo_phase = 0.0;
        self.lfo_phase2 = 0.0;
        self.env_follower = 0.0;
        self.comp_env = 0.0;
        self.srh_counter = 0.0;
        self.srh_l = 0.0;
        self.srh_r = 0.0;
        self.sub_sign_l = 1.0;
        self.sub_prev_l = 0.0;
        self.sub_lp = 0.0;
        self.stopper_speed = 1.0;
        self.stopper_phase = 0.0;
    }

    pub fn set_type(&mut self, id: usize) {
        let next = id.min(FX_COUNT - 1);
        if next == self.fx_id {
            return;
        }
        self.fx_id = next;
        self.looper_active = false;
        self.stopper_speed = 1.0;
        self.stopper_phase = 0.0;
        self.smoothed = self.params;
    }

    pub fn fx_type(&self) -> usize {
        self.fx_id
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_param(&mut self, index: usize, value: f32) {
        if index < 6 {
            self.params[index] = clampf(value, 0.0, 1.0);
        }
    }

    pub fn set_all_params(&mut self, values: &[f32; 6]) {
        for (i, value) in values.iter().enumerate() {
            self.set_param(i, *value);
        }
    }

    pub fn param(&self, index: usize) -> f32 {
        self.params.get(index).copied().unwrap_or(0.0)
    }

    #[inline]
    fn svf_l(&mut self, stage: usize, input: f32, cutoff: f32, k: f32, mode: i32) -> f32 {
        let idx = clampi(stage as i32, 0, 2) as usize;
        self.svf_l[idx].process(input, cutoff, 1.0 / k.max(0.2), mode, 1.0)
    }

    #[inline]
    fn svf_r(&mut self, stage: usize, input: f32, cutoff: f32, k: f32, mode: i32) -> f32 {
        let idx = clampi(stage as i32, 0, 2) as usize;
        self.svf_r[idx].process(input, cutoff, 1.0 / k.max(0.2), mode, 1.0)
    }

    #[allow(clippy::needless_range_loop)]
    pub fn process(&mut self, out_l: &mut f32, out_r: &mut f32, bpm: f32) {
        if !self.enabled {
            return;
        }

        for i in 0..6 {
            self.smoothed[i] += (self.params[i] - self.smoothed[i]) * 0.003;
        }
        let p = self.smoothed;
        let sr = self.sample_rate;
        let safe_bpm = clampf(bpm, 40.0, 240.0);
        let in_l = *out_l;
        let in_r = *out_r;
        let mut l = in_l;
        let mut r = in_r;
        let mut write_l = in_l;
        let mut write_r = in_r;

        let abs_in = 0.5 * (in_l.abs() + in_r.abs());
        let env_coeff = if abs_in > self.env_follower {
            0.015
        } else {
            0.0008
        };
        self.env_follower += (abs_in - self.env_follower) * env_coeff;

        match self.fx_id {
            0 => {
                // Bypass / trim
                let gain = 0.25 + p[0] * 1.5;
                let pan = p[1];
                let phase = if p[3] >= 0.5 { -1.0 } else { 1.0 };
                l = in_l * gain * phase * (1.0f32).min(1.0 - pan * 0.5);
                r = in_r * gain * phase * (1.0f32).min(0.5 + pan * 0.5);
            }
            1 => {
                // Filter+Drive
                let cutoff = 25.0 * 800.0_f32.powf(p[0]);
                let reso = clampf(p[1], 0.0, 0.96);
                let drive = 1.0 + p[2] * 9.0;
                let mode = (p[3] * 2.99) as i32;
                let low_boost = (p[5] - 0.5) * 3.0;
                let k = 2.0 - 1.92 * reso;

                let flt_l = self.svf_l(0, in_l, cutoff, k, mode);
                let flt_r = self.svf_r(0, in_r, cutoff, k, mode);
                let low_g = (PI_F * (40.0 + p[4] * 250.0) / sr).min(0.4);
                self.low_l += low_g * (flt_l - self.low_l);
                self.low_r += low_g * (flt_r - self.low_r);

                let pre_l = (flt_l + self.low_l * low_boost) * drive;
                let pre_r = (flt_r + self.low_r * low_boost) * drive;
                l = (pre_l + 0.12 * pre_l * pre_l).tanh() / drive.sqrt();
                r = (pre_r + 0.12 * pre_r * pre_r).tanh() / drive.sqrt();
            }
            3 => {
                // Isolator
                let g_low = (PI_F * 240.0 / sr).min(0.45);
                let g_high = (PI_F * 2800.0 / sr).min(0.85);
                self.low_l += g_low * (in_l - self.low_l);
                self.low_r += g_low * (in_r - self.low_r);
                self.high_l += g_high * (in_l - self.high_l);
                self.high_r += g_high * (in_r - self.high_r);
                let mid_l = self.high_l - self.low_l;
                let mid_r = self.high_r - self.low_r;
                let top_l = in_l - self.high_l;
                let top_r = in_r - self.high_r;
                let gl = band_gain(p[0], p[3]);
                let gm = band_gain(p[1], p[4]);
                let gh = band_gain(p[2], p[5]);
                l = (self.low_l * gl + mid_l * gm + top_l * gh).tanh();
                r = (self.low_r * gl + mid_r * gm + top_r * gh).tanh();
            }
            4 => {
                // Crusher
                let cutoff = 80.0 * 200.0_f32.powf(p[0]);
                let decim = 1.0 + p[1] * p[1] * 48.0;
                let wet = p[2];
                let bits = 16.0 - p[3] * 13.0;
                let steps = 2.0_f32.powf(bits);
                let drive = 1.0 + p[5] * 4.0;

                self.srh_counter += 1.0;
                if self.srh_counter >= decim {
                    self.srh_counter -= decim;
                    self.srh_l = ((in_l * drive * steps).round() / steps).tanh();
                    self.srh_r = ((in_r * drive * steps).round() / steps).tanh();
                }
                let k = 2.0 - 1.8 * clampf(p[4], 0.0, 0.9);
                let flt_l = self.svf_l(0, self.srh_l, cutoff, k, 0);
                let flt_r = self.svf_r(0, self.srh_r, cutoff, k, 0);
                l = in_l * (1.0 - wet) + flt_l * wet;
                r = in_r * (1.0 - wet) + flt_r * wet;
            }
            5 => {
                // Phaser
                let rate = 0.08 + p[0] * p[0] * 8.0;
                self.lfo_phase = wrap01(self.lfo_phase + rate / sr);
                let lfo_l = (self.lfo_phase * TWO_PI_F).sin();
                let lfo_r = ((self.lfo_phase + 0.25) * TWO_PI_F).sin();
                let stages = if p[4] < 0.25 {
                    4
                } else if p[4] < 0.5 {
                    8
                } else {
                    12
                };
                let base_f = 120.0 * 25.0_f32.powf(p[3]);
                let f_l = clampf(base_f * 2.0_f32.powf(lfo_l * p[1] * 2.2), 60.0, sr * 0.42);
                let f_r = clampf(base_f * 2.0_f32.powf(lfo_r * p[1] * 2.2), 60.0, sr * 0.42);
                let g_l = ((PI_F * f_l / sr).tan() - 1.0) / ((PI_F * f_l / sr).tan() + 1.0);
                let g_r = ((PI_F * f_r / sr).tan() - 1.0) / ((PI_F * f_r / sr).tan() + 1.0);
                let fb = p[2] * 0.82;
                let mut xl = (in_l + self.phaser_fb_l * fb).tanh();
                let mut xr = (in_r + self.phaser_fb_r * fb).tanh();
                for s in 0..stages {
                    let yl = g_l * xl + self.allpass_l[s];
                    self.allpass_l[s] = xl - g_l * yl;
                    xl = yl;
                    let yr = g_r * xr + self.allpass_r[s];
                    self.allpass_r[s] = xr - g_r * yr;
                    xr = yr;
                }
                self.phaser_fb_l = xl;
                self.phaser_fb_r = xr;
                let wet = p[5];
                l = in_l * (1.0 - wet * 0.5) + xl * wet * 0.7;
                r = in_r * (1.0 - wet * 0.5) + xr * wet * 0.7;
            }
            6 | 14 | 15 => {
                // Flanger / Chorus / JUNO Chorus
                let (rate, depth_ms, base_ms, fb, noise_amt, wet);
                if self.fx_id == 15 {
                    let mode = (p[0] * 4.99) as i32;
                    let mode_rate = match mode {
                        0 => 0.52,
                        1 => 0.86,
                        2 => 0.68,
                        3 => 1.35,
                        _ => 2.10,
                    };
                    rate = mode_rate * (0.5 + p[3]);
                    depth_ms = 1.8 + p[4] * 3.2;
                    base_ms = 3.4;
                    fb = 0.15;
                    noise_amt = p[1] * 0.008;
                    wet = p[2];
                } else if self.fx_id == 14 {
                    rate = 0.15 + p[0] * 4.0;
                    depth_ms = 1.5 + p[1] * 4.5;
                    base_ms = 7.0;
                    fb = 0.1;
                    noise_amt = 0.0;
                    wet = p[2];
                } else {
                    rate = 0.08 + p[0] * 6.0;
                    depth_ms = 0.4 + p[1] * 3.5;
                    base_ms = 0.8 + p[3] * 4.0;
                    fb = p[2] * 0.84;
                    noise_amt = 0.0;
                    wet = p[5];
                }
                self.lfo_phase = wrap01(self.lfo_phase + rate / sr);
                let lfo_l = (self.lfo_phase * TWO_PI_F).sin();
                let lfo_r = -lfo_l;
                let d_l = (base_ms + (0.5 + 0.5 * lfo_l) * depth_ms) * 0.001 * sr;
                let d_r = (base_ms + (0.5 + 0.5 * lfo_r) * depth_ms) * 0.001 * sr;
                let tap_l = self.delay_l.read(d_l);
                let tap_r = self.delay_r.read(d_r);
                write_l = in_l + tap_l * fb;
                write_r = in_r + tap_r * fb;
                let bbd = if noise_amt > 0.0001 {
                    self.rng.noise() * noise_amt
                } else {
                    0.0
                };
                l = in_l * (1.0 - wet * 0.45) + (tap_l + bbd) * wet;
                r = in_r * (1.0 - wet * 0.45) + (tap_r - bbd) * wet;
            }
            7 | 8 | 11 | 23 => {
                // Overdrive / Distortion / Saturator / Amp sim
                let (drive, tone, wet, level);
                if self.fx_id == 7 {
                    drive = 1.0 + p[0] * 14.0;
                    tone = p[1];
                    wet = p[2];
                    level = 0.4 + p[3] * 1.1;
                } else if self.fx_id == 8 {
                    drive = 1.5 + p[0] * 28.0;
                    tone = p[1];
                    wet = p[2];
                    level = 0.35 + p[3] * 1.1;
                } else if self.fx_id == 11 {
                    drive = 1.0 + p[0] * 7.5;
                    tone = p[2];
                    wet = p[5];
                    level = 0.85;
                } else {
                    drive = 1.2 + p[1] * 18.0;
                    tone = p[5];
                    wet = 1.0;
                    level = 0.4 + p[2] * 1.1;
                }
                let xl = in_l * drive;
                let xr = in_r * drive;
                let (sat_l, sat_r);
                if self.fx_id == 8 {
                    sat_l = (clampf(xl, -1.4, 1.4) + 0.25 * (xl * 1.8).sin()).tanh();
                    sat_r = (clampf(xr, -1.4, 1.4) + 0.25 * (xr * 1.8).sin()).tanh();
                } else {
                    sat_l = (xl + 0.18 * xl * xl).tanh();
                    sat_r = (xr + 0.18 * xr * xr).tanh();
                }
                let cutoff = 250.0 * 32.0_f32.powf(tone);
                let g = (PI_F * cutoff / sr).min(0.85);
                self.post_l += g * (sat_l - self.post_l);
                self.post_r += g * (sat_r - self.post_r);
                let gain_comp = drive.powf(0.32);
                let out_l = (self.post_l * 0.75 + sat_l * 0.25) * level / gain_comp;
                let out_r = (self.post_r * 0.75 + sat_r * 0.25) * level / gain_comp;
                l = in_l * (1.0 - wet) + out_l * wet;
                r = in_r * (1.0 - wet) + out_r * wet;
            }
            2 | 9 | 18 | 33 | 36 | 41 | 47 => {
                // Delays
                let beat_sec = 60.0 / safe_bpm;
                let div_mult = 0.0625 * 32.0_f32.powf(p[0]);
                let base_delay = clampf(beat_sec * div_mult, 0.012, 1.85);
                let wf_depth = match self.fx_id {
                    9 => p[5] * 0.0025,
                    47 => 0.004,
                    _ => p[5] * 0.0008,
                };
                self.lfo_phase = wrap01(self.lfo_phase + 2.4 / sr);
                let mod_amount = (self.lfo_phase * TWO_PI_F).sin() * wf_depth;
                let d_samples =
                    clampf((base_delay + mod_amount) * sr, 16.0, MAX_DELAY as f32 - 8.0);
                let d_samples_r = if self.fx_id == 33 || self.fx_id == 18 {
                    clampf(d_samples * 0.75, 16.0, MAX_DELAY as f32 - 8.0)
                } else {
                    d_samples
                };
                let tap_l = self.delay_l.read(d_samples);
                let tap_r = self.delay_r.read(d_samples_r);
                let hc = if self.fx_id == 9 {
                    0.48
                } else {
                    clampf(p[3], 0.15, 0.95)
                };
                let g_lp = (PI_F * (400.0 * 25.0_f32.powf(hc)) / sr).min(0.85);
                self.post_l += g_lp * (tap_l - self.post_l);
                self.post_r += g_lp * (tap_r - self.post_r);
                let fb = clampf(p[1], 0.0, 0.92);
                write_l = (in_l + self.post_r * fb).tanh();
                write_r = (in_r + self.post_l * fb).tanh();
                let wet = clampf(p[2], 0.0, 1.0);
                l = in_l + self.post_l * wet;
                r = in_r + self.post_r * wet;
            }
            10 | 37 | 44 => {
                // Reverb
                let decay = 0.70 + p[0] * 0.275;
                let wet = clampf(p[1], 0.0, 1.0);
                let damp = 0.25 + (1.0 - clampf(p[5], 0.1, 0.95)) * 0.60;
                const LENS: [f32; 4] = [1553.0, 1913.0, 2297.0, 2689.0];
                let scale = 0.6 + p[3] * 1.1;
                let mut sum_l = 0.0;
                let mut sum_r = 0.0;
                for i in 0..4 {
                    let tap_l = self.delay_l.read(LENS[i] * scale);
                    let tap_r = self.delay_r.read((LENS[i] + 173.0) * scale);
                    self.rev_comb_l[i] = tap_l * (1.0 - damp) + self.rev_comb_l[i] * damp;
                    self.rev_comb_r[i] = tap_r * (1.0 - damp) + self.rev_comb_r[i] * damp;
                    sum_l += self.rev_comb_l[i];
                    sum_r += self.rev_comb_r[i];
                }
                sum_l *= 0.25;
                sum_r *= 0.25;
                write_l = (in_l + sum_r * decay).tanh();
                write_r = (in_r + sum_l * decay).tanh();
                l = in_l * (1.0 - wet * 0.35) + sum_l * wet;
                r = in_r * (1.0 - wet * 0.35) + sum_r * wet;
            }
            12 | 35 => {
                // Resonator / Hyper-Reso
                let root_semi = (p[0] * 35.99) as i32;
                let base_midi = 36.0 + root_semi as f32 + self.env_follower * p[4] * 12.0;
                let chord_idx = clampi((p[3] * 8.99) as i32, 0, 8) as usize;
                let bright = 0.20 + p[1] * 0.75;
                let fb = 0.65 + p[2] * 0.325;
                let mut res_l = 0.0;
                let mut res_r = 0.0;
                let mono_in = 0.5 * (in_l + in_r);
                for v in 0..COMB_VOICES {
                    let note = base_midi + CHORD_INTERVALS[chord_idx][v];
                    let freq = clampf(440.0 * 2.0_f32.powf((note - 69.0) / 12.0), 35.0, 4000.0);
                    let period =
                        clampi((sr / freq).round() as i32, 8, COMB_LEN as i32 - 2) as usize;
                    let base = v * COMB_LEN;
                    let pos = self.comb_pos[v] % period;
                    let delayed = self.comb[base + pos];
                    self.comb_lp[v] += bright * (delayed - self.comb_lp[v]);
                    let next = (mono_in * 0.25 + self.comb_lp[v] * fb).tanh();
                    self.comb[base + pos] = next;
                    self.comb_pos[v] = (pos + 1) % period;
                    if v % 2 == 0 {
                        res_l += delayed;
                    } else {
                        res_r += delayed;
                    }
                }
                let spread = p[5];
                let wet_l = res_l * (0.5 + 0.5 * spread) + res_r * (0.5 - 0.5 * spread);
                let wet_r = res_r * (0.5 + 0.5 * spread) + res_l * (0.5 - 0.5 * spread);
                l = in_l * 0.6 + wet_l * 0.45;
                r = in_r * 0.6 + wet_r * 0.45;
            }
            13 | 24 | 30 | 46 => {
                // DJFX Looper / Stopper / Scatter / Back Spin
                write_l = in_l;
                write_r = in_r;
                let beat_samples = (60.0 / safe_bpm) * sr;
                let loop_div = 0.03125 * 16.0_f32.powf(1.0 - p[0]);
                let loop_len = clampf(beat_samples * loop_div, 128.0, MAX_DELAY as f32 / 2.0);
                let active = if self.fx_id == 13 {
                    p[2] >= 0.5 || p[0] > 0.05
                } else if self.fx_id == 24 || self.fx_id == 46 {
                    p[1] >= 0.5 || p[2] > 0.5
                } else {
                    true
                };

                if active {
                    if !self.looper_active {
                        self.looper_active = true;
                        self.looper_playhead = 0.0;
                    }

                    let speed = if self.fx_id == 24 {
                        self.stopper_speed =
                            (self.stopper_speed - 0.00015 * (0.2 + p[0])).max(0.02);
                        self.stopper_speed
                    } else if self.fx_id == 46 || p[3] >= 0.5 {
                        -(0.5 + p[1] * 1.5)
                    } else {
                        0.4 + p[1] * 1.2
                    };
                    self.looper_playhead = wrap(self.looper_playhead + speed, loop_len);
                    let d = loop_len - self.looper_playhead;
                    let ll = self.delay_l.read(d);
                    let rr = self.delay_r.read(d);
                    let mix = clampf(p[5], 0.2, 1.0);
                    l = in_l * (1.0 - mix) + ll * mix;
                    r = in_r * (1.0 - mix) + rr * mix;
                } else {
                    self.looper_active = false;
                    self.stopper_speed = 1.0;
                }
            }
            16 | 26 => {
                // Tremolo/Pan / Slicer
                let rate = 0.5 + p[0] * 14.0;
                self.lfo_phase = wrap01(self.lfo_phase + rate / sr);
                let depth = p[1];
                let wave = if self.fx_id == 26 || p[3] > 0.3 {
                    if wrap01(self.lfo_phase * 4.0) < (0.35 + p[4] * 0.4) {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    0.5 + 0.5 * (self.lfo_phase * TWO_PI_F).sin()
                };
                let gain_l = 1.0 - depth * (1.0 - wave);
                let gain_r = if self.fx_id == 16 && p[2] >= 0.5 {
                    1.0 - depth * wave
                } else {
                    gain_l
                };
                l = in_l * gain_l;
                r = in_r * gain_r;
            }
            17 | 34 | 38 | 40 => {
                // Wah / Super Filter / To-Gu-Ro / SBF
                let rate = 0.15 + p[4] * 9.0;
                self.lfo_phase = wrap01(self.lfo_phase + rate / sr);
                let lfo = (self.lfo_phase * TWO_PI_F).sin();
                let base_cut = 40.0 * 350.0_f32.powf(p[0]);
                let mod_oct = lfo * p[5] * 2.5 + self.env_follower * 3.5;
                let cutoff = clampf(base_cut * 2.0_f32.powf(mod_oct), 30.0, sr * 0.45);
                let reso = clampf(p[1], 0.0, 0.96);
                let mode = (p[2] * 2.99).min(2.0) as i32;
                let stages = if self.fx_id == 34 {
                    1 + (p[3] * 2.99).min(2.0) as usize
                } else {
                    2
                };
                let k = 2.0 - 1.92 * reso;
                for s in 0..stages {
                    l = self.svf_l(s, l, cutoff, k, mode);
                    r = self.svf_r(s, r, cutoff, k, mode);
                }
                l = l.tanh();
                r = r.tanh();
            }
            19 => {
                // 3-band parametric EQ
                let low_freq = 40.0 + p[3] * 300.0;
                let mid_freq = 250.0 * 16.0_f32.powf(p[4]);
                let g_low = (PI_F * low_freq / sr).min(0.45);
                let g_high = (PI_F * 3500.0 / sr).min(0.85);
                self.low_l += g_low * (in_l - self.low_l);
                self.low_r += g_low * (in_r - self.low_r);
                self.high_l += g_high * (in_l - self.high_l);
                self.high_r += g_high * (in_r - self.high_r);
                let k_mid = 2.0 - 1.6 * clampf(p[5], 0.0, 0.9);
                let mid_l = self.svf_l(0, in_l, mid_freq, k_mid, 1);
                let mid_r = self.svf_r(0, in_r, mid_freq, k_mid, 1);
                let lg = (p[0] - 0.5) * 3.2;
                let mg = (p[1] - 0.5) * 3.2;
                let hg = (p[2] - 0.5) * 3.2;
                l = (in_l + self.low_l * lg + mid_l * mg + (in_l - self.high_l) * hg).tanh();
                r = (in_r + self.low_r * lg + mid_r * mg + (in_r - self.high_r) * hg).tanh();
            }
            20 | 31 | 32 | 39 => {
                // Lo-fi / VinylSim / Cassette
                let wf_amt = p[2] * 38.0;
                self.lfo_phase = wrap01(self.lfo_phase + 1.85 / sr);
                self.lfo_phase2 = wrap01(self.lfo_phase2 + 6.7 / sr);
                let wf = ((self.lfo_phase * TWO_PI_F).sin() * 0.75
                    + (self.lfo_phase2 * TWO_PI_F).sin() * 0.25)
                    * wf_amt;
                write_l = in_l;
                write_r = in_r;
                let d = 48.0 + wf.max(-44.0);
                let wl = self.delay_l.read(d);
                let wr = self.delay_r.read(d + 3.0);
                let comp_amt = if self.fx_id == 32 {
                    1.0 + p[0] * 3.5
                } else {
                    1.0 + p[3] * 2.5
                };
                let rms = 0.5 * (wl.abs() + wr.abs());
                self.comp_env += (rms - self.comp_env) * 0.004;
                let gr = 1.0 / (1.0 + self.comp_env * comp_amt * 2.2);
                let raw = self.rng.noise();
                let crackle = if raw > 0.9965 { raw * 0.35 } else { 0.0 };
                let hiss = raw * 0.015;
                let noise_mix = (crackle + hiss) * p[1];
                let tone_ctrl = if self.fx_id == 31 {
                    p[0]
                } else {
                    p[4].max(0.25)
                };
                let cutoff = 450.0 * 25.0_f32.powf(tone_ctrl);
                let flt_l = self.svf_l(0, (wl * comp_amt * gr).tanh() + noise_mix, cutoff, 1.2, 0);
                let flt_r = self.svf_r(0, (wr * comp_amt * gr).tanh() + noise_mix, cutoff, 1.2, 0);
                let wet = clampf(p[5], 0.1, 1.0);
                l = in_l * (1.0 - wet) + flt_l * wet;
                r = in_r * (1.0 - wet) + flt_r * wet;
            }
            21 | 22 | 25 | 27 | 42 | 43 | 45 => {
                // Ring mod / pitch / sub / formant family
                if (in_l >= 0.0 && self.sub_prev_l < 0.0) || (in_l < 0.0 && self.sub_prev_l >= 0.0)
                {
                    self.sub_sign_l = -self.sub_sign_l;
                }
                self.sub_prev_l = in_l;
                let freq = 45.0 * 40.0_f32.powf(p[0]);
                self.lfo_phase = wrap01(self.lfo_phase + freq / sr);
                let carrier = (self.lfo_phase * TWO_PI_F).sin();
                let (mod_l, mod_r);
                if self.fx_id == 21 {
                    mod_l = in_l * carrier;
                    mod_r = in_r * carrier;
                } else if self.fx_id == 45 {
                    let sub = self.sub_sign_l * self.env_follower * 1.8;
                    let g = (PI_F * 140.0 / sr).min(0.25);
                    self.sub_lp += g * (sub - self.sub_lp);
                    mod_l = in_l + self.sub_lp * p[1] * 2.0;
                    mod_r = in_r + self.sub_lp * p[1] * 2.0;
                } else {
                    write_l = in_l;
                    write_r = in_r;
                    let ratio = 2.0_f32.powf((p[0] - 0.5) * 2.0);
                    let rate = (1.0 - ratio) / 1024.0;
                    self.stopper_phase = wrap01(self.stopper_phase + rate);
                    let ph1 = self.stopper_phase;
                    let ph2 = wrap01(ph1 + 0.5);
                    let w1 = (ph1 * PI_F).sin();
                    let w2 = (ph2 * PI_F).sin();
                    let g1 = w1 * w1;
                    let g2 = w2 * w2;
                    mod_l = self.delay_l.read(24.0 + ph1 * 1024.0) * g1
                        + self.delay_l.read(24.0 + ph2 * 1024.0) * g2;
                    mod_r = self.delay_r.read(24.0 + ph1 * 1024.0) * g1
                        + self.delay_r.read(24.0 + ph2 * 1024.0) * g2;
                }
                let wet = clampf(p[2], 0.1, 1.0);
                l = in_l * (1.0 - wet) + mod_l.tanh() * wet;
                r = in_r * (1.0 - wet) + mod_r.tanh() * wet;
            }
            _ => {
                // Compressor / 303 BusComp and anything unmapped
                let sustain = 1.0 + p[0] * 6.5;
                let att = 0.001 + (1.0 - p[1]) * 0.05;
                let lvl = 0.5 + p[2] * 1.5;
                let peak = in_l.abs().max(in_r.abs());
                let coeff = if peak > self.comp_env { att } else { 0.0008 };
                self.comp_env += (peak - self.comp_env) * coeff;
                let gain = (sustain / (1.0 + self.comp_env * sustain * 2.4)) * lvl;
                l = (in_l * gain).tanh();
                r = (in_r * gain).tanh();
            }
        }

        self.delay_l.write(write_l);
        self.delay_r.write(write_r);
        *out_l = sanitize(l);
        *out_r = sanitize(r);
    }

    /// True when this slot is the bypass/trim entry.
    pub fn is_bypass(&self) -> bool {
        self.fx_id == FX_ID_BYPASS
    }
}
