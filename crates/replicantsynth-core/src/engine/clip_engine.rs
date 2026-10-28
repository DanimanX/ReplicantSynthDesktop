//! The 64-pad clip engine: transport, quantized launching, scenes, per-pad
//! instruments and ReplicantFX inserts, sample playback and capture.
//!
//! Everything in this file runs on the audio thread: no allocation, no locks,
//! no file I/O and no callbacks into the UI.

use super::handles::{Command, CommandType, EngineShared, PadTelemetry, PADS};
use super::plugin::{NoteEvent, PluginBridge, PluginHostCommand};
use crate::dsp::drums::midi_note_to_instrument;
use crate::dsp::{
    clampf, clampi, sanitize, shared_wavetable_banks, soft_limit, Acid303, DrumKit909, Effect,
    WaveParams, WaveSynth, FX_COUNT,
};
use crate::model::ClipKind;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub const STEPS: usize = 16;
pub const DRUM_VOICES: usize = 11;
pub const PARAMS: usize = 32;
pub const MAX_MIDI_NOTES: usize = 96;
pub const ROWS: usize = 8;
pub const COLS: usize = 8;
pub const FX_PARAM_BASE: usize = 24;
pub const FX_ENABLED_INDEX: usize = 30;

const FX_ID_BYPASS: usize = 0;

#[derive(Clone, Copy, Debug)]
pub struct ClipStep {
    pub gate: bool,
    pub note: u8,
    pub velocity: u8,
    pub accent: bool,
    pub slide: bool,
}

