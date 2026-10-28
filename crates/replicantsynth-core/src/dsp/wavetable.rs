//! ReplicantWave engine: 8 band-limited wavetable banks with 16-frame morphing,
//! up to 8 unison voices per voice, a ZDF state-variable filter, sub, noise,
//! ADSR and chord mode.

use crate::dsp::{
    clampf, clampi, fast_tanh, lerpf, midi_to_freq, sanitize, wrap01, Adsr, Rng, ZdfSvf, PI_F,
    TWO_PI_F,
};

use std::sync::OnceLock;

pub const BANK_COUNT: usize = 8;
pub const FRAME_COUNT: usize = 16;
pub const MIP_COUNT: usize = 7;
pub const TABLE_SIZE: usize = 1024;
pub const MAX_HARMONICS: usize = 192;
pub const VOICE_COUNT: usize = 8;
pub const UNISON_MAX: usize = 8;

const BANK_FREQ_BASE: f32 = 30.0;
const TARGET_PEAK: f32 = 0.90;

pub const BANK_NAMES: [&str; BANK_COUNT] = [
    "ANALOG MORPH",
    "CYBER NEURO",
    "DIGITAL FM",
    "VOCALOID",
    "CYBER ACID",
    "DYSTOPIAN STEEL",
    "PWM HYPER",
    "SUB ZERO",
];

#[inline]
fn gaussian(x: f32, centre: f32, width: f32) -> f32 {
    let d = (x - centre) / width;
    (-0.5 * d * d).exp()
}

fn bessel_j(n: i32, x: f64) -> f64 {
    if n < 0 {
        return 0.0;
    }
    // J_n(x) series; the FM index keeps terms well inside f64 range.
    let mut term = (x * 0.5).powi(n) / factorial(n as u32);
    let mut sum = term;
    for k in 1..32 {
        term *= -(x * x * 0.25) / (k as f64 * (n as f64 + k as f64));
        sum += term;
        if term.abs() < 1.0e-10 {
            break;
        }
    }
    sum
}

fn factorial(n: u32) -> f64 {
    let mut acc = 1.0f64;
    for i in 2..=n {
        acc *= i as f64;
    }
    acc
}

fn mip_harmonics(mip: usize, sample_rate: f32) -> usize {
    let fundamental = BANK_FREQ_BASE * 2.0_f32.powi(mip as i32);
    let limit = (sample_rate * 0.46 / fundamental) as i32;
    clampi(limit, 1, MAX_HARMONICS as i32 - 1) as usize
}

fn sine_table() -> &'static [f32; 4097] {
    static TABLE: OnceLock<[f32; 4097]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0; 4097];
        for (i, slot) in table.iter_mut().enumerate() {
            *slot = (TWO_PI_F * i as f32 / 4096.0).sin();
        }
        table
    })
}

#[inline]
fn sine(phase: f32) -> f32 {
    let table = sine_table();
    let p = wrap01(phase) * 4096.0;
    let i0 = (p as usize) % 4096;
    let frac = p - p.floor();
    table[i0] * (1.0 - frac) + table[i0 + 1] * frac
}

