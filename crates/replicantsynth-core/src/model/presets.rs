//! Factory content for the 303 and the 909, ported from the Android model.
//!
//! 512 acid lines are generated deterministically (8 banks x 64 slots) with the
//! same SplitMix32 stream the desktop bank ships with, plus the eight classic
//! 909 patterns.

use super::clip::{
    AcidParam, Clip, ClipKind, DrumParam, FxParam, MidiNote, Step, DRUM_VOICES, PARAMS, STEPS,
};

#[derive(Clone)]
pub struct AcidPreset {
    pub name: String,
    pub bank: usize,
    pub slot: usize,
    pub root: i32,
    pub length: i32,
    pub params: [f32; PARAMS],
    pub steps: [Step; STEPS],
}

impl AcidPreset {
    pub fn bank_letter(&self) -> char {
        (b'A' + self.bank as u8) as char
    }

    pub fn display_name(&self) -> String {
        format!(
            "{} {:02} · {}",
            self.bank_letter(),
            self.slot + 1,
            self.name
        )
    }
}

/// SplitMix32, the deterministic PRNG the desktop 303 bank uses.
struct FastRandom {
    state: u32,
}

impl FastRandom {
    fn new(seed: u32) -> Self {
        Self {
            state: if seed != 0 { seed } else { 0x3030_3030 },
        }
    }

    fn next(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9e37_79b9);
        let mut z = self.state;
        z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
        z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
        z ^ (z >> 16)
    }

    fn next_float(&mut self) -> f32 {
        (self.next() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    fn next_int(&mut self, low: i32, high: i32) -> i32 {
        let span = (high - low + 1) as u32;
        low + (self.next() % span) as i32
    }

    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next_float()
    }

    fn chance(&mut self, probability: f32) -> bool {
        self.next_float() < probability
    }
}

struct DefaultLine {
    notes: [i32; 16],
    accents: &'static [usize],
    slides: &'static [usize],
    muted: &'static [usize],
    length: i32,
}

const DEFAULT_LINES: [DefaultLine; 8] = [
    DefaultLine {
        notes: [
            36, 36, 48, 36, 39, 36, 43, 36, 36, 48, 36, 39, 36, 43, 36, 36,
        ],
        accents: &[1, 6, 10],
        slides: &[4, 8],
        muted: &[3, 11],
        length: 16,
    },
    DefaultLine {
        notes: [
            24, 24, 36, 24, 27, 24, 31, 24, 24, 36, 24, 27, 31, 24, 24, 24,
        ],
        accents: &[1, 5, 9, 13],
        slides: &[2, 10],
        muted: &[7, 15],
        length: 16,
    },
    DefaultLine {
        notes: [
            36, 48, 36, 43, 36, 48, 39, 36, 36, 43, 48, 36, 39, 36, 43, 36,
        ],
        accents: &[1, 3, 5, 7, 9, 11, 13, 15],
        slides: &[2, 6, 10, 14],
        muted: &[],
        length: 16,
    },
    DefaultLine {
        notes: [
            24, 36, 24, 31, 24, 36, 27, 24, 31, 36, 24, 27, 24, 31, 36, 24,
        ],
        accents: &[0, 4, 8, 12],
        slides: &[1, 5, 9, 13],
        muted: &[6, 14],
        length: 16,
    },
    DefaultLine {
        notes: [
            36, 36, 39, 36, 43, 36, 46, 43, 36, 36, 39, 43, 36, 36, 31, 36,
        ],
        accents: &[3, 7, 11, 15],
        slides: &[4, 12],
        muted: &[1, 9],
        length: 16,
    },
    DefaultLine {
        notes: [
            48, 36, 48, 39, 48, 43, 48, 36, 46, 36, 46, 39, 43, 39, 36, 36,
        ],
        accents: &[0, 2, 6, 8, 12, 14],
        slides: &[2, 6, 10, 14],
        muted: &[4, 12],
        length: 16,
    },
    DefaultLine {
        notes: [
            24, 27, 24, 36, 24, 31, 27, 24, 36, 24, 27, 31, 24, 27, 24, 24,
        ],
        accents: &[1, 5, 9, 13],
        slides: &[3, 7, 11, 15],
        muted: &[2, 10],
        length: 16,
    },
    DefaultLine {
        notes: [
            36, 31, 36, 43, 36, 31, 39, 36, 36, 31, 36, 43, 46, 43, 39, 36,
        ],
        accents: &[0, 3, 6, 9, 12, 15],
        slides: &[1, 5, 9, 13],
        muted: &[7, 14],
        length: 16,
    },
];

