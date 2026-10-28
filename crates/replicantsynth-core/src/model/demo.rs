//! The factory demo: six scenes that exercise the 303, the 909, the wavetable
//! engine and an imported sample, each with a ReplicantFX insert. It is built
//! on first launch so the grid is playable before the user imports anything.

use super::clip::{
    apply_kind_defaults, AcidParam, ClipKind, DrumParam, MidiNote, SampleParam, Step, WaveParam,
    DRUM_VOICES, STEPS,
};
use super::project::ProjectState;

pub fn demo_project(sample_path: Option<&str>) -> ProjectState {
    let mut project = ProjectState::new();
    project.name = "NEON GRID".to_string();
    project.bpm = 126.0;
    project.quantize = 16;
    project.master_gain = 0.85;
    project.swing = 0.0;

    acid(&mut project, 0, 36, 1, "ACID BASS", -0.05);
    drums(&mut project, 1, 3, "909 KIT", 0.0, false);
    wave(&mut project, 2, 48, 0, 0, 15, 2100.0, 0.0, "NEON PAD");

    acid(&mut project, 8, 41, 5, "ACID LEAD", 0.05);
    drums(&mut project, 9, 12, "909 RESO", -0.1, true);
    wave(&mut project, 10, 53, 4, 2, 18, 2600.0, -0.2, "ACID ARP");

    sample(&mut project, 16, 10, -0.15, sample_path, "NEON VOX");
    acid(&mut project, 17, 36, 34, "FILTER BASS", 0.0);
    drums(&mut project, 18, 31, "VINYL 909", 0.1, true);

    wave(&mut project, 24, 55, 2, 3, 6, 3200.0, 0.2, "FM LEAD");
    acid(&mut project, 25, 43, 8, "DIST ACID", -0.2);
    drums(&mut project, 26, 2, "DELAY KIT", 0.2, true);

    wave(&mut project, 32, 48, 3, 4, 44, 1700.0, -0.25, "VOX PAD");
    acid(&mut project, 33, 36, 33, "ZAN-ZOU", 0.2);

    acid(&mut project, 40, 48, 13, "LOOP ACID", 0.0);
    drums(&mut project, 41, 8, "FULL KIT", -0.1, true);
    wave(&mut project, 42, 60, 7, 3, 14, 3600.0, 0.25, "SUB LEAD");
    sample(&mut project, 43, 47, 0.15, sample_path, "VOX CLOUD");

    project
}

fn base(project: &mut ProjectState, index: usize, kind: ClipKind, name: &str, gain: f32, pan: f32) {
    let clip = &mut project.clips[index];
    apply_kind_defaults(clip, kind);
    clip.name = name.to_string();
    clip.gain = gain;
    clip.pan = pan;
}

fn acid(project: &mut ProjectState, index: usize, root: i32, fx: usize, name: &str, pan: f32) {
    {
        base(project, index, ClipKind::Acid, name, 0.78, pan);
        let clip = &mut project.clips[index];
        clip.root = root;
        let notes = [0, 0, 12, 3, 7, 10, 12, 7];
        let gates = [0usize, 3, 6, 8, 10, 11, 14, 15];
        clip.steps = [Step::default(); STEPS];
        for (i, step) in gates.iter().enumerate() {
            let accent = i == 0 || i == 3 || i == 5;
            clip.steps[*step] = Step {
                gate: true,
                note: (root + notes[i]).clamp(0, 127) as u8,
                velocity: if accent { 118 } else { 94 },
                accent,
                slide: i == 6,
            };
        }
    }
    project.clips[index].apply_fx_defaults(fx);
}

#[allow(clippy::too_many_arguments)]
fn wave(
    project: &mut ProjectState,
    index: usize,
    root: i32,
    bank: i32,
    chord: i32,
    fx: usize,
    cutoff: f32,
    pan: f32,
    name: &str,
) {
    {
        base(project, index, ClipKind::Wave, name, 0.62, pan);
        let clip = &mut project.clips[index];
        clip.root = root;
        let p = &mut clip.params;
        p[WaveParam::BANK] = bank as f32;
        p[WaveParam::POSITION] = 0.3;
        p[WaveParam::UNISON] = 4.0;
        p[WaveParam::DETUNE] = 16.0;
        p[WaveParam::SPREAD] = 0.55;
        p[WaveParam::CUTOFF] = cutoff;
        p[WaveParam::RESONANCE] = 0.7;
        p[WaveParam::DRIVE] = 1.3;
        p[WaveParam::ATTACK] = 0.02;
        p[WaveParam::DECAY] = 0.5;
        p[WaveParam::SUSTAIN] = 0.65;
        p[WaveParam::RELEASE] = 0.55;
        p[WaveParam::SUB] = 0.2;
        p[WaveParam::NOISE] = 0.02;
        p[WaveParam::LEVEL] = 0.72;
        p[WaveParam::FILTER_MODE] = 0.0;
        p[WaveParam::CHORD] = chord as f32;
        // Wave clips are MIDI clips: seed the piano roll with a phrase.
        clip.steps = [Step::default(); STEPS];
        clip.midi_notes = [0, 3, 6, 10, 14]
            .iter()
            .map(|step| MidiNote::new((root + (step % 7)).clamp(0, 127), *step, 2, 100))
            .collect();
    }
    project.clips[index].apply_fx_defaults(fx);
}

