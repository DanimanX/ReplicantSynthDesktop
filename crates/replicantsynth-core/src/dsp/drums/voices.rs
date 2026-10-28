//! The eleven 909 circuit models: bridged-T bass drum, dual-mode snare,
//! pitch-swept toms, modal rim shot, burst hand clap and ROM cymbals.

use super::rom::{decode_6bit, rom_data, RomData, ROM_SAMPLE_RATE};
use super::{diode_waveshaper, fast_tanh, AnalogSvf, DrumVoice, FastNoise, VoiceParams, TWO_PI_F};

pub(crate) struct BassDrum {
    params: VoiceParams,
    active: bool,
    env_time: f32,
    phase: f32,
    velocity: f32,
    accent: f32,
    rng: FastNoise,
    click_filter: AnalogSvf,
    sample_rate: f64,
}

impl BassDrum {
    pub fn new() -> Self {
        Self {
            params: VoiceParams::default(),
            active: false,
            env_time: 0.0,
            phase: 0.0,
            velocity: 1.0,
            accent: 0.0,
            rng: FastNoise::new(0xbd90_9bd1),
            click_filter: AnalogSvf::default(),
            sample_rate: 48_000.0,
        }
    }
}

impl DrumVoice for BassDrum {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.reset();
    }

    fn reset(&mut self) {
        self.active = false;
        self.env_time = 0.0;
        self.phase = 0.0;
        self.click_filter.reset();
    }

    fn trigger(&mut self, velocity: f32, accent: f32) {
        self.velocity = velocity.clamp(0.05, 1.0);
        self.accent = accent.clamp(0.0, 1.0);
        self.env_time = 0.0;
        self.phase = 0.0;
        self.click_filter.reset();
        self.active = true;
    }

    fn render_sample(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let dt = 1.0 / self.sample_rate as f32;

        let tune_ratio = 2.0_f32.powf((self.params.tune - 0.5) * 1.4);
        let base_freq = 52.0 * tune_ratio;

        let chirp_depth = 1.0 + 0.45 * self.accent;
        let fast_env = (-self.env_time * 110.0).exp();
        let med_env = (-self.env_time * 22.0).exp();
        let inst_freq = base_freq + (230.0 * fast_env * chirp_depth + 65.0 * med_env) * tune_ratio;

        self.phase += inst_freq * dt;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }

        let tri = if self.phase < 0.5 {
            4.0 * self.phase - 1.0
        } else {
            3.0 - 4.0 * self.phase
        };

        let body = diode_waveshaper(tri);

        let decay_scale = 0.18 + self.params.decay * 1.95;
        let amp_env = (-self.env_time * (2.8 / decay_scale)).exp();
        if amp_env < 0.0003 {
            self.active = false;
            return 0.0;
        }

        let click_env = (-self.env_time * 380.0).exp();
        let raw_noise = self.rng.next_sample();
        self.click_filter
            .process(raw_noise, 2800.0 * tune_ratio, 1.8, self.sample_rate as f32);
        let click_noise = self.click_filter.bp * (-self.env_time * 520.0).exp();

        let click_amp = (0.15 + self.params.attack * 1.35) * (1.0 + 0.5 * self.accent);
        let click = (click_env * 0.95 + click_noise * 0.55) * click_amp;

        let accent_boost = 1.0 + self.accent * 0.45;
        let raw_out = (body * amp_env + click) * self.velocity * accent_boost * self.params.level;

        let out = fast_tanh(raw_out);
        self.env_time += dt;
        out
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_params(&mut self, params: VoiceParams) {
        self.params = params;
    }
}

pub(crate) struct SnareDrum {
    params: VoiceParams,
    active: bool,
    env_time: f32,
    phase1: f32,
    phase2: f32,
    velocity: f32,
    accent: f32,
    rng: FastNoise,
    snappy_filter: AnalogSvf,
    sample_rate: f64,
}

impl SnareDrum {
    pub fn new() -> Self {
        Self {
            params: VoiceParams::default(),
            active: false,
            env_time: 0.0,
            phase1: 0.0,
            phase2: 0.0,
            velocity: 1.0,
            accent: 0.0,
            rng: FastNoise::new(0x5d90_95d1),
            snappy_filter: AnalogSvf::default(),
            sample_rate: 48_000.0,
        }
    }
}