const BANK_NAMES: [&str; 8] = [
    "AMBER",
    "SUB",
    "NEON",
    "ACID",
    "BLADE",
    "ROOFTOP",
    "OFF WORLD",
    "LAST",
];

const FACTORY_NAMES: [&str; 8] = [
    "Amber Line",
    "Sub Level",
    "Neon Run",
    "Acid Rain",
    "Blade Pulse",
    "Rooftop Chase",
    "Off World",
    "Last Replicant",
];

const ADJECTIVES: [&str; 64] = [
    "Amber",
    "Sub",
    "Neon",
    "Acid",
    "Blade",
    "Rooftop",
    "Off",
    "Last",
    "Chrome",
    "Static",
    "Velvet",
    "Rust",
    "Ghost",
    "Silver",
    "Crimson",
    "Electric",
    "Hollow",
    "Bright",
    "Dark",
    "Fever",
    "Glass",
    "Iron",
    "Lunar",
    "Magnetic",
    "Midnight",
    "Mirror",
    "Molten",
    "Obsidian",
    "Phantom",
    "Plastic",
    "Prism",
    "Quartz",
    "Radiant",
    "Reactor",
    "Rebel",
    "Redline",
    "Satin",
    "Scarlet",
    "Secret",
    "Shadow",
    "Silent",
    "Solar",
    "Sonic",
    "Spectral",
    "Steel",
    "Storm",
    "Subzero",
    "Sunken",
    "Tidal",
    "Toxic",
    "Turbo",
    "Twilight",
    "Ultraviolet",
    "Vacuum",
    "Vapor",
    "Velour",
    "Violet",
    "Voltage",
    "Wired",
    "Wraith",
    "Zero",
    "Zenith",
    "Cobalt",
    "Carbon",
];

const NOUNS: [&str; 64] = [
    "Line",
    "Level",
    "Run",
    "Rain",
    "Pulse",
    "Chase",
    "World",
    "Replicant",
    "Circuit",
    "Signal",
    "Engine",
    "Wire",
    "Grid",
    "Vault",
    "Machine",
    "Protocol",
    "Sequence",
    "District",
    "Corridor",
    "Frequency",
    "Reactor",
    "Tempo",
    "Voltage",
    "Current",
    "Motion",
    "Shadow",
    "Mirror",
    "Ladder",
    "Filter",
    "Oscillator",
    "Groove",
    "Pattern",
    "Sector",
    "Tunnel",
    "Chamber",
    "Beacon",
    "Antenna",
    "Archive",
    "Bishop",
    "Cipher",
    "Column",
    "Compass",
    "Console",
    "Cortex",
    "Cradle",
    "Cursor",
    "Delta",
    "Divider",
    "Driver",
    "Echo",
    "Ember",
    "Envelope",
    "Fabric",
    "Factory",
    "Frame",
    "Horizon",
    "Kernel",
    "Lantern",
    "Lattice",
    "Matrix",
    "Meridian",
    "Monolith",
    "Neuron",
    "Orbit",
];

const SCALES: [[i32; 8]; 8] = [
    [0, 3, 5, 7, 10, 12, 15, 19],
    [0, 2, 3, 5, 7, 8, 10, 12],
    [0, 2, 4, 7, 9, 12, 14, 16],
    [0, 1, 3, 5, 6, 8, 10, 12],
    [0, 3, 5, 6, 7, 10, 12, 15],
    [0, 2, 3, 5, 7, 9, 10, 12],
    [0, 2, 3, 5, 7, 8, 11, 12],
    [0, 1, 2, 4, 6, 7, 10, 12],
];

const LENGTHS: [i32; 8] = [16, 16, 16, 16, 16, 12, 8, 7];

pub struct AcidPresets;

impl AcidPresets {
    pub const BANKS: usize = 8;
    pub const PER_BANK: usize = 64;
    pub const COUNT: usize = Self::BANKS * Self::PER_BANK;

