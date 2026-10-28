//! Replicant909: eleven circuit-level drum voices generated from a 6-bit ROM.

mod kit;
mod rom;
mod voices;

pub use kit::DrumKit909;
pub use rom::{rom_data, RomData, ROM_SAMPLE_RATE};

/// Standard TR-909 MIDI note mapping with multi-octave tolerance.
pub fn midi_note_to_instrument(note: i32) -> Option<usize> {
    const BD: usize = 0;
    const SD: usize = 1;
    const LT: usize = 2;
    const MT: usize = 3;
    const HT: usize = 4;
    const RS: usize = 5;
    const CP: usize = 6;
    const CH: usize = 7;
    const OH: usize = 8;
    const CC: usize = 9;
    const RC: usize = 10;

    if !(0..=127).contains(&note) {
        return None;
    }
    // Any octave of C triggers the bass drum.
    if note % 12 == 0 {
        return Some(BD);
    }
    if note == 35 || note == 23 || note == 11 {
        return Some(BD);
    }
    match note {
        37 => return Some(RS),
        38 | 40 => return Some(SD),
        39 => return Some(CP),
        41 | 43 => return Some(LT),
        42 | 44 => return Some(CH),
        45 | 47 => return Some(MT),
        46 => return Some(OH),
        49 => return Some(CC),
        50 => return Some(HT),
        51 => return Some(RC),
        _ => {}
    }
    if (61..=70).contains(&note) {
        const CHROMATIC: [usize; 10] = [SD, LT, MT, HT, RS, CP, CH, OH, CC, RC];
        return Some(CHROMATIC[(note - 61) as usize]);
    }
    for shift in [-24, -12, 12, 24, 36] {
        match note - shift {
            37 => return Some(RS),
            38 | 40 => return Some(SD),
            39 => return Some(CP),
            41 | 43 => return Some(LT),
            42 | 44 => return Some(CH),
            45 | 47 => return Some(MT),
            46 => return Some(OH),
            49 => return Some(CC),
            50 => return Some(HT),
            51 => return Some(RC),
            _ => {}
        }
    }
    None
}

pub fn instrument_default_note(instrument: usize) -> i32 {
    match instrument {
        0 => 36,
        1 => 38,
        2 => 41,
        3 => 45,
        4 => 50,
        5 => 37,
        6 => 39,
        7 => 42,
        8 => 46,
        9 => 49,
        10 => 51,
        _ => 36,
    }
}

pub const VOICE_NAMES: [&str; 11] = [
    "BD", "SD", "LT", "MT", "HT", "RS", "CP", "CH", "OH", "CC", "RC",
];

pub const VOICE_LONG_NAMES: [&str; 11] = [
    "BASS DRUM",
    "SNARE DRUM",
    "LOW TOM",
    "MID TOM",
    "HI TOM",
    "RIM SHOT",
    "HAND CLAP",
    "CLOSED HI-HAT",
    "OPEN HI-HAT",
    "CRASH CYMBAL",
    "RIDE CYMBAL",
];

pub(crate) const PI_F: f32 = std::f32::consts::PI;
pub(crate) const TWO_PI_F: f32 = std::f32::consts::TAU;

#[inline]
pub(crate) fn fast_tanh(x: f32) -> f32 {
    if x < -3.0 {
        return -1.0;
    }
    if x > 3.0 {
        return 1.0;
    }
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Diode-transistor waveshaper for the 909 triangle-to-sine conversion.
#[inline]
pub(crate) fn diode_waveshaper(x: f32) -> f32 {
    let v = x + 0.035 * (x * x - 1.0);
    fast_tanh(1.45 * v)
}

/// Xorshift white noise, the same generator the C++ voices use.
#[derive(Clone, Debug)]
pub(crate) struct FastNoise {
    state: u32,
}

impl FastNoise {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed != 0 { seed } else { 0x1337_9091 },
        }
    }

    #[inline]
    pub fn next_sample(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state as i32 as f32 * (1.0 / 2_147_483_648.0)
    }
}

/// Chamberlin state-variable filter used for the analog circuit models.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AnalogSvf {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

impl AnalogSvf {
    pub fn reset(&mut self) {
        self.lp = 0.0;
        self.bp = 0.0;
        self.hp = 0.0;
    }

    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, q: f32, sample_rate: f32) {
        let f = 2.0 * (PI_F * (cutoff_hz / sample_rate).min(0.48)).sin();
        let damp = 1.0 / q.max(0.2);
        self.hp = input - self.lp - damp * self.bp;
        self.bp += f * self.hp;
        self.lp += f * self.bp;
    }
}

/// Shared front-panel parameters of one 909 voice.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VoiceParams {
    pub tune: f32,
    pub level: f32,
    pub attack: f32,
    pub decay: f32,
    pub snappy: f32,
    pub tone: f32,
    /// Pan is applied by the kit mix, not by the voice, so a voice never reads it.
    #[allow(dead_code)]
    pub pan: f32,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            tune: 0.5,
            level: 0.8,
            attack: 0.5,
            decay: 0.5,
            snappy: 0.5,
            tone: 0.5,
            pan: 0.5,
        }
    }
}

/// Every circuit model implements this small surface.
pub(crate) trait DrumVoice: Send {
    fn prepare(&mut self, sample_rate: f64);
    fn reset(&mut self);
    fn trigger(&mut self, velocity: f32, accent: f32);
    fn choke(&mut self) {}
    fn render_sample(&mut self) -> f32;
    fn is_active(&self) -> bool;
    fn set_params(&mut self, params: VoiceParams);
}