/// Fill the harmonic amplitude/phase tables for one bank frame.
#[allow(clippy::needless_range_loop)]
fn bank_harmonics(bank: usize, t: f32, amp: &mut [f32], phase: &mut [f32]) {
    amp.fill(0.0);
    phase.fill(0.0);
    let count = amp.len().min(MAX_HARMONICS);

    match bank {
        0 => {
            // Analog Morph: saw -> triangle -> sine -> square -> pulse
            let pos = clampf(t, 0.0, 1.0) * 4.0;
            let segment = clampi(pos as i32, 0, 3) as usize;
            let blend = pos - segment as f32;
            let pulse_width = 0.5 - 0.36 * t;
            for n in 1..count {
                let fnn = n as f32;
                let saw = 1.0 / fnn;
                let tri = if n % 2 == 1 {
                    let sign = if ((n - 1) / 2) % 2 == 0 { 1.0 } else { -1.0 };
                    sign / (fnn * fnn)
                } else {
                    0.0
                };
                let sine_h = if n == 1 { 1.0 } else { 0.0 };
                let square = if n % 2 == 1 { 1.0 / fnn } else { 0.0 };
                let pulse = (2.0 / (fnn * PI_F)) * (PI_F * fnn * pulse_width).sin();
                let (a, b) = match segment {
                    0 => (saw, tri),
                    1 => (tri, sine_h),
                    2 => (sine_h, square),
                    _ => (square, pulse),
                };
                amp[n] = lerpf(a, b, blend);
            }
        }
        1 => {
            // Cyber Neuro: formant-swept folded saw
            let peak = 3.0 + 22.0 * t;
            let width = 2.0 + 7.0 * t;
            for n in 1..count {
                let fnn = n as f32;
                let base = 1.0 / fnn;
                let growl = gaussian(fnn, peak, width) * (2.2 + 4.0 * t);
                amp[n] = base * (0.22 + growl);
                phase[n] = if n % 3 == 0 { PI_F * 0.5 } else { 0.0 };
            }
        }
        2 => {
            // Digital FM: 2-op carrier/modulator with a 2:1 ratio
            let index = 8.0 * t;
            for n in 1..count {
                let j = if n % 2 == 1 {
                    bessel_j(((n - 1) / 2) as i32, index as f64)
                } else {
                    0.0
                };
                amp[n] = j.abs() as f32 * (0.35 + 0.65 * (1.0 / (1.0 + 0.02 * n as f32)));
                phase[n] = if j < 0.0 { PI_F } else { 0.0 };
            }
            if t < 0.05 {
                amp[1] = 1.0;
            }
        }
        3 => {
            // Vocaloid: A-E-I-O-U formant morph
            const FORMANTS: [[f32; 3]; 5] = [
                [800.0, 1150.0, 2900.0],
                [400.0, 1600.0, 2700.0],
                [350.0, 1700.0, 2700.0],
                [450.0, 800.0, 2830.0],
                [325.0, 700.0, 2530.0],
            ];
            let pos = clampf(t, 0.0, 1.0) * 4.0;
            let v0 = clampi(pos as i32, 0, 4) as usize;
            let v1 = clampi(v0 as i32 + 1, 0, 4) as usize;
            let blend = pos - v0 as f32;
            let f0 = 110.0;
            for n in 1..count {
                let freq = n as f32 * f0;
                let mut a = 0.0;
                for k in 0..3 {
                    let c0 = FORMANTS[v0][k] / f0;
                    let c1 = FORMANTS[v1][k] / f0;
                    let c = lerpf(c0, c1, blend);
                    a += gaussian(freq, c, 1.6 + 1.2 * k as f32);
                }
                amp[n] = (a + 0.08) / (n as f32).powf(0.9);
            }
        }
        4 => {
            // Cyber Acid: resonant harmonic squelch
            let peak = 3.0 + 16.0 * t;
            for n in 1..count {
                let fnn = n as f32;
                let resonance = gaussian(fnn, peak, 1.1) * (6.0 + 8.0 * t);
                amp[n] = (1.0 / fnn) * (0.35 + resonance);
                phase[n] = if n % 2 == 0 { PI_F } else { 0.0 };
            }
        }
        5 => {
            // Dystopian Steel: phase-randomised metallic partials
            let mut rng = Rng::new(0x5EED_0000u32.wrapping_add((t * 1000.0) as u32));
            for n in 1..count {
                let fnn = n as f32;
                let metallic = n % 7 == 0 || n % 11 == 0 || n < 9;
                let weight = if metallic { 1.0 } else { 0.22 };
                amp[n] = weight / fnn.powf(0.72);
                phase[n] = rng.uniform() * TWO_PI_F;
            }
        }
        6 => {
            // PWM Hyper: 50% -> 4% pulse width
            let width = 0.5 - 0.46 * t;
            for n in 1..count {
                let fnn = n as f32;
                amp[n] = (2.0 / (fnn * PI_F)) * (PI_F * fnn * width).sin();
            }
        }
        _ => {
            // Sub Zero: saturated sub with wavefolded odd harmonics
            for n in 1..count {
                let fnn = n as f32;
                if n == 1 {
                    amp[n] = 1.0;
                } else if n % 2 == 0 {
                    amp[n] = 0.28 * t / fnn;
                } else {
                    amp[n] = (0.55 * t + 0.12) / fnn.powf(0.8);
                }
            }
        }
    }
}

/// The shared read-only bank set: built once on first use.
pub struct WavetableBanks {
    data: Vec<f32>,
}