    pub fn bank_names() -> &'static [&'static str; 8] {
        &BANK_NAMES
    }

    fn default_params() -> [f32; PARAMS] {
        let mut p = [0.0f32; PARAMS];
        p[AcidParam::WAVE] = 0.0;
        p[AcidParam::CUTOFF] = 350.0;
        p[AcidParam::RESONANCE] = 0.65;
        p[AcidParam::ENV_MOD] = 0.60;
        p[AcidParam::DECAY] = 0.75;
        p[AcidParam::ACCENT] = 0.70;
        p[AcidParam::DIST_TYPE] = 0.0;
        p[AcidParam::DIST_DRIVE] = 0.50;
        p[AcidParam::DIST_MIX] = 0.85;
        p[AcidParam::DIST_TONE] = 0.60;
        p[AcidParam::VOLUME] = 0.90;
        p
    }

    fn default_steps(line: &DefaultLine) -> [Step; STEPS] {
        let mut steps = [Step::default(); STEPS];
        for (i, slot) in steps.iter_mut().enumerate() {
            *slot = Step {
                gate: !line.muted.contains(&i),
                note: line.notes[i].clamp(24, 48) as u8,
                velocity: if line.accents.contains(&i) { 118 } else { 92 },
                accent: line.accents.contains(&i),
                slide: line.slides.contains(&i),
            };
        }
        steps
    }

    fn preset_from(
        name: String,
        bank: usize,
        slot: usize,
        length: i32,
        steps: [Step; STEPS],
        params: [f32; PARAMS],
    ) -> AcidPreset {
        let root = steps
            .iter()
            .find(|s| s.gate)
            .map(|s| s.note as i32)
            .unwrap_or(36);
        AcidPreset {
            name,
            bank,
            slot,
            root,
            length: length.clamp(1, STEPS as i32),
            params,
            steps,
        }
    }

    pub fn preset(bank: usize, slot: usize) -> AcidPreset {
        let b = bank.min(Self::BANKS - 1);
        let s = slot.min(Self::PER_BANK - 1);
        if s == 0 {
            let line = &DEFAULT_LINES[b];
            return Self::preset_from(
                FACTORY_NAMES[b].to_string(),
                b,
                s,
                line.length,
                Self::default_steps(line),
                Self::default_params(),
            );
        }
        Self::generated(b, s)
    }

    fn generated(bank: usize, slot: usize) -> AcidPreset {
        let seed = ((bank + 1) as u32).wrapping_mul(0x9e37_79b9)
            ^ ((slot + 1) as u32).wrapping_mul(0x85eb_ca6b)
            ^ 0x5244_3303;
        let mut rng = FastRandom::new(seed);
        let scale = SCALES[bank];
        let root = 24 + rng.next_int(0, 12);
        let length = LENGTHS[rng.next_int(0, LENGTHS.len() as i32 - 1) as usize];
        let density = rng.between(0.62, 0.94);
        let accent_chance = rng.between(0.16, 0.38);
        let slide_chance = rng.between(0.10, 0.32);

        let mut gates = [false; STEPS];
        let mut notes = [0i32; STEPS];
        let mut accents = [false; STEPS];
        let mut slides = [false; STEPS];

        let mut previous_note = root + scale[0];
        for i in 0..STEPS {
            let gate = i == 0 || rng.chance(density);
            gates[i] = gate;
            let degree = rng.next_int(0, 7) as usize;
            let mut octave = 0;
            if rng.chance(0.16) {
                octave = 12;
            }
            if rng.chance(0.08) {
                octave = -12;
            }
            let mut note = root + scale[degree] + octave;
            if rng.chance(0.12) {
                note = previous_note;
            }
            note = note.clamp(24, 48);
            notes[i] = note;
            previous_note = note;
            accents[i] = gate && rng.chance(accent_chance);
            slides[i] = gate && rng.chance(slide_chance);
        }

        let mut gate_count = (0..length as usize).filter(|i| gates[*i]).count();
        let mut guard = 0;
        while gate_count < 4 && guard < 64 {
            let i = rng.next_int(0, length - 1) as usize;
            if !gates[i] {
                gates[i] = true;
                gate_count += 1;
            }
            guard += 1;
        }

        let all_same_note = (1..length as usize).all(|i| notes[i] == notes[0]);
        if all_same_note {
            notes[(length / 2) as usize] = (notes[0] + scale[3]).clamp(24, 48);
            notes[(length - 1) as usize] = (notes[0] + scale[5]).clamp(24, 48);
        }
        if (0..length as usize).all(|i| !accents[i]) {
            accents[0] = true;
        }

        let mut p = Self::default_params();
        match (bank + slot) % 8 {
            0 => {
                p[AcidParam::WAVE] = 0.0;
                p[AcidParam::CUTOFF] = log_between(&mut rng, 170.0, 900.0);
                p[AcidParam::RESONANCE] = rng.between(0.55, 0.84);
                p[AcidParam::ENV_MOD] = rng.between(0.50, 0.82);
                p[AcidParam::DECAY] = rng.between(0.38, 0.90);
                p[AcidParam::ACCENT] = rng.between(0.60, 0.85);
                p[AcidParam::DIST_TYPE] = 0.0;
                p[AcidParam::DIST_DRIVE] = rng.between(0.28, 0.62);
                p[AcidParam::DIST_TONE] = rng.between(0.45, 0.75);
                p[AcidParam::DIST_MIX] = rng.between(0.62, 0.90);
            }
            1 => {
                p[AcidParam::WAVE] = 0.0;
                p[AcidParam::CUTOFF] = log_between(&mut rng, 70.0, 380.0);
                p[AcidParam::RESONANCE] = rng.between(0.30, 0.68);
                p[AcidParam::ENV_MOD] = rng.between(0.30, 0.68);
                p[AcidParam::DECAY] = rng.between(0.65, 1.45);
                p[AcidParam::ACCENT] = rng.between(0.55, 0.82);
                p[AcidParam::DIST_TYPE] = if rng.chance(0.70) { 0.0 } else { 2.0 };
                p[AcidParam::DIST_DRIVE] = rng.between(0.18, 0.48);
                p[AcidParam::DIST_TONE] = rng.between(0.25, 0.55);
                p[AcidParam::DIST_MIX] = rng.between(0.55, 0.82);
            }
            2 => {
                p[AcidParam::WAVE] = if rng.chance(0.75) { 1.0 } else { 0.0 };
                p[AcidParam::CUTOFF] = log_between(&mut rng, 800.0, 2600.0);
                p[AcidParam::RESONANCE] = rng.between(0.35, 0.78);
                p[AcidParam::ENV_MOD] = rng.between(0.68, 0.95);
                p[AcidParam::DECAY] = rng.between(0.20, 0.52);
                p[AcidParam::ACCENT] = rng.between(0.50, 0.80);
                p[AcidParam::DIST_TYPE] = if rng.chance(0.65) { 1.0 } else { 0.0 };
                p[AcidParam::DIST_DRIVE] = rng.between(0.15, 0.42);
                p[AcidParam::DIST_TONE] = rng.between(0.62, 0.90);
                p[AcidParam::DIST_MIX] = rng.between(0.35, 0.68);
            }
            3 => {
                p[AcidParam::WAVE] = 0.0;
                p[AcidParam::CUTOFF] = log_between(&mut rng, 280.0, 1300.0);
                p[AcidParam::RESONANCE] = rng.between(0.68, 0.94);
                p[AcidParam::ENV_MOD] = rng.between(0.58, 0.90);
                p[AcidParam::DECAY] = rng.between(0.22, 0.62);
                p[AcidParam::ACCENT] = rng.between(0.65, 0.92);
                p[AcidParam::DIST_TYPE] = if rng.chance(0.60) { 2.0 } else { 0.0 };
                p[AcidParam::DIST_DRIVE] = rng.between(0.58, 0.92);
                p[AcidParam::DIST_TONE] = rng.between(0.40, 0.78);
                p[AcidParam::DIST_MIX] = rng.between(0.72, 1.0);
            }
            4 => {
                p[AcidParam::WAVE] = if rng.chance(0.70) { 1.0 } else { 0.0 };
                p[AcidParam::CUTOFF] = log_between(&mut rng, 200.0, 1000.0);
                p[AcidParam::RESONANCE] = rng.between(0.45, 0.80);
                p[AcidParam::ENV_MOD] = rng.between(0.48, 0.86);
                p[AcidParam::DECAY] = rng.between(0.30, 0.85);
                p[AcidParam::ACCENT] = rng.between(0.55, 0.85);
                p[AcidParam::DIST_TYPE] = 3.0;
                p[AcidParam::DIST_DRIVE] = rng.between(0.48, 0.90);
                p[AcidParam::DIST_TONE] = rng.between(0.30, 0.68);
                p[AcidParam::DIST_MIX] = rng.between(0.60, 0.95);
            }
            5 => {
                p[AcidParam::WAVE] = 0.0;
                p[AcidParam::CUTOFF] = log_between(&mut rng, 380.0, 1900.0);
                p[AcidParam::RESONANCE] = rng.between(0.42, 0.82);
                p[AcidParam::ENV_MOD] = rng.between(0.62, 0.94);
                p[AcidParam::DECAY] = rng.between(0.25, 0.72);
                p[AcidParam::ACCENT] = rng.between(0.55, 0.88);
                p[AcidParam::DIST_TYPE] = 4.0;
                p[AcidParam::DIST_DRIVE] = rng.between(0.38, 0.86);
                p[AcidParam::DIST_TONE] = rng.between(0.42, 0.82);
                p[AcidParam::DIST_MIX] = rng.between(0.50, 0.90);
            }
            6 => {
                p[AcidParam::WAVE] = if rng.chance(0.55) { 0.0 } else { 1.0 };
                p[AcidParam::CUTOFF] = log_between(&mut rng, 650.0, 2200.0);
                p[AcidParam::RESONANCE] = rng.between(0.18, 0.55);
                p[AcidParam::ENV_MOD] = rng.between(0.38, 0.78);
                p[AcidParam::DECAY] = rng.between(0.28, 0.72);
                p[AcidParam::ACCENT] = rng.between(0.42, 0.75);
                let active = rng.chance(0.35);
                p[AcidParam::DIST_TYPE] = rng.next_int(0, 4) as f32;
                p[AcidParam::DIST_DRIVE] = rng.between(0.10, 0.38);
                p[AcidParam::DIST_TONE] = rng.between(0.50, 0.85);
                let mix = rng.between(0.20, 0.55);
                p[AcidParam::DIST_MIX] = if active { mix } else { 0.0 };
            }
            _ => {
                p[AcidParam::WAVE] = 0.0;
                p[AcidParam::CUTOFF] = log_between(&mut rng, 140.0, 720.0);
                p[AcidParam::RESONANCE] = rng.between(0.60, 0.94);
                p[AcidParam::ENV_MOD] = rng.between(0.70, 0.97);
                p[AcidParam::DECAY] = rng.between(0.20, 0.48);
                p[AcidParam::ACCENT] = rng.between(0.72, 0.96);
                p[AcidParam::DIST_TYPE] = if rng.chance(0.68) { 0.0 } else { 2.0 };
                p[AcidParam::DIST_DRIVE] = rng.between(0.70, 1.0);
                p[AcidParam::DIST_TONE] = rng.between(0.32, 0.70);
                p[AcidParam::DIST_MIX] = rng.between(0.78, 1.0);
            }
        }

        // Tempo/swing/tuning draws are consumed to keep the stream identical
        // to the desktop bank; tempo is global in the clip launcher.
        rng.between(116.0, 150.0);
        if !rng.chance(0.55) {
            rng.between(0.04, 0.30);
        }
        if rng.chance(0.10) {
            rng.chance(0.5);
        }

        let mut steps = [Step::default(); STEPS];
        for (i, slot_step) in steps.iter_mut().enumerate() {
            *slot_step = Step {
                gate: gates[i],
                note: notes[i] as u8,
                velocity: if accents[i] { 118 } else { 92 },
                accent: accents[i],
                slide: slides[i],
            };
        }

        let adjective = (slot * 7 + bank * 11) % 64;
        let noun = (slot * 17 + bank * 23) % 64;
        let mut name = format!("{} {}", ADJECTIVES[adjective], NOUNS[noun]);
        if FACTORY_NAMES.contains(&name.as_str()) {
            name.push_str(" II");
        }

        Self::preset_from(name, bank, slot, length, steps, p)
    }

    /// All 512 factory lines, generated on demand.
    pub fn all() -> Vec<AcidPreset> {
        let mut out = Vec::with_capacity(Self::COUNT);
        for bank in 0..Self::BANKS {
            for slot in 0..Self::PER_BANK {
                out.push(Self::preset(bank, slot));
            }
        }
        out
    }
}

