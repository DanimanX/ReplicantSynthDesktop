//! Engine wiring: shared handles, the clip engine and the 24-bit recorder.

pub mod clip_engine;
pub mod handles;
pub mod recorder;

pub use clip_engine::{
    Clip, ClipEngine, ClipStep, MidiSlot, COLS, DRUM_VOICES, FX_ENABLED_INDEX, FX_PARAM_BASE,
    MAX_MIDI_NOTES, PARAMS, ROWS, STEPS,
};
pub use handles::{
    Command, CommandType, EngineHandles, EngineShared, PadTelemetry, SampleData, SampleSlots,
    Telemetry, PADS, QUEUE_CAPACITY,
};
pub use recorder::{RecordTap, RecordingWriter};

use crate::model::{Clip as ModelClip, ClipKind, ProjectState};
use std::sync::Arc;

/// Create the audio-thread engine and the UI-side handle to it.
pub fn create_engine(sample_rate: f64, max_block_frames: usize) -> (ClipEngine, EngineHandles) {
    let shared = Arc::new(EngineShared::new());
    let mut engine = ClipEngine::new(shared.clone());
    engine.prepare(sample_rate, max_block_frames);
    let handles = EngineHandles::new(shared);
    (engine, handles)
}

/// Replay one UI clip into the engine (full state: kind, params, pattern).
pub fn sync_clip(handles: &EngineHandles, clip: &ModelClip) {
    handles.configure_clip(
        clip.index,
        clip.kind.ordinal(),
        clip.root,
        clip.length,
        clip.gain,
        clip.pan,
        clip.fx_type,
        clip.fx_enabled,
        clip.sample_loop,
        clip.mute,
        clip.solo,
        &clip.params,
    );

    // Patterns are sent after the configure command so the engine's defaults
    // for a freshly changed kind never leak through.
    for (step, data) in clip.steps.iter().enumerate() {
        handles.set_step(
            clip.index,
            step,
            data.gate,
            data.note as i32,
            data.velocity as i32,
            data.accent,
            data.slide,
        );
    }
    for voice in 0..DRUM_VOICES {
        for step in 0..STEPS {
            let on = clip.drum_on(voice, step);
            if on {
                handles.set_drum_step(
                    clip.index,
                    voice,
                    step,
                    true,
                    clip.drum_velocity(voice, step) as i32,
                );
            }
        }
    }
    handles.clear_midi_notes(clip.index);
    if clip.kind == ClipKind::Wave {
        for note in &clip.midi_notes {
            handles.set_midi_note(
                clip.index,
                true,
                note.note as i32,
                note.start as i32,
                note.length as i32,
                note.velocity as i32,
            );
        }
    }
}

/// Replay a whole project, including transport settings.
pub fn sync_project(handles: &EngineHandles, project: &ProjectState) {
    handles.shared().set_tempo(project.bpm);
    handles.shared().set_quantize(project.quantize);
    handles.shared().set_swing(project.swing);
    handles.shared().set_master_gain(project.master_gain);
    for clip in &project.clips {
        sync_clip(handles, clip);
    }
}
