//! Project persistence: one JSON document is the single source of truth for
//! the 64 slots, and the app replays it into the engine on load.

use super::clip::{Clip, ClipKind, MidiNote, Step, DRUM_VOICES, PADS, PARAMS, STEPS};
use super::project::ProjectState;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

pub const FILE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile {
    version: u32,
    name: String,
    bpm: f32,
    quantize: i32,
    swing: f32,
    master_gain: f32,
    clips: Vec<ClipFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipFile {
    kind: ClipKind,
    #[serde(default)]
    name: String,
    #[serde(default = "default_root")]
    root: i32,
    #[serde(default = "default_length")]
    length: i32,
    #[serde(default = "default_gain")]
    gain: f32,
    #[serde(default)]
    pan: f32,
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    solo: bool,
    #[serde(default = "default_true")]
    sample_loop: bool,
    #[serde(default)]
    sample_name: Option<String>,
    #[serde(default)]
    sample_path: Option<String>,
    #[serde(default)]
    sample_missing: bool,
    #[serde(default)]
    fx_type: usize,
    #[serde(default)]
    fx_enabled: bool,
    #[serde(default)]
    params: Vec<f32>,
    #[serde(default)]
    steps: Vec<Step>,
    #[serde(default)]
    midi_notes: Vec<MidiNote>,
    #[serde(default)]
    drum_mask: Vec<u16>,
    #[serde(default)]
    drum_vel: Vec<Vec<u8>>,
}

fn default_root() -> i32 {
    60
}
fn default_length() -> i32 {
    STEPS as i32
}
fn default_gain() -> f32 {
    0.8
}
fn default_true() -> bool {
    true
}

impl ClipFile {
    fn from_clip(clip: &Clip) -> Self {
        Self {
            kind: clip.kind,
            name: clip.name.clone(),
            root: clip.root,
            length: clip.length,
            gain: clip.gain,
            pan: clip.pan,
            mute: clip.mute,
            solo: clip.solo,
            sample_loop: clip.sample_loop,
            sample_name: clip.sample_name.clone(),
            sample_path: clip.sample_path.clone(),
            sample_missing: clip.sample_missing,
            fx_type: clip.fx_type,
            fx_enabled: clip.fx_enabled,
            params: clip.params.to_vec(),
            steps: clip.steps.to_vec(),
            midi_notes: clip.midi_notes.clone(),
            drum_mask: clip.drum_mask.to_vec(),
            drum_vel: clip.drum_vel.iter().map(|row| row.to_vec()).collect(),
        }
    }

    fn into_clip(self, index: usize) -> Clip {
        let mut clip = Clip::new(index);
        clip.kind = self.kind;
        clip.name = self.name;
        clip.root = self.root;
        clip.length = self.length;
        clip.gain = self.gain;
        clip.pan = self.pan;
        clip.mute = self.mute;
        clip.solo = self.solo;
        clip.sample_loop = self.sample_loop;
        clip.sample_name = self.sample_name;
        clip.sample_path = self.sample_path;
        clip.sample_missing = self.sample_missing;
        clip.fx_type = self.fx_type.min(crate::dsp::FX_COUNT - 1);
        clip.fx_enabled = self.fx_enabled;
        for (i, value) in self.params.iter().take(PARAMS).enumerate() {
            clip.params[i] = *value;
        }
        for (i, step) in self.steps.iter().take(STEPS).enumerate() {
            clip.steps[i] = *step;
        }
        clip.midi_notes = self.midi_notes;
        for (i, mask) in self.drum_mask.iter().take(DRUM_VOICES).enumerate() {
            clip.drum_mask[i] = *mask;
        }
        for (voice, row) in self.drum_vel.iter().take(DRUM_VOICES).enumerate() {
            for (step, value) in row.iter().take(STEPS).enumerate() {
                clip.drum_vel[voice][step] = *value;
            }
        }
        clip
    }
}

pub fn encode(project: &ProjectState) -> String {
    let file = ProjectFile {
        version: FILE_VERSION,
        name: project.name.clone(),
        bpm: project.bpm,
        quantize: project.quantize,
        swing: project.swing,
        master_gain: project.master_gain,
        clips: project.clips.iter().map(ClipFile::from_clip).collect(),
    };
    serde_json::to_string_pretty(&file).unwrap_or_else(|_| "{}".to_string())
}

pub fn decode(text: &str) -> Result<ProjectState, serde_json::Error> {
    let file: ProjectFile = serde_json::from_str(text)?;
    let mut project = ProjectState::new();
    project.name = file.name;
    project.bpm = file.bpm;
    project.quantize = file.quantize;
    project.swing = file.swing;
    project.master_gain = file.master_gain;
    project.clips = file
        .clips
        .into_iter()
        .enumerate()
        .take(PADS)
        .map(|(index, clip)| clip.into_clip(index))
        .collect();
    while project.clips.len() < PADS {
        project.clips.push(Clip::new(project.clips.len()));
    }
    project.sanitize();
    Ok(project)
}

pub fn save(project: &ProjectState, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, encode(project))?;
    fs::rename(&temp, path).or_else(|_| {
        fs::copy(&temp, path)?;
        fs::remove_file(&temp)
    })
}

pub fn load(path: &Path) -> io::Result<ProjectState> {
    let text = fs::read_to_string(path)?;
    decode(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}