fn log_between(rng: &mut FastRandom, low: f32, high: f32) -> f32 {
    (rng.between(low.ln(), high.ln())).exp()
}

#[derive(Clone)]
pub struct DrumPreset {
    pub name: String,
    pub category: String,
    pub accent: f32,
    pub masks: [u16; DRUM_VOICES],
    pub velocities: [[u8; STEPS]; DRUM_VOICES],
}

struct PatternBuilder {
    masks: [u16; DRUM_VOICES],
    velocities: [[u8; STEPS]; DRUM_VOICES],
}

impl PatternBuilder {
    fn new() -> Self {
        Self {
            masks: [0; DRUM_VOICES],
            velocities: [[100; STEPS]; DRUM_VOICES],
        }
    }

    fn hit(&mut self, voice: usize, step: usize, velocity: u8) {
        if voice >= DRUM_VOICES || step >= STEPS {
            return;
        }
        self.masks[voice] |= 1 << step;
        self.velocities[voice][step] = velocity;
    }

    fn every(&mut self, voice: usize, from: usize, to: usize, interval: usize, velocity: u8) {
        let mut step = from;
        while step <= to {
            self.hit(voice, step, velocity);
            step += interval;
        }
    }
}

pub struct DrumPresets;

impl DrumPresets {
    pub fn all() -> Vec<DrumPreset> {
        // BD=0 SD=1 LT=2 MT=3 HT=4 RS=5 CP=6 CH=7 OH=8 CC=9 RC=10
        let mut out = Vec::new();

        let mut b = PatternBuilder::new();
        for step in [0usize, 4, 8, 12] {
            b.hit(0, step, if step % 8 == 0 { 120 } else { 100 });
        }
        b.hit(6, 4, 120);
        b.hit(6, 12, 120);
        for step in [2usize, 6, 10, 14] {
            b.hit(8, step, 118);
        }
        for step in 0..16 {
            if step % 4 != 2 {
                b.hit(7, step, 96);
            }
        }
        b.hit(5, 3, 100);
        b.hit(5, 11, 100);
        out.push(DrumPreset {
            name: "Classic House".into(),
            category: "House".into(),
            accent: 0.75,
            masks: b.masks,
            velocities: b.velocities,
        });

        let mut b = PatternBuilder::new();
        for step in [0usize, 4, 8, 12] {
            b.hit(0, step, 122);
        }
        b.hit(1, 4, 120);
        b.hit(1, 12, 120);
        b.hit(1, 15, 100);
        b.hit(2, 6, 100);
        b.hit(3, 10, 100);
        b.hit(4, 14, 100);
        b.every(10, 0, 15, 2, 96);
        b.hit(10, 2, 118);
        b.hit(10, 10, 118);
        for step in [2usize, 6, 10, 14] {
            b.hit(8, step, 118);
        }
        out.push(DrumPreset {
            name: "Detroit Techno".into(),
            category: "Techno".into(),
            accent: 0.85,
            masks: b.masks,
            velocities: b.velocities,
        });

        let mut b = PatternBuilder::new();
        for step in [0usize, 4, 8, 12] {
            b.hit(0, step, if step % 8 == 0 { 120 } else { 100 });
        }
        b.hit(1, 4, 120);
        b.hit(1, 10, 100);
        b.hit(1, 12, 120);
        for step in [2usize, 7, 14] {
            b.hit(5, step, 100);
        }
        for step in 0..16 {
            b.hit(7, step, if step % 2 == 0 { 100 } else { 118 });
        }
        out.push(DrumPreset {
            name: "Acid Jack".into(),
            category: "Acid".into(),
            accent: 0.80,
            masks: b.masks,
            velocities: b.velocities,
        });

        let mut b = PatternBuilder::new();
        b.every(0, 0, 15, 4, 122);
        b.hit(1, 4, 120);
        b.hit(1, 12, 110);
        b.hit(2, 3, 100);
        b.hit(2, 11, 100);
        b.hit(3, 7, 100);
        b.every(10, 0, 15, 1, 92);
        out.push(DrumPreset {
            name: "Hard Groove".into(),
            category: "Techno".into(),
            accent: 0.90,
            masks: b.masks,
            velocities: b.velocities,
        });

        let mut b = PatternBuilder::new();
        b.hit(0, 0, 122);
        b.hit(0, 6, 100);
        b.hit(0, 8, 100);
        b.hit(0, 10, 118);
        b.hit(6, 4, 120);
        b.hit(6, 12, 120);
        b.hit(5, 2, 100);
        b.hit(5, 14, 100);
        b.every(7, 0, 15, 2, 96);
        out.push(DrumPreset {
            name: "Electro Funk".into(),
            category: "Electro".into(),
            accent: 0.75,
            masks: b.masks,
            velocities: b.velocities,
        });

        let mut b = PatternBuilder::new();
        b.every(0, 0, 15, 4, 124);
        b.hit(1, 4, 120);
        b.hit(1, 12, 120);
        b.hit(1, 14, 100);
        b.hit(9, 0, 120);
        b.hit(9, 8, 100);
        for step in (2..16).step_by(4) {
            b.hit(8, step, 118);
        }
        out.push(DrumPreset {
            name: "Industrial Rave".into(),
            category: "Industrial".into(),
            accent: 0.95,
            masks: b.masks,
            velocities: b.velocities,
        });

        let mut b = PatternBuilder::new();
        b.every(0, 0, 15, 4, 100);
        b.hit(5, 4, 100);
        b.hit(5, 12, 100);
        b.hit(5, 7, 100);
        for step in [2usize, 6, 10, 14] {
            b.hit(7, step, 96);
        }
        out.push(DrumPreset {
            name: "Minimal Techno".into(),
            category: "Minimal".into(),
            accent: 0.60,
            masks: b.masks,
            velocities: b.velocities,
        });

        let mut b = PatternBuilder::new();
        b.hit(0, 0, 120);
        b.hit(0, 8, 120);
        b.hit(1, 4, 120);
        b.hit(1, 12, 120);
        b.hit(6, 4, 100);
        b.hit(6, 12, 100);
        b.every(7, 0, 15, 1, 92);
        b.hit(8, 2, 100);
        b.hit(8, 10, 100);
        b.hit(4, 14, 100);
        b.hit(3, 15, 100);
        out.push(DrumPreset {
            name: "Synthwave 909".into(),
            category: "Synthwave".into(),
            accent: 0.70,
            masks: b.masks,
            velocities: b.velocities,
        });

        out
    }
}