impl WavetableBanks {
    #[allow(clippy::needless_range_loop)]
    fn build_bank(bank: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; FRAME_COUNT * MIP_COUNT * TABLE_SIZE];
        let mut amp = vec![0.0f32; MAX_HARMONICS];
        let mut phase = vec![0.0f32; MAX_HARMONICS];
        let mut frame = vec![0.0f32; MIP_COUNT * TABLE_SIZE];
        let mut peaks = [0.0f32; MIP_COUNT];

        for f in 0..FRAME_COUNT {
            let t = f as f32 / (FRAME_COUNT - 1) as f32;
            bank_harmonics(bank, t, &mut amp, &mut phase);

            for mip in 0..MIP_COUNT {
                let harmonics = mip_harmonics(mip, 48_000.0);
                let dst = &mut frame[mip * TABLE_SIZE..(mip + 1) * TABLE_SIZE];
                let mut peak = 0.0f32;
                for (i, slot) in dst.iter_mut().enumerate() {
                    let p = i as f32 / TABLE_SIZE as f32;
                    let mut sample = 0.0f32;
                    for n in 1..=harmonics {
                        let a = amp[n];
                        if a.abs() < 1.0e-6 {
                            continue;
                        }
                        sample += a * sine(p * n as f32 + phase[n] / TWO_PI_F);
                    }
                    *slot = sample;
                    peak = peak.max(sample.abs());
                }
                peaks[mip] = peak;
            }

            let frame_peak = peaks.iter().cloned().fold(0.0f32, f32::max).max(1.0e-6);
            let scale = TARGET_PEAK / frame_peak;

            for mip in 0..MIP_COUNT {
                let src = &frame[mip * TABLE_SIZE..(mip + 1) * TABLE_SIZE];
                let base = (f * MIP_COUNT + mip) * TABLE_SIZE;
                for i in 0..TABLE_SIZE {
                    out[base + i] = src[i] * scale;
                }
            }
        }
        out
    }

    pub fn new() -> Self {
        let banks: Vec<Vec<f32>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..BANK_COUNT)
                .map(|bank| scope.spawn(move || Self::build_bank(bank)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("wavetable bank build"))
                .collect()
        });

        let mut data = Vec::with_capacity(BANK_COUNT * FRAME_COUNT * MIP_COUNT * TABLE_SIZE);
        for bank in banks {
            data.extend_from_slice(&bank);
        }
        Self { data }
    }

    pub fn ready(&self) -> bool {
        !self.data.is_empty()
    }

    /// Linear frame morph and linear phase interpolation.
    #[inline]
    #[allow(clippy::needless_range_loop)]
    pub fn sample(&self, bank: usize, frame_position: f32, mip: usize, phase: f32) -> f32 {
        if self.data.is_empty() {
            return 0.0;
        }
        let b = clampi(bank as i32, 0, BANK_COUNT as i32 - 1) as usize;
        let m = clampi(mip as i32, 0, MIP_COUNT as i32 - 1) as usize;
        let fp = clampf(frame_position, 0.0, 1.0) * (FRAME_COUNT - 1) as f32;
        let f0 = clampi(fp as i32, 0, FRAME_COUNT as i32 - 1) as usize;
        let f1 = (f0 + 1).min(FRAME_COUNT - 1);
        let f_frac = fp - f0 as f32;

        let p = wrap01(phase) * TABLE_SIZE as f32;
        let i0 = (p as usize) % TABLE_SIZE;
        let frac = p - p.floor();

        let read = |frame_index: usize| {
            let base = ((b * FRAME_COUNT + frame_index) * MIP_COUNT + m) * TABLE_SIZE;
            self.data[base + i0] * (1.0 - frac) + self.data[base + (i0 + 1) % TABLE_SIZE] * frac
        };

        lerpf(read(f0), read(f1), f_frac)
    }

    pub fn bank_name(bank: usize) -> &'static str {
        BANK_NAMES[clampi(bank as i32, 0, BANK_COUNT as i32 - 1) as usize]
    }
}

impl Default for WavetableBanks {
    fn default() -> Self {
        Self::new()
    }
}

/// One shared bank set for every clip: built once, read on the audio thread.
pub fn shared_wavetable_banks() -> &'static WavetableBanks {
    static BANKS: OnceLock<WavetableBanks> = OnceLock::new();
    BANKS.get_or_init(WavetableBanks::new)
}

#[derive(Clone, Copy, Debug)]
pub struct WaveParams {
    pub bank: i32,
    pub position: f32,
    pub unison: i32,
    pub detune: f32,
    pub spread: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub drive: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub sub: f32,
    pub noise: f32,
    pub level: f32,
    pub filter_mode: i32,
    pub chord: i32,
}

