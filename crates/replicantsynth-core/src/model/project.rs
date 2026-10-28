//! Project document: tempo, launch quantization, master gain and 64 clips.

use super::clip::{apply_kind_defaults, Clip, ClipKind, PADS, QUANTIZE_OPTIONS};

#[derive(Clone, Debug)]
pub struct ProjectState {
    pub name: String,
    pub bpm: f32,
    pub quantize: i32,
    pub swing: f32,
    pub master_gain: f32,
    pub clips: Vec<Clip>,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectState {
    pub fn new() -> Self {
        Self {
            name: "NEON GRID".to_string(),
            bpm: 120.0,
            quantize: 16,
            swing: 0.0,
            master_gain: 0.85,
            clips: (0..PADS).map(Clip::new).collect(),
        }
    }

    pub fn clip(&self, index: usize) -> Option<&Clip> {
        self.clips.get(index)
    }

    pub fn clip_mut(&mut self, index: usize) -> Option<&mut Clip> {
        self.clips.get_mut(index)
    }

    pub fn set_kind(&mut self, index: usize, kind: ClipKind) {
        if let Some(clip) = self.clips.get_mut(index) {
            apply_kind_defaults(clip, kind);
        }
    }

    pub fn has_any_clip(&self) -> bool {
        self.clips.iter().any(|c| c.kind != ClipKind::Empty)
    }

    pub fn next_quantize(&mut self) {
        self.quantize = next_quantize(self.quantize);
    }

    /// Clamp every value into the ranges the engine accepts.
    pub fn sanitize(&mut self) {
        self.bpm = self.bpm.clamp(30.0, 300.0);
        self.swing = self.swing.clamp(0.0, 0.66);
        self.master_gain = self.master_gain.clamp(0.0, 1.5);
        if !QUANTIZE_OPTIONS.contains(&self.quantize) {
            self.quantize = 16;
        }
        for clip in self.clips.iter_mut() {
            clip.length = clip.length.clamp(1, 16);
            clip.gain = clip.gain.clamp(0.0, 1.5);
            clip.pan = clip.pan.clamp(-1.0, 1.0);
        }
    }
}

pub fn next_quantize(current: i32) -> i32 {
    let index = QUANTIZE_OPTIONS
        .iter()
        .position(|q| *q == current)
        .unwrap_or(3);
    QUANTIZE_OPTIONS[(index + 1) % QUANTIZE_OPTIONS.len()]
}

pub fn quantize_label(steps: i32) -> &'static str {
    match steps {
        0 => "OFF",
        1 => "1/16",
        4 => "1/4",
        16 => "1 BAR",
        32 => "2 BARS",
        64 => "4 BARS",
        _ => "1 BAR",
    }
}