impl Default for ClipStep {
    fn default() -> Self {
        Self {
            gate: false,
            note: 60,
            velocity: 100,
            accent: false,
            slide: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MidiSlot {
    pub active: bool,
    pub note: u8,
    pub velocity: u8,
    pub start: u8,
    pub length: u8,
}

/// The real-time copy of one pad.
pub struct Clip {
    pub kind: i32,
    pub root: i32,
    pub length: i32,
    pub gain: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub fx_type: usize,
    pub fx_enabled: bool,
    pub sample_loop: bool,
    pub params: [f32; PARAMS],

    pub steps: [ClipStep; STEPS],
    pub midi: [MidiSlot; MAX_MIDI_NOTES],
    pub drum_mask: [u16; DRUM_VOICES],
    pub drum_vel: [[u8; STEPS]; DRUM_VOICES],

    state: i32,
    phase: i32,
    pending_start: i64,
    pending_stop: i64,
    last_note: i32,
    sample_pos: f64,
    tail_samples: f64,

    acid: Acid303,
    wave: WaveSynth,
    drums: DrumKit909,

    /// Set for `ClipKind::Plugin`: the bridge to the plugin worker thread.
    pub plugin: Option<PluginBridge>,
    /// Notes currently sounding on the hosted plugin, so a stop can release them.
    plugin_notes: u128,
}

impl Clip {
    fn new() -> Self {
        Self {
            kind: 0,
            root: 60,
            length: STEPS as i32,
            gain: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            fx_type: 0,
            fx_enabled: false,
            sample_loop: true,
            params: [0.0; PARAMS],
            steps: [ClipStep::default(); STEPS],
            midi: [MidiSlot::default(); MAX_MIDI_NOTES],
            drum_mask: [0; DRUM_VOICES],
            drum_vel: [[0; STEPS]; DRUM_VOICES],
            state: 0,
            phase: 0,
            pending_start: -1,
            pending_stop: -1,
            last_note: -1,
            sample_pos: 0.0,
            tail_samples: 0.0,
            acid: Acid303::new(),
            wave: WaveSynth::new(),
            drums: DrumKit909::new(),
            plugin: None,
            plugin_notes: 0,
        }
    }

    pub fn state(&self) -> i32 {
        self.state
    }

    /// The six ReplicantFX macros, read from the shared parameter block.
    pub fn fx_params(&self) -> [f32; 6] {
        let mut out = [0.0; 6];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.params[FX_PARAM_BASE + i];
        }
        out
    }
}

#[inline]
fn balance_l(pan: f32) -> f32 {
    if pan <= 0.0 {
        1.0
    } else {
        1.0 - pan
    }
}

#[inline]
fn balance_r(pan: f32) -> f32 {
    if pan >= 0.0 {
        1.0
    } else {
        1.0 + pan
    }
}

const DEFAULT_ACID_NOTES: [i32; 6] = [0, 0, 12, 0, 7, 12];
const DEFAULT_WAVE_NOTES: [i32; 4] = [0, 3, 7, 12];
const DEFAULT_ACID_STEPS: [usize; 6] = [0, 3, 6, 8, 11, 14];
const DEFAULT_WAVE_STEPS: [usize; 4] = [0, 4, 8, 12];

/// The engine. Own it on the audio thread; drive it through `EngineHandles`.
pub struct ClipEngine {
    shared: Arc<EngineShared>,
    sample_rate: f64,
    max_block_frames: usize,
    transport_running: bool,
    absolute_step: i64,
    samples_into_step: f64,
    samples_per_step: f64,
    step_duration: f64,
    clips: Vec<Clip>,
    fx: Vec<Effect>,
    pad_peak_block: [f32; PADS],
    silent_buffer: Vec<f32>,
    pad_capture: Vec<f32>,
    plugin_rx: Option<crossbeam_channel::Receiver<PluginHostCommand>>,
}

impl ClipEngine {
    pub fn new(shared: Arc<EngineShared>) -> Self {
        shared_wavetable_banks();
        let plugin_rx = shared.take_plugin_receiver();
        let fx = (0..PADS).map(|_| Effect::new()).collect();
        let clips = (0..PADS).map(|_| Clip::new()).collect();
        let mut engine = Self {
            shared,
            sample_rate: 48_000.0,
            max_block_frames: 512,
            transport_running: false,
            absolute_step: 0,
            samples_into_step: 0.0,
            samples_per_step: 6_000.0,
            step_duration: 6_000.0,
            clips,
            fx,
            pad_peak_block: [0.0; PADS],
            silent_buffer: vec![0.0; 1024],
            pad_capture: vec![0.0; 1024],
            plugin_rx,
        };
        engine.prepare(48_000.0, 512);
        engine
    }

    pub fn prepare(&mut self, sample_rate: f64, max_block_frames: usize) {
        self.sample_rate = sample_rate.max(8_000.0);
        self.max_block_frames = max_block_frames.max(64);
        self.samples_per_step = self.sample_rate * 60.0 / self.shared.tempo() as f64 / 4.0;
        self.step_duration = self.samples_per_step;

        for clip in self.clips.iter_mut() {
            clip.acid.prepare(self.sample_rate);
            clip.wave.prepare(self.sample_rate);
            clip.drums.prepare(self.sample_rate);
            clip.state = 0;
            clip.pending_start = -1;
            clip.pending_stop = -1;
            clip.phase = 0;
            clip.tail_samples = 0.0;
            clip.sample_pos = 0.0;
            clip.last_note = -1;
        }
        for effect in self.fx.iter_mut() {
            effect.prepare(self.sample_rate as f32);
        }

        self.silent_buffer = vec![0.0; self.max_block_frames * 2];
        self.pad_capture = vec![0.0; self.max_block_frames * 2];
        self.transport_running = false;
        self.absolute_step = 0;
        self.samples_into_step = 0.0;
        self.pad_peak_block = [0.0; PADS];
        self.shared
            .sample_rate
            .store(self.sample_rate as u32, Ordering::Relaxed);
        self.shared
            .max_block
            .store(self.max_block_frames as u32, Ordering::Relaxed);
        for pad in 0..PADS {
            self.update_pad_state(pad);
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }

    pub fn handles(&self) -> &Arc<EngineShared> {
        &self.shared
    }

    // -----------------------------------------------------------------------
    // Commands
    // -----------------------------------------------------------------------

    fn drain_commands(&mut self) {
        self.drain_plugin_commands();
        while let Some(command) = self.shared.commands.pop() {
            if (command.pad as usize) >= PADS {
                continue;
            }
            let pad = command.pad as usize;
            match command.kind {
                CommandType::Configure => self.apply_configure(pad, &command),
                CommandType::Params => self.apply_params(pad, &command),
                CommandType::Step => {
                    let clip = &mut self.clips[pad];
                    let step = &mut clip.steps[clampi(command.step as i32, 0, 15) as usize];
                    step.gate = command.flags & 1 != 0;
                    step.accent = command.flags & 2 != 0;
                    step.slide = command.flags & 4 != 0;
                    step.note = command.note as u8;
                    step.velocity = command.velocity as u8;
                }
                CommandType::DrumStep => {
                    let clip = &mut self.clips[pad];
                    let voice = clampi(command.voice as i32, 0, DRUM_VOICES as i32 - 1) as usize;
                    let step = clampi(command.step as i32, 0, 15) as usize;
                    let bit = 1u16 << step;
                    if command.flags & 1 != 0 {
                        clip.drum_mask[voice] |= bit;
                        clip.drum_vel[voice][step] = command.velocity as u8;
                    } else {
                        clip.drum_mask[voice] &= !bit;
                    }
                }
                CommandType::MidiNote => self.apply_midi_note(pad, &command),
                CommandType::ClearMidi => {
                    let clip = &mut self.clips[pad];
                    for note in clip.midi.iter_mut() {
                        note.active = false;
                    }
                    if clip.state == 2 {
                        clip.wave.all_notes_off();
                    }
                }
                CommandType::Preview => {
                    let clip = &mut self.clips[pad];
                    let on = command.flags & 1 != 0;
                    if on && clip.state != 2 {
                        clip.tail_samples = clip.tail_samples.max(self.sample_rate * 1.5);
                    }
                    match ClipKind::from_ordinal(clip.kind) {
                        ClipKind::Wave => {
                            if on {
                                clip.wave
                                    .note_on(command.note as i32, command.velocity as i32);
                            } else {
                                clip.wave.note_off(command.note as i32);
                            }
                        }
                        ClipKind::Acid => {
                            if on {
                                clip.acid
                                    .note_on(command.note as i32, command.velocity as i32);
                            } else {
                                clip.acid.note_off(command.note as i32);
                            }
                        }
                        ClipKind::Drums if on => {
                            if let Some(voice) = midi_note_to_instrument(command.note as i32) {
                                clip.drums
                                    .trigger(voice, command.velocity as f32 / 127.0, 0.0);
                            }
                        }
                        _ => {}
                    }
                }
                CommandType::Launch => {
                    self.launch_clip(pad);
                }
                CommandType::Stop => {
                    if self.clips[pad].state == 0 {
                        continue;
                    }
                    let quant = self.shared.quantize();
                    let boundary = self.next_boundary(quant);
                    let transport_running = self.transport_running;
                    let sample_rate = self.sample_rate;
                    let clip = &mut self.clips[pad];
                    if !transport_running || quant == 0 {
                        stop_clip(clip, false, sample_rate);
                    } else {
                        clip.pending_stop = boundary;
                        clip.pending_start = -1;
                        clip.state = 3;
                    }
                    self.update_pad_state(pad);
                }
                CommandType::LaunchScene | CommandType::StopScene => {
                    self.apply_scene(
                        command.pad as usize,
                        command.kind == CommandType::LaunchScene,
                    );
                }
                CommandType::StopAll => {
                    let quant = self.shared.quantize();
                    let immediate = !self.transport_running || quant == 0;
                    let target = if self.transport_running {
                        self.next_boundary(quant)
                    } else {
                        0
                    };
                    for pad in 0..PADS {
                        if self.clips[pad].state == 0 {
                            continue;
                        }
                        if immediate {
                            stop_clip(&mut self.clips[pad], false, self.sample_rate);
                        } else {
                            let clip = &mut self.clips[pad];
                            clip.pending_stop = target;
                            clip.pending_start = -1;
                            clip.state = 3;
                        }
                        self.update_pad_state(pad);
                    }
                }
                CommandType::Clear => {
                    self.clear_pad(pad);
                }
                CommandType::Panic => {
                    self.stop_transport();
                }
            }
        }
    }

    /// Attach or detach hosted plugins. Bridges that are replaced are handed
    /// back to the UI thread so the plugin itself is torn down there.
    fn drain_plugin_commands(&mut self) {
        let Some(receiver) = &self.plugin_rx else {
            return;
        };
        while let Ok(command) = receiver.try_recv() {
            match command {
                PluginHostCommand::Install { pad, bridge } => {
                    let pad = pad as usize;
                    let bridge = *bridge;
                    if pad >= PADS {
                        self.shared.retire_bridge(bridge);
                        continue;
                    }
                    if let Some(old) = self.clips[pad].plugin.replace(bridge) {
                        self.shared.retire_bridge(old);
                    }
                    self.clips[pad].plugin_notes = 0;
                }
                PluginHostCommand::Remove { pad } => {
                    let pad = pad as usize;
                    if pad < PADS {
                        if let Some(old) = self.clips[pad].plugin.take() {
                            self.shared.retire_bridge(old);
                        }
                        self.clips[pad].plugin_notes = 0;
                    }
                }
            }
        }
    }

    fn launch_clip(&mut self, pad: usize) {
        let kind = self.clips[pad].kind;
        if kind == 0 {
            return;
        }
        let is_sample = kind == ClipKind::Sample.ordinal();
        if is_sample && self.shared.samples.get(pad).is_none() {
            return;
        }

        let quant = self.shared.quantize();
        let boundary = self.next_boundary(quant);
        let transport_running = self.transport_running;
        let sample_rate = self.sample_rate;
        let clip = &mut self.clips[pad];

        if !transport_running {
            self.shared.playing.store(true, Ordering::Release);
            clip.pending_start = 0;
            clip.pending_stop = -1;
            clip.state = 1;
        } else if quant == 0 {
            if clip.state == 2 {
                stop_clip(clip, false, sample_rate);
            } else {
                start_clip(clip);
                trigger_step(clip, 0);
                clip.phase = if clip.length > 0 { 1 % clip.length } else { 0 };
            }
        } else if clip.state == 2 || clip.pending_stop >= 0 {
            clip.pending_stop = boundary;
            clip.pending_start = -1;
            clip.state = 3;
        } else if clip.pending_start >= 0 {
            clip.pending_start = -1;
            clip.pending_stop = -1;
            clip.state = 0;
        } else {
            clip.pending_start = boundary;
            clip.pending_stop = -1;
            clip.state = 1;
        }
        self.update_pad_state(pad);
    }

    fn apply_scene(&mut self, row: usize, launch: bool) {
        let row = row.min(ROWS - 1);
        if launch && !self.transport_running {
            self.shared.playing.store(true, Ordering::Release);
        }
        let quant = self.shared.quantize();
        let immediate = if launch {
            self.transport_running && quant == 0
        } else {
            !self.transport_running || quant == 0
        };
        let target = if self.transport_running && !immediate {
            self.next_boundary(quant)
        } else {
            0
        };

        for pad in 0..PADS {
            let in_row = pad / COLS == row;
            if launch && in_row {
                let clip = &mut self.clips[pad];
                if clip.kind == 0 {
                    continue;
                }
                if clip.kind == ClipKind::Sample.ordinal() && self.shared.samples.get(pad).is_none()
                {
                    continue;
                }
                if immediate {
                    start_clip(clip);
                    trigger_step(clip, 0);
                    clip.phase = if clip.length > 0 { 1 % clip.length } else { 0 };
                } else {
                    clip.pending_start = target;
                    clip.pending_stop = -1;
                    clip.state = 1;
                }
            } else if launch || (in_row && !launch) {
                let clip = &mut self.clips[pad];
                if clip.state == 2 || clip.pending_start >= 0 || clip.pending_stop >= 0 {
                    if immediate {
                        stop_clip(clip, false, self.sample_rate);
                    } else {
                        clip.pending_stop = target;
                        clip.pending_start = -1;
                        clip.state = 3;
                    }
                }
            }
            self.update_pad_state(pad);
        }
    }

    fn clear_pad(&mut self, pad: usize) {
        {
            let clip = &mut self.clips[pad];
            stop_clip(clip, true, self.sample_rate);
            clip.kind = 0;
            clip.fx_enabled = false;
            clip.fx_type = 0;
            clip.sample_loop = true;
            clip.steps = [ClipStep::default(); STEPS];
            clip.midi = [MidiSlot::default(); MAX_MIDI_NOTES];
            clip.drum_mask = [0; DRUM_VOICES];
            clip.drum_vel = [[0; STEPS]; DRUM_VOICES];
        }
        self.shared.samples.clear(pad);
        if let Some(bridge) = self.clips[pad].plugin.take() {
            self.shared.retire_bridge(bridge);
        }
        self.update_pad_state(pad);
    }

    fn apply_configure(&mut self, pad: usize, command: &Command) {
        let kind = clampi(command.clip_kind as i32, 0, ClipKind::Plugin.ordinal());
        let previous_kind = self.clips[pad].kind;
        if kind != previous_kind {
            let clip = &mut self.clips[pad];
            stop_clip(clip, true, self.sample_rate);
            clip.kind = kind;
            clip.phase = 0;
            clip.pending_start = -1;
            clip.pending_stop = -1;
            clip.last_note = -1;
            clip.steps = [ClipStep::default(); STEPS];
            clip.midi = [MidiSlot::default(); MAX_MIDI_NOTES];
            clip.drum_mask = [0; DRUM_VOICES];
            clip.drum_vel = [[0; STEPS]; DRUM_VOICES];

            let root = clampi(command.root as i32, 0, 127);
            if kind == ClipKind::Acid.ordinal() {
                for i in 0..6 {
                    let step = &mut clip.steps[DEFAULT_ACID_STEPS[i]];
                    step.gate = true;
                    step.note = clampi(root + DEFAULT_ACID_NOTES[i], 0, 127) as u8;
                    step.velocity = if i == 0 || i == 3 { 118 } else { 92 };
                    step.accent = i == 0 || i == 3;
                    step.slide = i == 5;
                }
            } else if kind == ClipKind::Wave.ordinal() {
                for i in 0..4 {
                    let step = &mut clip.steps[DEFAULT_WAVE_STEPS[i]];
                    step.gate = true;
                    step.note = clampi(root + DEFAULT_WAVE_NOTES[i], 0, 127) as u8;
                    step.velocity = 100;
                }
            } else if kind == ClipKind::Drums.ordinal() {
                for step in [0usize, 4, 8, 12] {
                    clip.drum_mask[0] |= 1 << step;
                    clip.drum_vel[0][step] = 120;
                }
                clip.drum_mask[1] |= (1 << 4) | (1 << 12);
                clip.drum_vel[1][4] = 112;
                clip.drum_vel[1][12] = 112;
                for step in (0..STEPS).step_by(2) {
                    clip.drum_mask[7] |= 1 << step;
                    clip.drum_vel[7][step] = if step % 4 == 0 { 92 } else { 70 };
                }
                clip.drum_mask[8] |= 1 << 14;
                clip.drum_vel[8][14] = 84;
                clip.drum_mask[6] |= 1 << 11;
                clip.drum_vel[6][11] = 96;
            }
        }

        {
            let clip = &mut self.clips[pad];
            clip.root = clampi(command.root as i32, 0, 127);
            clip.length = clampi(command.length as i32, 1, STEPS as i32);
            clip.gain = clampf(command.gain, 0.0, 1.5);
            clip.pan = clampf(command.pan, -1.0, 1.0);
            clip.mute = command.flags & 0x01 != 0;
            clip.solo = command.flags & 0x02 != 0;
            clip.sample_loop = command.flags & 0x08 != 0;
            clip.fx_type = clampi(command.fx_type as i32, 0, FX_COUNT as i32 - 1) as usize;
            clip.fx_enabled = command.fx_enabled;
            clip.params = command.params;
            apply_voice_params(clip);
        }

        let effect = &mut self.fx[pad];
        let clip = &self.clips[pad];
        effect.set_type(clip.fx_type);
        effect.set_all_params(&clip.fx_params());
        effect.set_enabled(clip.fx_enabled && clip.fx_type != FX_ID_BYPASS);
        self.update_pad_state(pad);
    }

    fn apply_params(&mut self, pad: usize, command: &Command) {
        {
            let clip = &mut self.clips[pad];
            clip.gain = clampf(command.gain, 0.0, 1.5);
            clip.pan = clampf(command.pan, -1.0, 1.0);
            clip.mute = command.flags & 0x01 != 0;
            clip.solo = command.flags & 0x02 != 0;
            clip.fx_enabled = command.fx_enabled;
            clip.params = command.params;
            apply_voice_params(clip);
        }
        let effect = &mut self.fx[pad];
        let clip = &self.clips[pad];
        effect.set_all_params(&clip.fx_params());
        effect.set_enabled(clip.fx_enabled && clip.fx_type != FX_ID_BYPASS);
        self.update_pad_state(pad);
    }

    fn apply_midi_note(&mut self, pad: usize, command: &Command) {
        let clip = &mut self.clips[pad];
        // Wave instruments and hosted plugins both play the piano roll.
        if clip.kind != ClipKind::Wave.ordinal() && clip.kind != ClipKind::Plugin.ordinal() {
            return;
        }
        let note = clampi(command.note as i32, 0, 127) as u8;
        let start = clampi(command.step as i32, 0, (clip.length - 1).max(0)) as u8;
        let length = clampi(
            command.length as i32,
            1,
            (clip.length - start as i32).max(1),
        ) as u8;

        if command.flags & 1 == 0 {
            for entry in clip.midi.iter_mut() {
                if !entry.active || entry.note != note || entry.start != start {
                    continue;
                }
                entry.active = false;
                if clip.state == 2 {
                    clip.wave.note_off(note as i32);
                }
            }
            return;
        }

        // One note per pitch/start cell: re-tapping a cell updates its length.
        for entry in clip.midi.iter_mut() {
            if entry.active && entry.note == note && entry.start == start {
                entry.velocity = clampi(command.velocity as i32, 1, 127) as u8;
                entry.length = length;
                return;
            }
        }
        for entry in clip.midi.iter_mut() {
            if entry.active {
                continue;
            }
            entry.active = true;
            entry.note = note;
            entry.velocity = clampi(command.velocity as i32, 1, 127) as u8;
            entry.start = start;
            entry.length = length;
            // MIDI editing replaces the legacy step pattern for this clip.
            for step in clip.steps.iter_mut() {
                step.gate = false;
            }
            return;
        }
    }

    fn update_pad_state(&self, pad: usize) {
        let clip = &self.clips[pad];
        let packed = (clip.kind as u32 & 0x0F)
            | ((clip.state as u32 & 0x0F) << 4)
            | ((clip.phase as u32 & 0xFF) << 8)
            | ((clip.fx_type as u32 & 0xFF) << 16)
            | (((clip.mute as u32) & 1) << 24)
            | (((clip.solo as u32) & 1) << 25);
        self.shared.telemetry.set_pad_state(pad, packed);
    }

    fn next_boundary(&self, quant_steps: i32) -> i64 {
        let base = self.absolute_step
            + if self.samples_into_step > 1.0e-9 {
                1
            } else {
                0
            };
        if quant_steps <= 0 {
            return base;
        }
        let quant = quant_steps as i64;
        ((base + quant - 1) / quant) * quant
    }

    fn step_duration_for(&self, step: i64) -> f64 {
        let swing = self.shared.swing() as f64;
        let mult = if step % 2 == 0 {
            1.0 - swing * 0.5
        } else {
            1.0 + swing * 0.5
        };
        self.samples_per_step * mult.max(0.25)
    }

    fn start_transport(&mut self) {
        self.transport_running = true;
        self.absolute_step = 0;
        self.samples_into_step = 0.0;
        self.step_duration = self.step_duration_for(0);
    }

    fn stop_transport(&mut self) {
        self.transport_running = false;
        self.shared.playing.store(false, Ordering::Release);
        for pad in 0..PADS {
            stop_clip(&mut self.clips[pad], true, self.sample_rate);
            self.update_pad_state(pad);
        }
        self.absolute_step = 0;
        self.samples_into_step = 0.0;
    }

    fn on_step_boundary(&mut self) {
        self.samples_into_step = 0.0;

        for pad in 0..PADS {
            let clip = &mut self.clips[pad];
            if clip.pending_stop >= 0 && clip.pending_stop == self.absolute_step {
                stop_clip(clip, false, self.sample_rate);
                self.update_pad_state(pad);
            }
        }
        for pad in 0..PADS {
            let clip = &mut self.clips[pad];
            if clip.pending_start >= 0 && clip.pending_start == self.absolute_step {
                start_clip(clip);
                self.update_pad_state(pad);
            }
        }
        for pad in 0..PADS {
            let clip = &mut self.clips[pad];
            if clip.state != 2 {
                continue;
            }
            trigger_step(clip, clip.phase);
            clip.phase = if clip.length > 0 {
                (clip.phase + 1) % clip.length
            } else {
                0
            };
            self.update_pad_state(pad);
        }

        self.step_duration = self.step_duration_for(self.absolute_step);
        self.samples_into_step = self.step_duration;
        self.absolute_step += 1;

        self.shared.telemetry.set_playhead(
            (self.absolute_step % STEPS as i64) as i32,
            self.absolute_step as i32,
        );
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    pub fn process(&mut self, interleaved: &mut [f32], frames: usize) {
        if interleaved.is_empty() || frames == 0 {
            return;
        }
        if frames > self.max_block_frames {
            self.prepare(self.sample_rate, frames);
        }
        let frames = frames.min(self.max_block_frames);
        interleaved[..frames * 2].fill(0.0);
        self.drain_commands();

        let want_playing = self.shared.playing.load(Ordering::Acquire);
        if want_playing && !self.transport_running {
            self.start_transport();
        } else if !want_playing && self.transport_running {
            self.stop_transport();
        }

        self.samples_per_step = self.sample_rate * 60.0 / self.shared.tempo() as f64 / 4.0;

        self.pad_peak_block = [0.0; PADS];
        let pad_record_index = self.shared.pad_record_index.load(Ordering::Acquire);
        if pad_record_index >= 0 && self.shared.pad_tap.is_active() {
            self.pad_capture[..frames * 2].fill(0.0);
        }

        let mut offset = 0usize;
        while offset < frames {
            let chunk;
            if self.transport_running {
                if self.samples_into_step <= 1.0e-9 {
                    self.on_step_boundary();
                }
                let remaining = self.samples_into_step;
                chunk = (frames - offset).min((remaining.ceil() as usize).max(1));
                self.render_chunk(
                    &mut interleaved[offset * 2..],
                    chunk,
                    pad_record_index,
                    offset,
                );
                self.samples_into_step -= chunk as f64;
            } else {
                chunk = frames - offset;
                self.render_chunk(
                    &mut interleaved[offset * 2..],
                    chunk,
                    pad_record_index,
                    offset,
                );
            }
            offset += chunk;
        }

        let mut master_peak = 0.0f32;
        for i in 0..frames {
            master_peak = master_peak.max(interleaved[i * 2].abs());
            master_peak = master_peak.max(interleaved[i * 2 + 1].abs());
        }
        self.shared.telemetry.set_master_meter(master_peak);
        for pad in 0..PADS {
            self.shared
                .telemetry
                .set_pad_meter(pad, self.pad_peak_block[pad]);
            let packed = self.shared.telemetry.pad(pad);
            if packed.state as i32 != self.clips[pad].state {
                self.update_pad_state(pad);
            }
        }

        let mut i = 0;
        while i < frames {
            self.shared.telemetry.push_scope(interleaved[i * 2]);
            i += 32;
        }

        self.shared.master_tap.push_interleaved(interleaved, frames);
        if pad_record_index >= 0 {
            self.shared
                .pad_tap
                .push_interleaved(&self.pad_capture, frames);
        }

        self.shared
            .telemetry
            .master_frames
            .store(self.shared.master_tap.pushed(), Ordering::Relaxed);
        self.shared
            .telemetry
            .pad_frames
            .store(self.shared.pad_tap.pushed(), Ordering::Relaxed);
        self.shared.telemetry.dropped_frames.store(
            self.shared.master_tap.dropped() + self.shared.pad_tap.dropped(),
            Ordering::Relaxed,
        );
    }

    fn render_chunk(
        &mut self,
        out: &mut [f32],
        frames: usize,
        pad_record_index: i32,
        buffer_offset: usize,
    ) {
        let master_gain = self.shared.master_gain();
        let bpm = self.shared.tempo();
        let any_solo = self.clips.iter().any(|clip| clip.solo);
        let sample_rate = self.sample_rate;
        let samples = &self.shared.samples;

        for i in 0..frames {
            let mut mix_l = 0.0f64;
            let mut mix_r = 0.0f64;
            let mut captured_l = 0.0f32;
            let mut captured_r = 0.0f32;

            for pad in 0..PADS {
                let clip = &mut self.clips[pad];
                if clip.kind == 0 {
                    continue;
                }
                let playing = clip.state == 2;
                if !playing && clip.tail_samples <= 0.0 {
                    continue;
                }

                let mut l = 0.0f32;
                let mut r = 0.0f32;
                match ClipKind::from_ordinal(clip.kind) {
                    ClipKind::Acid => {
                        let s = clip.acid.process();
                        l = s;
                        r = s;
                    }
                    ClipKind::Wave => {
                        clip.wave.render(&mut l, &mut r);
                    }
                    ClipKind::Drums => {
                        clip.drums.render(&mut l, &mut r);
                    }
                    ClipKind::Plugin => {
                        // The plugin runs on its own thread; the bridge hands
                        // back the block that was rendered one step earlier.
                        if let Some(bridge) = clip.plugin.as_mut() {
                            bridge.bpm = bpm;
                            bridge.playing = clip.state == 2;
                            bridge.position = bridge.position.wrapping_add(1);
                            let (out_l, out_r) = bridge.tick(0.0, 0.0);
                            l = out_l;
                            r = out_r;
                        }
                    }
                    ClipKind::Sample => {
                        let sample = samples.get(pad);
                        let Some(sample) = sample else {
                            clip.state = 0;
                            continue;
                        };
                        if sample.frames() < 2 {
                            clip.state = 0;
                            continue;
                        }
                        let sample_frames = sample.frames();
                        let ratio = (sample.sample_rate as f64 / sample_rate)
                            * 2.0_f64.powf(clip.params[1] as f64 / 12.0);
                        let mut idx = clip.sample_pos as usize;
                        if idx >= sample_frames - 1 {
                            if clip.sample_loop {
                                clip.sample_pos %= (sample_frames - 1) as f64;
                                idx = clip.sample_pos as usize;
                            } else {
                                clip.state = 0;
                                clip.sample_pos = 0.0;
                                continue;
                            }
                        }
                        let frac = (clip.sample_pos - idx as f64) as f32;
                        if sample.channels == 2 {
                            let l0 = sample.stereo[idx * 2];
                            let r0 = sample.stereo[idx * 2 + 1];
                            let l1 = sample.stereo[(idx + 1) * 2];
                            let r1 = sample.stereo[(idx + 1) * 2 + 1];
                            l = l0 + (l1 - l0) * frac;
                            r = r0 + (r1 - r0) * frac;
                        } else {
                            let s0 = sample.stereo[idx];
                            let s1 = sample.stereo[idx + 1];
                            l = s0 + (s1 - s0) * frac;
                            r = l;
                        }
                        let sample_gain = if clip.params[0] <= 0.0 {
                            1.0
                        } else {
                            clip.params[0]
                        };
                        l *= sample_gain;
                        r *= sample_gain;
                        clip.sample_pos += ratio;
                        if clip.sample_pos >= (sample_frames - 1) as f64 {
                            if clip.sample_loop {
                                clip.sample_pos %= (sample_frames - 1) as f64;
                            } else {
                                clip.state = 0;
                                clip.sample_pos = 0.0;
                            }
                        }
                    }
                    ClipKind::Empty => {}
                }

                let effect = &mut self.fx[pad];
                if clip.fx_enabled && clip.fx_type != FX_ID_BYPASS {
                    effect.process(&mut l, &mut r, bpm);
                }

                let gain = clip.gain;
                l *= gain * balance_l(clip.pan);
                r *= gain * balance_r(clip.pan);

                if pad as i32 == pad_record_index {
                    captured_l = l;
                    captured_r = r;
                }

                let mut tail_gain = 1.0f32;
                if clip.tail_samples > 0.0 {
                    let fade = sample_rate * 0.25;
                    tail_gain = (clip.tail_samples / fade).min(1.0) as f32;
                    clip.tail_samples -= 1.0;
                    if clip.tail_samples <= 0.0 {
                        clip.state = 0;
                        clip.tail_samples = 0.0;
                    }
                }

                let peak = l.abs().max(r.abs());
                if peak > self.pad_peak_block[pad] {
                    self.pad_peak_block[pad] = peak;
                }

                if !clip.mute && (!any_solo || clip.solo) {
                    mix_l += (l * tail_gain) as f64;
                    mix_r += (r * tail_gain) as f64;
                }
            }

            let out_l = soft_limit((mix_l * master_gain as f64) as f32, 0.90);
            let out_r = soft_limit((mix_r * master_gain as f64) as f32, 0.90);
            out[i * 2] = sanitize(out_l);
            out[i * 2 + 1] = sanitize(out_r);

            if pad_record_index >= 0 {
                let at = (buffer_offset + i) * 2;
                if at + 1 < self.pad_capture.len() {
                    self.pad_capture[at] = captured_l;
                    self.pad_capture[at + 1] = captured_r;
                }
            }
        }
    }

    /// Render into the internal buffer (offline renders, tests).
    pub fn render_silent(&mut self, frames: usize) {
        if frames == 0 {
            return;
        }
        if self.silent_buffer.len() < frames * 2 {
            self.silent_buffer = vec![0.0; frames * 2];
            self.prepare(self.sample_rate, frames);
        }
        let mut buffer = std::mem::take(&mut self.silent_buffer);
        self.process(&mut buffer, frames);
        self.silent_buffer = buffer;
    }

    pub fn pad_telemetry(&self, pad: usize) -> PadTelemetry {
        self.shared.telemetry.pad(pad)
    }

    pub fn is_transport_running(&self) -> bool {
        self.transport_running
    }

    pub fn absolute_step(&self) -> i64 {
        self.absolute_step
    }
}

fn apply_voice_params(clip: &mut Clip) {
    match ClipKind::from_ordinal(clip.kind) {
        ClipKind::Acid => {
            let p = &clip.params;
            let cutoff = if p[1] < 20.0 { 350.0 } else { p[1] };
            let decay = if p[4] < 0.05 { 0.7 } else { p[4] };
            let volume = if p[10] <= 0.0 { 0.8 } else { p[10] };
            clip.acid.set_waveform(p[0] as i32);
            clip.acid.set_cutoff(cutoff);
            clip.acid.set_resonance(clampf(p[2], 0.0, 1.0));
            clip.acid.set_env_mod(clampf(p[3], 0.0, 1.0));
            clip.acid.set_decay(decay);
            clip.acid.set_accent(clampf(p[5], 0.0, 1.0));
            clip.acid.set_volume(volume);
            clip.acid.set_distortion(
                p[8] > 0.001,
                p[6] as i32,
                clampf(p[7], 0.0, 1.0),
                clampf(p[9], 0.0, 1.0),
                clampf(p[8], 0.0, 1.0),
            );
        }
        ClipKind::Wave => {
            let p = &clip.params;
            let params = WaveParams {
                bank: p[0] as i32,
                position: clampf(p[1], 0.0, 1.0),
                unison: if p[2] < 1.0 { 1 } else { p[2] as i32 },
                detune: p[3],
                spread: p[4],
                cutoff: if p[5] < 40.0 { 2400.0 } else { p[5] },
                resonance: if p[6] < 0.5 { 0.5 } else { p[6] },
                drive: if p[7] < 1.0 { 1.0 } else { p[7] },
                attack: if p[8] <= 0.0 { 0.005 } else { p[8] },
                decay: if p[9] <= 0.0 { 0.35 } else { p[9] },
                sustain: p[10],
                release: if p[11] <= 0.0 { 0.45 } else { p[11] },
                sub: p[12],
                noise: p[13],
                level: if p[14] <= 0.0 { 0.8 } else { p[14] },
                filter_mode: p[15] as i32,
                chord: p[16] as i32,
            };
            clip.wave.set_params(params);
        }
        ClipKind::Drums => {
            let p = &clip.params;
            clip.drums.set_params(
                p[0],
                p[1],
                p[2],
                p[3],
                p[4],
                if p[5] <= 0.0 { 0.85 } else { p[5] },
                if p[6] <= 0.0 { 18000.0 } else { p[6] },
                if p[7] <= 0.0 { 0.6 } else { p[7] },
            );
        }
        _ => {}
    }
}

fn start_clip(clip: &mut Clip) {
    clip.state = 2;
    clip.pending_start = -1;
    clip.pending_stop = -1;
    clip.phase = 0;
    clip.tail_samples = 0.0;
    clip.last_note = -1;
    clip.sample_pos = 0.0;
    clip.acid.all_notes_off();
    clip.wave.all_notes_off();
    clip.plugin_notes = 0;
}

fn stop_clip(clip: &mut Clip, hard: bool, sample_rate: f64) {
    let was_playing = clip.state == 2;
    clip.state = 0;
    clip.pending_start = -1;
    clip.pending_stop = -1;
    clip.last_note = -1;
    clip.acid.all_notes_off();
    clip.wave.all_notes_off();
    release_plugin_notes(&mut clip.plugin, &mut clip.plugin_notes);
    if hard {
        clip.tail_samples = 0.0;
        clip.sample_pos = 0.0;
        clip.drums.reset();
    } else {
        clip.tail_samples = if clip.kind == ClipKind::Sample.ordinal() || !was_playing {
            0.0
        } else {
            sample_rate * 0.75
        };
    }
    if clip.kind == ClipKind::Sample.ordinal() {
        clip.sample_pos = 0.0;
    }
}

fn trigger_step(clip: &mut Clip, step: i32) {
    let s = clampi(step, 0, STEPS as i32 - 1) as usize;
    if clip.kind == ClipKind::Plugin.ordinal() {
        trigger_plugin_step(
            &clip.midi,
            &mut clip.plugin,
            &mut clip.plugin_notes,
            clip.length,
            s,
        );
        return;
    }
    match ClipKind::from_ordinal(clip.kind) {
        ClipKind::Acid => {
            let cs = clip.steps[s];
            if cs.gate {
                if !cs.slide {
                    clip.acid.all_notes_off();
                }
                clip.acid.note_on(cs.note as i32, cs.velocity as i32);
                clip.last_note = cs.note as i32;
            } else {
                clip.acid.all_notes_off();
                clip.last_note = -1;
            }
        }
        ClipKind::Wave => {
            let has_midi = clip.midi.iter().any(|note| note.active);
            if has_midi {
                trigger_midi_step(clip, s);
            } else {
                let cs = clip.steps[s];
                clip.wave.all_notes_off();
                if cs.gate {
                    clip.wave.note_on(cs.note as i32, cs.velocity as i32);
                }
            }
        }
        ClipKind::Drums => {
            for voice in 0..DRUM_VOICES {
                if clip.drum_mask[voice] & (1u16 << s) == 0 {
                    continue;
                }
                let velocity = clip.drum_vel[voice][s];
                let accent = if velocity >= 100 { 0.85 } else { 0.0 };
                clip.drums.trigger(voice, velocity as f32 / 127.0, accent);
            }
        }
        _ => {}
    }
}

/// Send the note on/off events for one 16th-note step to a hosted plugin.
///
/// The note model is the same one the wavetable piano roll uses, so switching a
/// pad between Wave and a hosted instrument keeps its phrase.
fn trigger_plugin_step(
    midi: &[MidiSlot; MAX_MIDI_NOTES],
    plugin: &mut Option<PluginBridge>,
    active: &mut u128,
    length: i32,
    step: usize,
) {
    let Some(bridge) = plugin.as_mut() else {
        return;
    };
    let loop_len = length.max(1) as usize;

    // Releases first: a note ending here can be retriggered on the same step.
    for note in midi.iter() {
        if !note.active {
            continue;
        }
        let span = (note.length as usize).clamp(1, loop_len);
        let end_step = (note.start as usize + span) % loop_len;
        if end_step == step && note_off_bit(active, note.note) {
            bridge.push_event(NoteEvent::note_off(note.note, 0));
        }
    }
    for note in midi.iter() {
        if note.active && note.start as usize == step {
            bridge.push_event(NoteEvent::note_on(note.note, note.velocity, 0));
            *active |= 1u128 << (note.note & 0x7F);
        }
    }
}

#[inline]
fn note_off_bit(active: &mut u128, note: u8) -> bool {
    let mask = 1u128 << (note & 0x7F);
    if *active & mask != 0 {
        *active &= !mask;
        true
    } else {
        false
    }
}

/// Release everything a plugin pad is holding.
fn release_plugin_notes(plugin: &mut Option<PluginBridge>, active: &mut u128) {
    let Some(bridge) = plugin.as_mut() else {
        *active = 0;
        return;
    };
    let mut mask = *active;
    while mask != 0 {
        let note = mask.trailing_zeros() as u8;
        bridge.push_event(NoteEvent::note_off(note, 0));
        mask &= mask - 1;
    }
    *active = 0;
}

fn trigger_midi_step(clip: &mut Clip, step: usize) {
    let loop_len = clip.length.max(1) as usize;
    // Releases first: a note ending here can be retriggered on the same step.
    for note in clip.midi.iter() {
        if !note.active {
            continue;
        }
        let span = (note.length as usize).clamp(1, loop_len);
        let end_step = (note.start as usize + span) % loop_len;
        if end_step == step {
            clip.wave.note_off(note.note as i32);
        }
    }
    for note in clip.midi.iter() {
        if note.active && note.start as usize == step {
            clip.wave.note_on(note.note as i32, note.velocity as i32);
        }
    }
}