impl Default for WaveParams {
    fn default() -> Self {
        Self {
            bank: 0,
            position: 0.25,
            unison: 1,
            detune: 12.0,
            spread: 0.4,
            cutoff: 2400.0,
            resonance: 0.35,
            drive: 1.0,
            attack: 0.005,
            decay: 0.35,
            sustain: 0.55,
            release: 0.45,
            sub: 0.15,
            noise: 0.0,
            level: 0.8,
            filter_mode: 0,
            chord: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct WaveVoice {
    active: bool,
    held: bool,
    note: i32,
    root_note: i32,
    velocity: f32,
    age: u32,
    phase: [f32; UNISON_MAX],
    inc: [f32; UNISON_MAX],
    pan: [f32; UNISON_MAX],
    gain: [f32; UNISON_MAX],
    sub_phase: f32,
    sub_inc: f32,
    env: Adsr,
    filter_l: ZdfSvf,
    filter_r: ZdfSvf,
    filter2_l: ZdfSvf,
    filter2_r: ZdfSvf,
}

impl Default for WaveVoice {
    fn default() -> Self {
        Self {
            active: false,
            held: false,
            note: 60,
            root_note: 60,
            velocity: 0.8,
            age: 0,
            phase: [0.0; UNISON_MAX],
            inc: [0.0; UNISON_MAX],
            pan: [0.0; UNISON_MAX],
            gain: [0.0; UNISON_MAX],
            sub_phase: 0.0,
            sub_inc: 0.0,
            env: Adsr::default(),
            filter_l: ZdfSvf::default(),
            filter_r: ZdfSvf::default(),
            filter2_l: ZdfSvf::default(),
            filter2_r: ZdfSvf::default(),
        }
    }
}

/// The eight-voice wavetable synth owned by every `ClipKind::Wave` pad.
pub struct WaveSynth {
    sample_rate: f64,
    sample_rate_f: f32,
    params: WaveParams,
    voices: [WaveVoice; VOICE_COUNT],
    age_counter: u32,
    rng: Rng,
    smoothed_cutoff: f32,
    smoothed_resonance: f32,
    smoothed_drive: f32,
}

impl Default for WaveSynth {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveSynth {
    pub fn new() -> Self {
        let mut synth = Self {
            sample_rate: 48_000.0,
            sample_rate_f: 48_000.0,
            params: WaveParams::default(),
            voices: std::array::from_fn(|_| WaveVoice::default()),
            age_counter: 0,
            rng: Rng::new(0x51A7_E001),
            smoothed_cutoff: 2400.0,
            smoothed_resonance: 0.35,
            smoothed_drive: 1.0,
        };
        synth.prepare(48_000.0);
        synth
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.sample_rate_f = self.sample_rate as f32;
        for voice in self.voices.iter_mut() {
            voice.env.prepare(self.sample_rate_f);
            voice.filter_l.prepare(self.sample_rate_f);
            voice.filter_r.prepare(self.sample_rate_f);
            voice.filter2_l.prepare(self.sample_rate_f);
            voice.filter2_r.prepare(self.sample_rate_f);
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        for voice in self.voices.iter_mut() {
            *voice = WaveVoice::default();
            voice.env.prepare(self.sample_rate_f);
            voice.filter_l.prepare(self.sample_rate_f);
            voice.filter_r.prepare(self.sample_rate_f);
            voice.filter2_l.prepare(self.sample_rate_f);
            voice.filter2_r.prepare(self.sample_rate_f);
        }
        self.age_counter = 0;
        self.smoothed_cutoff = self.params.cutoff;
        self.smoothed_resonance = self.params.resonance;
        self.smoothed_drive = self.params.drive;
    }

    pub fn set_params(&mut self, params: WaveParams) {
        self.params = params;
        self.params.bank = clampi(self.params.bank, 0, BANK_COUNT as i32 - 1);
        self.params.unison = clampi(self.params.unison, 1, UNISON_MAX as i32);
        self.params.position = clampf(self.params.position, 0.0, 1.0);
        self.params.detune = clampf(self.params.detune, 0.0, 80.0);
        self.params.spread = clampf(self.params.spread, 0.0, 1.0);
        self.params.cutoff = clampf(self.params.cutoff, 40.0, 19_000.0);
        self.params.resonance = clampf(self.params.resonance, 0.5, 18.0);
        self.params.drive = clampf(self.params.drive, 1.0, 6.0);
        self.params.attack = clampf(self.params.attack, 0.001, 6.0);
        self.params.decay = clampf(self.params.decay, 0.005, 8.0);
        self.params.sustain = clampf(self.params.sustain, 0.0, 1.0);
        self.params.release = clampf(self.params.release, 0.005, 8.0);
        self.params.sub = clampf(self.params.sub, 0.0, 1.0);
        self.params.noise = clampf(self.params.noise, 0.0, 1.0);
        self.params.level = clampf(self.params.level, 0.0, 1.2);
        self.params.filter_mode = clampi(self.params.filter_mode, 0, 3);
        self.params.chord = clampi(self.params.chord, 0, 4);

        for index in 0..VOICE_COUNT {
            let voice = &mut self.voices[index];
            voice.env.set_attack(self.params.attack);
            voice.env.set_decay(self.params.decay);
            voice.env.set_sustain(self.params.sustain);
            voice.env.set_release(self.params.release);
            if voice.active {
                self.update_voice_tuning(index);
            }
        }
    }

    pub fn params(&self) -> WaveParams {
        self.params
    }

    fn update_voice_tuning(&mut self, index: usize) {
        let unison = clampi(self.params.unison, 1, UNISON_MAX as i32) as usize;
        let voice = &mut self.voices[index];
        let base_freq = midi_to_freq(voice.note as f32);
        let detune_range = self.params.detune;
        let spread = self.params.spread;
        let norm = if unison > 1 {
            1.0 / (unison - 1) as f32
        } else {
            0.0
        };
        let gain_norm = 0.75 / (unison as f32).sqrt();
        for u in 0..UNISON_MAX {
            let spread_pos = (u as f32 * norm) * 2.0 - 1.0;
            let cents = if unison == 1 {
                0.0
            } else {
                spread_pos * detune_range * 0.5
            };
            let pan = if unison == 1 {
                0.0
            } else {
                spread_pos * spread
            };
            let freq = base_freq * 2.0_f32.powf(cents / 1200.0);
            voice.inc[u] = if u < unison {
                freq / self.sample_rate_f
            } else {
                0.0
            };
            voice.pan[u] = pan;
            voice.gain[u] = if u < unison { gain_norm } else { 0.0 };
        }
        voice.sub_inc = (base_freq * 0.5) / self.sample_rate_f;
    }

    fn chord_interval(chord: i32, index: usize) -> i32 {
        const INTERVALS: [[i32; 3]; 5] = [
            [0, 0, 0],   // single
            [0, 7, 12],  // fifth
            [0, 3, 7],   // minor
            [0, 4, 7],   // major
            [0, 12, 19], // octave stack
        ];
        INTERVALS[clampi(chord, 0, 4) as usize][index.min(2)]
    }

    fn start_voice(&mut self, note: i32, velocity: i32, root_note: i32) {
        let mut slot = None;
        for (i, voice) in self.voices.iter().enumerate() {
            if !voice.active {
                slot = Some(i);
                break;
            }
        }
        let slot = slot.unwrap_or_else(|| {
            let mut oldest = u32::MAX;
            let mut chosen = 0;
            for (i, voice) in self.voices.iter().enumerate() {
                if voice.age < oldest {
                    oldest = voice.age;
                    chosen = i;
                }
            }
            chosen
        });

        self.age_counter = self.age_counter.wrapping_add(1);
        let age = self.age_counter;
        let params = self.params;
        let sample_rate = self.sample_rate_f;
        let seed_phase = |rng: &mut Rng| rng.uniform();

        let voice = &mut self.voices[slot];
        *voice = WaveVoice::default();
        voice.active = true;
        voice.held = true;
        voice.note = clampi(note, 0, 127);
        voice.root_note = clampi(root_note, 0, 127);
        voice.velocity = clampf(velocity as f32 / 127.0, 0.05, 1.0);
        voice.age = age;
        voice.env.prepare(sample_rate);
        voice.env.set_attack(params.attack);
        voice.env.set_decay(params.decay);
        voice.env.set_sustain(params.sustain);
        voice.env.set_release(params.release);
        voice.env.note_on();
        for u in 0..UNISON_MAX {
            voice.phase[u] = seed_phase(&mut self.rng);
        }
        voice.filter_l.prepare(sample_rate);
        voice.filter_r.prepare(sample_rate);
        voice.filter2_l.prepare(sample_rate);
        voice.filter2_r.prepare(sample_rate);
        self.update_voice_tuning(slot);
    }

    pub fn note_on(&mut self, note: i32, velocity: i32) {
        let notes = if self.params.chord == 0 { 1 } else { 3 };
        for i in 0..notes {
            let interval = Self::chord_interval(self.params.chord, i);
            self.start_voice(note + interval, velocity, note);
        }
    }

    pub fn note_off(&mut self, note: i32) {
        for voice in self.voices.iter_mut() {
            if voice.active && voice.root_note == note && voice.held {
                voice.held = false;
                voice.env.note_off();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for voice in self.voices.iter_mut() {
            if voice.active {
                voice.held = false;
                voice.env.note_off();
            }
        }
    }

    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    #[allow(clippy::needless_range_loop)]
    pub fn render(&mut self, out_l: &mut f32, out_r: &mut f32) {
        let mut l = 0.0f32;
        let mut r = 0.0f32;

        self.smoothed_cutoff += (self.params.cutoff - self.smoothed_cutoff) * 0.002;
        self.smoothed_resonance += (self.params.resonance - self.smoothed_resonance) * 0.002;
        self.smoothed_drive += (self.params.drive - self.smoothed_drive) * 0.002;

        let params = self.params;
        let banks = shared_wavetable_banks();
        let unison = clampi(params.unison, 1, UNISON_MAX as i32) as usize;
        let cutoff_smooth = self.smoothed_cutoff;
        let resonance = self.smoothed_resonance;
        let drive = self.smoothed_drive;

        for voice in self.voices.iter_mut() {
            if !voice.active {
                continue;
            }
            let env = voice.env.process();
            if !voice.env.active() && env < 0.0002 {
                voice.active = false;
                continue;
            }

            let note_cutoff = cutoff_smooth * 2.0_f32.powf((voice.note as f32 - 60.0) / 48.0);
            let cutoff = clampf(note_cutoff, 40.0, 19_000.0);
            let freq = midi_to_freq(voice.note as f32);
            let mip = clampi(
                (freq.max(20.0) / 30.0).log2() as i32,
                0,
                MIP_COUNT as i32 - 1,
            ) as usize;

            let mut voice_l = 0.0f32;
            let mut voice_r = 0.0f32;
            for u in 0..unison {
                voice.phase[u] = wrap01(voice.phase[u] + voice.inc[u]);
                let s = banks.sample(params.bank as usize, params.position, mip, voice.phase[u]);
                let angle = (voice.pan[u] + 1.0) * 0.25 * PI_F;
                voice_l += s * voice.gain[u] * angle.cos();
                voice_r += s * voice.gain[u] * angle.sin();
            }

            if params.sub > 0.0001 {
                voice.sub_phase = wrap01(voice.sub_phase + voice.sub_inc);
                let sub = (TWO_PI_F * voice.sub_phase).sin() * params.sub * 0.45;
                voice_l += sub;
                voice_r += sub;
            }
            if params.noise > 0.0001 {
                let n = self.rng.noise() * params.noise * 0.12;
                voice_l += n;
                voice_r -= n;
            }

            voice_l = voice.filter_l.process(voice_l, cutoff, resonance, 0, drive);
            voice_r = voice.filter_r.process(voice_r, cutoff, resonance, 0, drive);
            if params.filter_mode == 0 {
                voice_l = voice.filter2_l.process(voice_l, cutoff, resonance, 0, 1.0);
                voice_r = voice.filter2_r.process(voice_r, cutoff, resonance, 0, 1.0);
            } else if params.filter_mode == 2 {
                voice_l = voice.filter2_l.process(voice_l, cutoff, resonance, 1, 1.0);
                voice_r = voice.filter2_r.process(voice_r, cutoff, resonance, 1, 1.0);
            } else if params.filter_mode == 3 {
                voice_l = voice.filter2_l.process(voice_l, cutoff, resonance, 2, 1.0);
                voice_r = voice.filter2_r.process(voice_r, cutoff, resonance, 2, 1.0);
            }

            let amp = env * voice.velocity * params.level;
            l += fast_tanh(voice_l * amp);
            r += fast_tanh(voice_r * amp);
        }

        *out_l = sanitize(l);
        *out_r = sanitize(r);
    }
}