/// Merge preset params while keeping the live ReplicantFX insert and macros.
fn merge_preset_params(clip: &Clip, source: &[f32; PARAMS]) -> [f32; PARAMS] {
    let mut merged = *source;
    merged[FxParam::BASE..PARAMS].copy_from_slice(&clip.params[FxParam::BASE..PARAMS]);
    merged
}

impl Clip {
    pub fn apply_acid_preset(&mut self, preset: &AcidPreset) {
        self.kind = ClipKind::Acid;
        self.name = preset.name.clone();
        self.root = preset.root;
        self.length = preset.length;
        self.params = merge_preset_params(self, &preset.params);
        self.steps = preset.steps;
        self.midi_notes.clear();
        self.sample_missing = false;
    }

    pub fn apply_drum_preset(&mut self, preset: &DrumPreset) {
        self.kind = ClipKind::Drums;
        self.name = preset.name.clone();
        self.root = 36;
        self.length = STEPS as i32;
        let mut p = [0.0f32; PARAMS];
        p[DrumParam::TUNE] = 0.5;
        p[DrumParam::DECAY] = 0.5;
        p[DrumParam::SNAPPY] = 0.5;
        p[DrumParam::TONE] = 0.5;
        p[DrumParam::ACCENT] = preset.accent;
        p[DrumParam::LEVEL] = 0.85;
        p[DrumParam::CUTOFF] = 18000.0;
        p[DrumParam::RESONANCE] = 0.6;
        self.params = merge_preset_params(self, &p);
        self.steps = [Step::default(); STEPS];
        self.midi_notes.clear();
        self.drum_mask = preset.masks;
        self.drum_vel = preset.velocities;
        self.sample_missing = false;
    }

    pub fn apply_wave_preset(&mut self, preset: &super::wave_presets::WavePreset) {
        self.kind = ClipKind::Wave;
        self.name = preset.name.to_string();
        self.root = preset.base_note;
        self.length = STEPS as i32;
        self.params = merge_preset_params(self, &preset.params);
        self.steps = [Step::default(); STEPS];
        self.midi_notes = preset.notes.to_vec();
        self.sample_missing = false;
    }
}

/// MIDI-note helper used by presets and the piano roll.
pub fn note(note: i32, start: i32, length: i32, velocity: i32) -> MidiNote {
    MidiNote::new(note, start, length, velocity)
}