impl DrumVoice for SnareDrum {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.reset();
    }

    fn reset(&mut self) {
        self.active = false;
        self.env_time = 0.0;
        self.phase1 = 0.0;
        self.phase2 = 0.0;
        self.snappy_filter.reset();
    }

    fn trigger(&mut self, velocity: f32, accent: f32) {
        self.velocity = velocity.clamp(0.05, 1.0);
        self.accent = accent.clamp(0.0, 1.0);
        self.env_time = 0.0;
        self.phase1 = 0.0;
        self.phase2 = 0.0;
        self.snappy_filter.reset();
        self.active = true;
    }

    fn render_sample(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let dt = 1.0 / self.sample_rate as f32;

        let tune_ratio = 2.0_f32.powf((self.params.tune - 0.5) * 1.5);
        let sweep = 1.0 + 0.60 * (-self.env_time * 90.0).exp();
        let f1 = 185.0 * tune_ratio * sweep;
        let f2 = 340.0 * tune_ratio * sweep;

        self.phase1 += f1 * dt;
        if self.phase1 >= 1.0 {
            self.phase1 -= self.phase1.floor();
        }
        self.phase2 += f2 * dt;
        if self.phase2 >= 1.0 {
            self.phase2 -= self.phase2.floor();
        }

        let body_env = (-self.env_time * 17.5).exp();
        let shell = ((self.phase1 * TWO_PI_F).sin() * 0.65 + (self.phase2 * TWO_PI_F).sin() * 0.35)
            * body_env;

        let snappy_decay_scale = 0.35 + self.params.snappy * 1.65;
        let snap_env = 0.65 * (-self.env_time * (24.0 / snappy_decay_scale)).exp()
            + 0.35 * (-self.env_time * (8.0 / snappy_decay_scale)).exp();
        if body_env < 0.0003 && snap_env < 0.0003 {
            self.active = false;
            return 0.0;
        }

        let noise_cutoff = 1600.0 + self.params.tone * 2200.0;
        let raw_noise = self.rng.next_sample();
        self.snappy_filter
            .process(raw_noise, noise_cutoff, 1.5, self.sample_rate as f32);
        let filtered_noise = self.snappy_filter.hp * 0.65 + self.snappy_filter.bp * 0.55;

        let body_gain = 1.15 - self.params.tone * 0.35;
        let snappy_gain = (0.20 + self.params.snappy * 1.45) * (0.6 + self.params.tone * 0.8);

        let accent_boost = 1.0 + self.accent * 0.5;
        let mixed = (shell * body_gain + filtered_noise * snap_env * snappy_gain)
            * self.velocity
            * accent_boost
            * self.params.level
            * 0.95;

        let out = fast_tanh(mixed);
        self.env_time += dt;
        out
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_params(&mut self, params: VoiceParams) {
        self.params = params;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TomType {
    Low = 0,
    Mid = 1,
    Hi = 2,
}

pub(crate) struct Tom {
    params: VoiceParams,
    tom_type: TomType,
    active: bool,
    env_time: f32,
    phase: f32,
    velocity: f32,
    accent: f32,
    rng: FastNoise,
    sample_rate: f64,
}

impl Tom {
    pub fn new(tom_type: TomType) -> Self {
        Self {
            params: VoiceParams::default(),
            tom_type,
            active: false,
            env_time: 0.0,
            phase: 0.0,
            velocity: 1.0,
            accent: 0.0,
            rng: FastNoise::new(0x7090_9000 + tom_type as u32),
            sample_rate: 48_000.0,
        }
    }
}

impl DrumVoice for Tom {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.reset();
    }

    fn reset(&mut self) {
        self.active = false;
        self.env_time = 0.0;
        self.phase = 0.0;
    }

    fn trigger(&mut self, velocity: f32, accent: f32) {
        self.velocity = velocity.clamp(0.05, 1.0);
        self.accent = accent.clamp(0.0, 1.0);
        self.env_time = 0.0;
        self.phase = 0.0;
        self.active = true;
    }

    fn render_sample(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let dt = 1.0 / self.sample_rate as f32;

        let nominal = match self.tom_type {
            TomType::Low => 92.0,
            TomType::Mid => 132.0,
            TomType::Hi => 185.0,
        };
        let tune_ratio = 2.0_f32.powf((self.params.tune - 0.5) * 1.4);
        let base_freq = nominal * tune_ratio;
        let pitch_sweep = 1.0 + (0.80 + 0.35 * self.accent) * (-self.env_time * 30.0).exp();
        let inst_freq = base_freq * pitch_sweep;

        self.phase += inst_freq * dt;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }

        let decay_scale = 0.25 + self.params.decay * 2.1;
        let amp_env = (-self.env_time * (6.0 / decay_scale)).exp();
        if amp_env < 0.0003 {
            self.active = false;
            return 0.0;
        }

        let body = (self.phase * TWO_PI_F).sin() + 0.08 * (self.phase * TWO_PI_F * 2.0).sin();
        let click = self.rng.next_sample() * (-self.env_time * 175.0).exp() * 0.28;

        let accent_boost = 1.0 + self.accent * 0.45;
        let raw = (body * amp_env + click) * self.velocity * accent_boost * self.params.level;

        let out = fast_tanh(raw);
        self.env_time += dt;
        out
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_params(&mut self, params: VoiceParams) {
        self.params = params;
    }
}

pub(crate) struct RimShot {
    params: VoiceParams,
    active: bool,
    env_time: f32,
    phase1: f32,
    phase2: f32,
    phase3: f32,
    velocity: f32,
    accent: f32,
    sample_rate: f64,
}

impl RimShot {
    pub fn new() -> Self {
        Self {
            params: VoiceParams::default(),
            active: false,
            env_time: 0.0,
            phase1: 0.0,
            phase2: 0.0,
            phase3: 0.0,
            velocity: 1.0,
            accent: 0.0,
            sample_rate: 48_000.0,
        }
    }
}

impl DrumVoice for RimShot {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.reset();
    }

    fn reset(&mut self) {
        self.active = false;
        self.env_time = 0.0;
        self.phase1 = 0.0;
        self.phase2 = 0.0;
        self.phase3 = 0.0;
    }

    fn trigger(&mut self, velocity: f32, accent: f32) {
        self.velocity = velocity.clamp(0.05, 1.0);
        self.accent = accent.clamp(0.0, 1.0);
        self.env_time = 0.0;
        self.phase1 = 0.0;
        self.phase2 = 0.0;
        self.phase3 = 0.0;
        self.active = true;
    }

    fn render_sample(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let dt = 1.0 / self.sample_rate as f32;

        self.phase1 += 250.0 * dt;
        if self.phase1 >= 1.0 {
            self.phase1 -= self.phase1.floor();
        }
        self.phase2 += 495.0 * dt;
        if self.phase2 >= 1.0 {
            self.phase2 -= self.phase2.floor();
        }
        self.phase3 += 1580.0 * dt;
        if self.phase3 >= 1.0 {
            self.phase3 -= self.phase3.floor();
        }

        let env = (-self.env_time * 65.0).exp();
        if env < 0.0003 {
            self.active = false;
            return 0.0;
        }

        let modal_mix = (self.phase1 * TWO_PI_F).sin() * 0.40
            + (self.phase2 * TWO_PI_F).sin() * 0.60
            + (self.phase3 * TWO_PI_F).sin() * 0.90;

        let accent_boost = 1.0 + self.accent * 0.50;
        let raw = modal_mix * env * self.velocity * accent_boost * self.params.level;

        let out = fast_tanh(raw);
        self.env_time += dt;
        out
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_params(&mut self, params: VoiceParams) {
        self.params = params;
    }
}

pub(crate) struct HandClap {
    params: VoiceParams,
    active: bool,
    env_time: f32,
    reverb: f32,
    velocity: f32,
    accent: f32,
    rng: FastNoise,
    clap_filter: AnalogSvf,
    sample_rate: f64,
}

impl HandClap {
    pub fn new() -> Self {
        Self {
            params: VoiceParams::default(),
            active: false,
            env_time: 0.0,
            reverb: 0.5,
            velocity: 1.0,
            accent: 0.0,
            rng: FastNoise::new(0xc190_9a11),
            clap_filter: AnalogSvf::default(),
            sample_rate: 48_000.0,
        }
    }
}

impl DrumVoice for HandClap {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.reset();
    }

    fn reset(&mut self) {
        self.active = false;
        self.env_time = 0.0;
        self.clap_filter.reset();
    }

    fn trigger(&mut self, velocity: f32, accent: f32) {
        self.velocity = velocity.clamp(0.05, 1.0);
        self.accent = accent.clamp(0.0, 1.0);
        self.env_time = 0.0;
        self.clap_filter.reset();
        self.active = true;
    }

    fn render_sample(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let dt = 1.0 / self.sample_rate as f32;

        let decay_scale = 0.25 + self.reverb * 2.2;
        let amp_env = if self.env_time < 0.030 {
            let burst_phase = (self.env_time % 0.010) / 0.010;
            (-burst_phase * 4.5).exp()
        } else {
            let tail_time = self.env_time - 0.030;
            0.75 * (-tail_time * (7.0 / decay_scale)).exp()
        };

        if self.env_time > 0.035 && amp_env < 0.0003 {
            self.active = false;
            return 0.0;
        }

        let raw_noise = self.rng.next_sample();
        self.clap_filter
            .process(raw_noise, 1180.0, 1.8, self.sample_rate as f32);
        let bandpassed = self.clap_filter.bp;

        let accent_boost = 1.0 + self.accent * 0.45;
        let raw = bandpassed * amp_env * self.velocity * accent_boost * self.params.level;

        let out = fast_tanh(raw);
        self.env_time += dt;
        out
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_params(&mut self, params: VoiceParams) {
        self.params = params;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CymbalType {
    ClosedHat = 0,
    OpenHat = 1,
    Crash = 2,
    Ride = 3,
}

pub(crate) struct Cymbal {
    params: VoiceParams,
    cymbal_type: CymbalType,
    active: bool,
    playhead: f64,
    env_time: f32,
    velocity: f32,
    accent: f32,
    choking: bool,
    choke_gain: f32,
    recon_filter: AnalogSvf,
    sample_rate: f64,
}

impl Cymbal {
    pub fn new(cymbal_type: CymbalType) -> Self {
        Self {
            params: VoiceParams::default(),
            cymbal_type,
            active: false,
            playhead: 0.0,
            env_time: 0.0,
            velocity: 1.0,
            accent: 0.0,
            choking: false,
            choke_gain: 1.0,
            recon_filter: AnalogSvf::default(),
            sample_rate: 48_000.0,
        }
    }

    fn buffer<'a>(&self, rom: &'a RomData) -> &'a [i8] {
        match self.cymbal_type {
            CymbalType::ClosedHat => &rom.closed_hat,
            CymbalType::OpenHat => &rom.open_hat,
            CymbalType::Crash => &rom.crash,
            CymbalType::Ride => &rom.ride,
        }
    }
}

impl DrumVoice for Cymbal {
    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.reset();
    }

    fn reset(&mut self) {
        self.active = false;
        self.playhead = 0.0;
        self.env_time = 0.0;
        self.choking = false;
        self.choke_gain = 1.0;
        self.recon_filter.reset();
    }

    fn trigger(&mut self, velocity: f32, accent: f32) {
        self.velocity = velocity.clamp(0.05, 1.0);
        self.accent = accent.clamp(0.0, 1.0);
        self.playhead = 0.0;
        self.env_time = 0.0;
        self.choking = false;
        self.choke_gain = 1.0;
        self.recon_filter.reset();
        self.active = true;
    }

    fn choke(&mut self) {
        if self.active {
            self.choking = true;
        }
    }

    fn render_sample(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let rom = rom_data();
        let pcm = self.buffer(rom);
        if pcm.is_empty() {
            self.active = false;
            return 0.0;
        }
        let pcm_len = pcm.len();

        let tune_ratio = match self.cymbal_type {
            CymbalType::Crash | CymbalType::Ride => 2.0_f32.powf((self.params.tune - 0.5) * 1.6),
            _ => 2.0_f32.powf((self.params.tune - 0.5) * 0.35),
        };

        let sample_pitch_step = (ROM_SAMPLE_RATE / self.sample_rate) * tune_ratio as f64;

        let idx0 = self.playhead as usize;
        if idx0 + 1 >= pcm_len {
            self.active = false;
            return 0.0;
        }

        let frac = (self.playhead - idx0 as f64) as f32;
        let s0 = decode_6bit(pcm[idx0]);
        let s1 = decode_6bit(pcm[idx0 + 1]);
        let raw_pcm = s0 + frac * (s1 - s0);

        self.playhead += sample_pitch_step;

        let decay_scale = 0.22 + self.params.decay * 2.2;
        let decay_rate = match self.cymbal_type {
            CymbalType::ClosedHat => 26.0 / decay_scale,
            CymbalType::OpenHat => 3.6 / decay_scale,
            CymbalType::Crash => 1.35 / decay_scale,
            CymbalType::Ride => 1.15 / decay_scale,
        };

        let dt = 1.0 / self.sample_rate as f32;
        let amp_env = (-self.env_time * decay_rate).exp();
        if amp_env < 0.0003 {
            self.active = false;
            return 0.0;
        }

        if self.choking {
            self.choke_gain *= 0.965;
            if self.choke_gain < 0.001 {
                self.active = false;
                self.choking = false;
                return 0.0;
            }
        }

        let filter_freq = match self.cymbal_type {
            CymbalType::Ride => 5200.0,
            CymbalType::Crash => 4800.0,
            _ => 6500.0,
        };

        self.recon_filter.process(
            raw_pcm,
            filter_freq * tune_ratio,
            1.4,
            self.sample_rate as f32,
        );
        let filtered = self.recon_filter.hp * 0.75 + self.recon_filter.bp * 0.45;

        let accent_boost = 1.0 + self.accent * 0.45;
        let shaped = filtered
            * amp_env
            * self.choke_gain
            * self.velocity
            * accent_boost
            * self.params.level
            * 1.10;

        let out = fast_tanh(shaped);
        self.env_time += dt;
        out
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_params(&mut self, params: VoiceParams) {
        self.params = params;
    }
}
