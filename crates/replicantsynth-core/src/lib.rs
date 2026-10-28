//! ReplicantSynth engine.
//!
//! A pure-Rust port of the ReplicantSynth DSP core: an 8 x 8 Ableton-style
//! clip launcher that unites Replicant303, ReplicantWave, Replicant909 and the
//! 48-slot ReplicantFX engine.
//!
//! The crate is platform agnostic: it never opens an audio device, a MIDI port
//! or a window. `replicantsynth-desktop` does that and drives this engine.

pub mod dsp;
pub mod engine;
pub mod model;

pub use engine::{
    create_engine, sync_clip, sync_project, ClipEngine, Command, EngineHandles, EngineShared,
    NoteEvent, PadTelemetry, PluginBlock, PluginBridge, RecordTap, RecordingWriter, SampleData,
    SampleSlots, PLUGIN_BLOCK_FRAMES, STEPS,
};
pub use model::{Clip, ClipKind, ClipState, ProjectState};

/// Crate version, matching the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
