//! Clip and project state owned by the UI.
//!
//! The engine keeps its own real-time copy; this module is the document the
//! user edits, saves and replays into the engine through commands.

pub mod clip;
pub mod demo;
pub mod presets;
pub mod project;
pub mod store;
pub mod wave_presets;

pub use clip::{
    apply_kind_defaults, default_name, AcidParam, Clip, ClipKind, ClipLimits, ClipState, DrumParam,
    FxParam, MidiNote, PluginFormat, PluginRef, SampleParam, Step, WaveParam, CHORD_MODES, COLS,
    DISTORTION_TYPES, DRUM_VOICES, DRUM_VOICE_NAMES, FILTER_MODES, MAX_MIDI_NOTES, PADS, PARAMS,
    QUANTIZE_OPTIONS, ROWS, STEPS, WAVETABLE_BANKS,
};
pub use demo::demo_project;
pub use presets::{AcidPreset, AcidPresets, DrumPreset, DrumPresets};
pub use project::{quantize_label, ProjectState};
pub use wave_presets::{wave_presets, WavePreset, WAVE_PRESETS};