fn drums(project: &mut ProjectState, index: usize, fx: usize, name: &str, pan: f32, busy: bool) {
    {
        base(project, index, ClipKind::Drums, name, 0.85, pan);
        let clip = &mut project.clips[index];
        let p = &mut clip.params;
        p[DrumParam::TUNE] = 0.52;
        p[DrumParam::DECAY] = 0.5;
        p[DrumParam::SNAPPY] = 0.52;
        p[DrumParam::TONE] = 0.55;
        p[DrumParam::ACCENT] = 0.7;
        p[DrumParam::LEVEL] = 0.9;
        p[DrumParam::CUTOFF] = 18000.0;
        p[DrumParam::RESONANCE] = 0.6;

        clip.drum_mask = [0; DRUM_VOICES];
        clip.drum_vel = [[100; STEPS]; DRUM_VOICES];
        for step in [0usize, 4, 8, 12] {
            clip.drum_mask[0] |= 1 << step;
            clip.drum_vel[0][step] = 122;
        }
        for step in [4usize, 12] {
            clip.drum_mask[1] |= 1 << step;
            clip.drum_vel[1][step] = 112;
        }
        for step in (0..STEPS).step_by(2) {
            clip.drum_mask[7] |= 1 << step;
            clip.drum_vel[7][step] = if step % 4 == 0 { 92 } else { 70 };
        }
        if busy {
            for step in [6usize, 14] {
                clip.drum_mask[8] |= 1 << step;
                clip.drum_vel[8][step] = 88;
            }
            clip.drum_mask[6] |= 1 << 11;
            clip.drum_vel[6][11] = 96;
            clip.drum_mask[9] |= 1 << 0;
            clip.drum_vel[9][0] = 90;
        }
    }
    project.clips[index].apply_fx_defaults(fx);
}

fn sample(
    project: &mut ProjectState,
    index: usize,
    fx: usize,
    pan: f32,
    sample_path: Option<&str>,
    name: &str,
) {
    {
        base(project, index, ClipKind::Sample, name, 0.75, pan);
        let clip = &mut project.clips[index];
        clip.params[SampleParam::GAIN] = 0.8;
        clip.params[SampleParam::PITCH] = 0.0;
        clip.sample_path = sample_path.map(|s| s.to_string());
        clip.sample_name = Some(if sample_path.is_some() {
            "NEON VOX".to_string()
        } else {
            "NO SAMPLE".to_string()
        });
        clip.sample_missing = sample_path.is_none();
        clip.sample_loop = true;
    }
    project.clips[index].apply_fx_defaults(fx);
}

/// Small helper for the UI: which pads the demo fills.
pub fn demo_pads() -> Vec<usize> {
    vec![
        0, 1, 2, 8, 9, 10, 16, 17, 18, 24, 25, 26, 32, 33, 40, 41, 42, 43,
    ]
}

/// Kept for parity with the Android model's parameter constants.
pub fn acid_param_names() -> [&'static str; 11] {
    [
        "WAVE",
        "CUTOFF",
        "RESONANCE",
        "ENV MOD",
        "DECAY",
        "ACCENT",
        "DIST",
        "DRIVE",
        "MIX",
        "TONE",
        "VOLUME",
    ]
}

pub const ACID_PARAM_ORDER: [usize; 11] = [
    AcidParam::WAVE,
    AcidParam::CUTOFF,
    AcidParam::RESONANCE,
    AcidParam::ENV_MOD,
    AcidParam::DECAY,
    AcidParam::ACCENT,
    AcidParam::DIST_TYPE,
    AcidParam::DIST_DRIVE,
    AcidParam::DIST_MIX,
    AcidParam::DIST_TONE,
    AcidParam::VOLUME,
];
