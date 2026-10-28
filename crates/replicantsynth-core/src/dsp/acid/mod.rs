//! Replicant303 voice: monophonic acid line with polyBLEP oscillator, diode
//! ladder filter, capacitor slide, accent envelope and a five-model distortion.

mod distortion;
mod envelope;
mod filter;
mod oscillator;

pub use distortion::DistortionType;

use crate::dsp::{clampf, midi_to_freq, sanitize};
use distortion::DistortionUnit;
use envelope::Envelope303;
use filter::DiodeLadderFilter;
use oscillator::{Oscillator303, Waveform};

#[derive(Clone, Copy, Debug)]
struct ActiveNote {
    note: i32,
    /// Kept for parity with the C++ voice: per-note velocity is recorded even
    /// though the 303 only distinguishes accent from normal by `note_on`.
    #[allow(dead_code)]
    velocity: i32,
}

/// The acid bass voice used by every `ClipKind::Acid` pad.
pub struct Acid303 {
    sample_rate: f64,
    osc: Oscillator303,
    filter: DiodeLadderFilter,
    env: Envelope303,
    distortion: DistortionUnit,

    env_mod: f32,
    accent_amount: f32,
    master_gain: f32,

    note_stack: Vec<ActiveNote>,
    target_pitch_hz: f32,
    current_pitch_hz: f32,
    pitch_slide_coeff: f32,

    slide_led_active: bool,
    accent_led_active: bool,
    last_note_number: i32,
}

impl Default for Acid303 {
    fn default() -> Self {
        Self::new()
    }
}

impl Acid303 {
    pub fn new() -> Self {
        let mut voice = Self {
            sample_rate: 48_000.0,
            osc: Oscillator303::default(),
            filter: DiodeLadderFilter::default(),
            env: Envelope303::default(),
            distortion: DistortionUnit::default(),
            env_mod: 0.5,
            accent_amount: 0.7,
            master_gain: 0.8,
            note_stack: Vec::with_capacity(128),
            target_pitch_hz: 110.0,
            current_pitch_hz: 110.0,
            pitch_slide_coeff: 0.01,
            slide_led_active: false,
            accent_led_active: false,
            last_note_number: 36,
        };
        voice.prepare(48_000.0);
        voice
    }

    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1_000.0);
        self.osc.prepare(self.sample_rate);
        self.filter.prepare(self.sample_rate);
        self.env.prepare(self.sample_rate);
        self.distortion.prepare(self.sample_rate);
        if self.note_stack.capacity() < 128 {
            self.note_stack.reserve(128 - self.note_stack.capacity());
        }
        self.update_slide_coefficient();
        self.reset();
    }

    pub fn reset(&mut self) {
        self.osc.reset();
        self.filter.reset();
        self.env.reset();
        self.distortion.reset();
        self.note_stack.clear();
        self.target_pitch_hz = 110.0;
        self.current_pitch_hz = 110.0;
        self.slide_led_active = false;
        self.accent_led_active = false;
    }

    fn update_slide_coefficient(&mut self) {
        let dt = 1.0 / self.sample_rate as f32;
        self.pitch_slide_coeff = 1.0 - (-dt / 0.060).exp();
    }

    pub fn set_waveform(&mut self, wave: i32) {
        self.osc.set_waveform(if wave == 1 {
            Waveform::Square
        } else {
            Waveform::Sawtooth
        });
    }

    pub fn set_tuning(&mut self, semitones: f32) {
        self.osc.set_tuning_semi(semitones);
    }

    pub fn set_cutoff(&mut self, hz: f32) {
        self.filter.set_cutoff(hz);
    }

    pub fn set_resonance(&mut self, resonance01: f32) {
        self.filter.set_resonance(resonance01);
    }

    pub fn set_env_mod(&mut self, env_mod01: f32) {
        self.env_mod = clampf(env_mod01, 0.0, 1.0);
    }

    pub fn set_decay(&mut self, seconds: f32) {
        self.env.set_decay_time(seconds);
    }

    pub fn set_accent(&mut self, accent01: f32) {
        self.accent_amount = clampf(accent01, 0.0, 1.0);
    }

    pub fn set_volume(&mut self, linear: f32) {
        self.master_gain = clampf(linear, 0.0, 1.5);
    }

    pub fn set_distortion(&mut self, active: bool, kind: i32, drive: f32, tone: f32, mix: f32) {
        self.distortion.set_active(active);
        self.distortion.set_type(DistortionType::from_index(kind));
        self.distortion.set_drive(drive);
        self.distortion.set_tone(tone);
        self.distortion.set_mix(mix);
    }

    pub fn note_on(&mut self, note: i32, velocity: i32) {
        self.last_note_number = note;
        let accented = velocity >= 100;
        let legato = !self.note_stack.is_empty();

        self.target_pitch_hz = midi_to_freq(note as f32);
        if !legato {
            self.current_pitch_hz = self.target_pitch_hz;
            self.slide_led_active = false;
        } else {
            self.slide_led_active = true;
        }
        self.accent_led_active = accented;
        self.env.note_on(accented, legato);

        self.note_stack.retain(|n| n.note != note);
        if self.note_stack.len() >= 128 {
            self.note_stack.remove(0);
        }
        self.note_stack.push(ActiveNote { note, velocity });
    }

    pub fn note_off(&mut self, note: i32) {
        if let Some(pos) = self.note_stack.iter().position(|n| n.note == note) {
            self.note_stack.remove(pos);
        }
        if self.note_stack.is_empty() {
            self.env.note_off();
            self.slide_led_active = false;
            self.accent_led_active = false;
        } else {
            self.target_pitch_hz = midi_to_freq(self.note_stack.last().unwrap().note as f32);
            self.slide_led_active = true;
        }
    }

    pub fn all_notes_off(&mut self) {
        self.note_stack.clear();
        self.env.note_off();
        self.slide_led_active = false;
        self.accent_led_active = false;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        self.current_pitch_hz +=
            (self.target_pitch_hz - self.current_pitch_hz) * self.pitch_slide_coeff;

        self.osc.set_frequency(self.current_pitch_hz);
        let raw_osc = self.osc.process();

        let (vcf_env, vca_env, accent_env) = self.env.process();
        let filtered = self.filter.process(
            raw_osc,
            self.env_mod,
            vcf_env,
            accent_env * self.accent_amount,
        );
        let vca_boost = 1.0 + accent_env * self.accent_amount * 0.45;
        let amplified = filtered * vca_env * vca_boost;
        let distorted = self.distortion.process_sample(amplified);
        sanitize(distorted * self.master_gain)
    }

    pub fn is_active(&self) -> bool {
        self.env.is_active()
    }

    pub fn held_notes(&self) -> usize {
        self.note_stack.len()
    }

    pub fn sliding(&self) -> bool {
        self.slide_led_active
    }

    pub fn accent_led(&self) -> bool {
        self.accent_led_active
    }

    pub fn last_note(&self) -> i32 {
        self.last_note_number
    }
}
