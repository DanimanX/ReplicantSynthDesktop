//! The control surface between the UI/MIDI threads and the audio thread:
//! a command ring, atomic telemetry, sample publication and capture taps.

use super::plugin::{PluginBridge, PluginHostCommand};
use super::recorder::RecordTap;
use crate::dsp::FX_COUNT;
use crate::model::{ClipKind, ClipState};
use arc_swap::ArcSwapOption;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const PADS: usize = 64;
pub const QUEUE_CAPACITY: usize = 8192;
const SCOPE_SIZE: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandType {
    Configure,
    Params,
    Step,
    DrumStep,
    MidiNote,
    ClearMidi,
    Preview,
    Launch,
    Stop,
    LaunchScene,
    StopScene,
    StopAll,
    Clear,
    Panic,
}

/// A fixed-size control message. Everything the engine needs is inline so no
/// allocation or ownership hand-off happens on the audio thread.
#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub kind: CommandType,
    pub pad: u8,
    pub clip_kind: u8,
    pub flags: u8,
    pub root: i16,
    pub length: i16,
    pub step: i16,
    pub voice: i16,
    pub note: i16,
    pub velocity: i16,
    pub fx_type: i16,
    pub fx_enabled: bool,
    pub gain: f32,
    pub pan: f32,
    pub params: [f32; 32],
}

impl Command {
    pub fn new(kind: CommandType, pad: usize) -> Self {
        Self {
            kind,
            pad: pad as u8,
            clip_kind: 0,
            flags: 0,
            root: 60,
            length: 16,
            step: 0,
            voice: 0,
            note: 60,
            velocity: 100,
            fx_type: 0,
            fx_enabled: false,
            gain: 0.8,
            pan: 0.0,
            params: [0.0; 32],
        }
    }
}

/// Interleaved stereo sample owned by the UI and published to the engine.
pub struct SampleData {
    pub stereo: Vec<f32>,
    pub channels: usize,
    pub sample_rate: u32,
}

impl SampleData {
    pub fn frames(&self) -> usize {
        self.stereo.len().checked_div(self.channels).unwrap_or(0)
    }
}

/// One `ArcSwapOption` slot per pad, so a sample can be replaced without ever
/// freeing memory under the audio callback.
pub struct SampleSlots {
    slots: [ArcSwapOption<SampleData>; PADS],
}

impl Default for SampleSlots {
    fn default() -> Self {
        Self::new()
    }
}

impl SampleSlots {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| ArcSwapOption::empty()),
        }
    }

    pub fn publish(&self, pad: usize, sample: Arc<SampleData>) {
        if pad < PADS {
            self.slots[pad].store(Some(sample));
        }
    }

    pub fn clear(&self, pad: usize) {
        if pad < PADS {
            self.slots[pad].store(None);
        }
    }

    pub fn get(&self, pad: usize) -> Option<Arc<SampleData>> {
        if pad < PADS {
            self.slots[pad].load_full()
        } else {
            None
        }
    }

    pub fn has_sample(&self, pad: usize) -> bool {
        pad < PADS && self.slots[pad].load().is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PadTelemetry {
    pub kind: ClipKind,
    pub state: ClipState,
    pub phase: u8,
    pub fx_type: u8,
    pub mute: bool,
    pub solo: bool,
}

impl PadTelemetry {
    fn decode(packed: u32) -> Self {
        Self {
            kind: ClipKind::from_ordinal((packed & 0x0F) as i32),
            state: ClipState::from_ordinal(((packed >> 4) & 0x0F) as i32),
            phase: ((packed >> 8) & 0xFF) as u8,
            fx_type: ((packed >> 16) & 0xFF) as u8,
            mute: ((packed >> 24) & 1) != 0,
            solo: ((packed >> 25) & 1) != 0,
        }
    }
}

/// Everything the UI polls: pad states, meters, the scope and the playhead.
pub struct Telemetry {
    pad_state: [AtomicU32; PADS],
    pad_meter: [AtomicU32; PADS],
    master_meter: AtomicU32,
    scope: [AtomicU32; SCOPE_SIZE],
    scope_write: AtomicU32,
    playhead: AtomicI32,
    transport_step: AtomicI32,
    pub master_frames: AtomicU64,
    pub pad_frames: AtomicU64,
    pub dropped_frames: AtomicU64,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            pad_state: std::array::from_fn(|_| AtomicU32::new(0)),
            pad_meter: std::array::from_fn(|_| AtomicU32::new(0)),
            master_meter: AtomicU32::new(0),
            scope: std::array::from_fn(|_| AtomicU32::new(0)),
            scope_write: AtomicU32::new(0),
            playhead: AtomicI32::new(0),
            transport_step: AtomicI32::new(0),
            master_frames: AtomicU64::new(0),
            pad_frames: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
        }
    }

    pub fn set_pad_state(&self, pad: usize, packed: u32) {
        self.pad_state[pad].store(packed, Ordering::Release);
    }

    pub fn pad(&self, pad: usize) -> PadTelemetry {
        PadTelemetry::decode(self.pad_state[pad].load(Ordering::Acquire))
    }

    pub fn set_pad_meter(&self, pad: usize, value: f32) {
        self.pad_meter[pad].store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn pad_meter(&self, pad: usize) -> f32 {
        f32::from_bits(self.pad_meter[pad].load(Ordering::Relaxed))
    }

    pub fn set_master_meter(&self, value: f32) {
        self.master_meter.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn master_meter(&self) -> f32 {
        f32::from_bits(self.master_meter.load(Ordering::Relaxed))
    }

    pub fn push_scope(&self, value: f32) {
        let write = self.scope_write.load(Ordering::Relaxed) as usize % SCOPE_SIZE;
        self.scope[write].store(value.to_bits(), Ordering::Relaxed);
        self.scope_write
            .store(((write + 1) % SCOPE_SIZE) as u32, Ordering::Relaxed);
    }

    pub fn read_scope(&self, out: &mut [f32]) {
        let write = self.scope_write.load(Ordering::Relaxed) as usize;
        let count = out.len().min(SCOPE_SIZE);
        for (i, slot) in out.iter_mut().enumerate().take(count) {
            let index = (write + SCOPE_SIZE - count + i) % SCOPE_SIZE;
            *slot = f32::from_bits(self.scope[index].load(Ordering::Relaxed));
        }
    }

    pub fn set_playhead(&self, absolute_step: i32, phase: i32) {
        self.transport_step.store(absolute_step, Ordering::Relaxed);
        self.playhead.store(phase, Ordering::Relaxed);
    }

    pub fn playhead(&self) -> i32 {
        self.playhead.load(Ordering::Relaxed)
    }

    pub fn absolute_step(&self) -> i32 {
        self.transport_step.load(Ordering::Relaxed)
    }
}

/// Shared engine state. The UI and the audio thread each hold one `Arc`.
pub struct EngineShared {
    pub commands: ArrayQueue<Command>,
    pub telemetry: Telemetry,
    pub samples: SampleSlots,
    pub master_tap: Arc<RecordTap>,
    pub pad_tap: Arc<RecordTap>,
    pub pad_record_index: AtomicI32,
    pub playing: AtomicBool,
    pub tempo: AtomicU32,
    pub quantize: AtomicI32,
    pub swing: AtomicU32,
    pub master_gain: AtomicU32,
    pub sample_rate: AtomicU32,
    pub max_block: AtomicU32,
    /// Pads whose hosted plugin must keep processing even while the pad is
    /// stopped. A plugin with its editor open has to run: its editor and its
    /// parameter reports only move while it processes, so a stopped pad with a
    /// visible editor would otherwise show a frozen parameter list.
    plugin_awake: [AtomicBool; PADS],
    /// UI -> engine: attach or detach a hosted plugin bridge.
    plugin_tx: Sender<PluginHostCommand>,
    plugin_rx: Mutex<Option<Receiver<PluginHostCommand>>>,
    /// Engine -> UI: bridges the engine has detached, to be dropped off the
    /// audio thread.
    plugin_retired_rx: Mutex<Receiver<PluginBridge>>,
    plugin_retired_tx: Sender<PluginBridge>,
}

impl EngineShared {
    pub fn new() -> Self {
        let (plugin_tx, plugin_rx) = unbounded();
        let (plugin_retired_tx, plugin_retired_rx) = unbounded();
        Self {
            commands: ArrayQueue::new(QUEUE_CAPACITY),
            telemetry: Telemetry::new(),
            samples: SampleSlots::new(),
            master_tap: Arc::new(RecordTap::new()),
            pad_tap: Arc::new(RecordTap::new()),
            pad_record_index: AtomicI32::new(-1),
            playing: AtomicBool::new(false),
            tempo: AtomicU32::new(120.0f32.to_bits()),
            quantize: AtomicI32::new(16),
            swing: AtomicU32::new(0.0f32.to_bits()),
            master_gain: AtomicU32::new(0.85f32.to_bits()),
            sample_rate: AtomicU32::new(48_000),
            max_block: AtomicU32::new(512),
            plugin_awake: std::array::from_fn(|_| AtomicBool::new(false)),
            plugin_tx,
            plugin_rx: Mutex::new(Some(plugin_rx)),
            plugin_retired_tx,
            plugin_retired_rx: Mutex::new(plugin_retired_rx),
        }
    }

    /// Hand the install-command receiver to the engine. Called once, at
    /// construction, so the mutex never runs on the audio thread.
    pub fn take_plugin_receiver(&self) -> Option<Receiver<PluginHostCommand>> {
        self.plugin_rx.lock().ok().and_then(|mut slot| slot.take())
    }

    pub fn plugin_sender(&self) -> Sender<PluginHostCommand> {
        self.plugin_tx.clone()
    }

    /// Called by the engine when it detaches a bridge; the UI drains and drops
    /// it, keeping plugin teardown off the audio thread.
    pub fn retire_bridge(&self, bridge: PluginBridge) {
        let _ = self.plugin_retired_tx.send(bridge);
    }

    /// Collect the bridges the engine has finished with. Dropping them here
    /// releases the plugin worker.
    pub fn drain_retired_bridges(&self) {
        if let Ok(receiver) = self.plugin_retired_rx.lock() {
            while receiver.try_recv().is_ok() {}
        }
    }

    /// Keep this pad's plugin processing while the pad is stopped.
    pub fn set_plugin_awake(&self, pad: usize, awake: bool) {
        if let Some(slot) = self.plugin_awake.get(pad) {
            slot.store(awake, Ordering::Relaxed);
        }
    }

    /// Whether a stopped pad still has to run its plugin.
    pub fn plugin_awake(&self, pad: usize) -> bool {
        self.plugin_awake
            .get(pad)
            .is_some_and(|slot| slot.load(Ordering::Relaxed))
    }

    pub fn tempo(&self) -> f32 {
        f32::from_bits(self.tempo.load(Ordering::Relaxed))
    }

    pub fn set_tempo(&self, bpm: f32) {
        self.tempo
            .store(bpm.clamp(30.0, 300.0).to_bits(), Ordering::Relaxed);
    }

    pub fn quantize(&self) -> i32 {
        self.quantize.load(Ordering::Relaxed)
    }

    pub fn set_quantize(&self, steps: i32) {
        const ALLOWED: [i32; 6] = [0, 1, 4, 16, 32, 64];
        let best = ALLOWED
            .iter()
            .copied()
            .min_by_key(|value| (value - steps).abs())
            .unwrap_or(16);
        self.quantize.store(best, Ordering::Relaxed);
    }

    pub fn swing(&self) -> f32 {
        f32::from_bits(self.swing.load(Ordering::Relaxed))
    }

    pub fn set_swing(&self, swing: f32) {
        self.swing
            .store(swing.clamp(0.0, 0.66).to_bits(), Ordering::Relaxed);
    }

    pub fn master_gain(&self) -> f32 {
        f32::from_bits(self.master_gain.load(Ordering::Relaxed))
    }

    pub fn set_master_gain(&self, gain: f32) {
        self.master_gain
            .store(gain.clamp(0.0, 1.5).to_bits(), Ordering::Relaxed);
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }
}

impl Default for EngineShared {
    fn default() -> Self {
        Self::new()
    }
}

/// Cheap cloneable handle used by the UI, MIDI thread and tests.
#[derive(Clone)]
pub struct EngineHandles {
    shared: Arc<EngineShared>,
}

impl EngineHandles {
    pub fn new(shared: Arc<EngineShared>) -> Self {
        Self { shared }
    }

    pub fn shared(&self) -> &Arc<EngineShared> {
        &self.shared
    }

    pub fn telemetry(&self) -> &Telemetry {
        &self.shared.telemetry
    }

    pub fn samples(&self) -> &SampleSlots {
        &self.shared.samples
    }

    /// Channel the UI uses to attach or detach hosted plugins.
    pub fn plugin_sender(&self) -> Sender<PluginHostCommand> {
        self.shared.plugin_sender()
    }

    /// Keep a pad's plugin processing while the pad is stopped, which is what
    /// keeps its editor alive and its parameter reports flowing.
    pub fn set_plugin_awake(&self, pad: usize, awake: bool) {
        self.shared.set_plugin_awake(pad, awake);
    }

    /// Drop the bridges the engine has detached, which releases the plugin
    /// workers they belong to.
    pub fn drain_retired_bridges(&self) {
        self.shared.drain_retired_bridges();
    }

    pub fn master_tap(&self) -> Arc<RecordTap> {
        self.shared.master_tap.clone()
    }

    pub fn pad_tap(&self) -> Arc<RecordTap> {
        self.shared.pad_tap.clone()
    }

    pub fn push(&self, command: Command) -> bool {
        if self.shared.commands.push(command).is_ok() {
            true
        } else {
            // A full queue drops the newest command rather than blocking the
            // caller; the next UI sync re-sends the authoritative state.
            false
        }
    }

    pub fn set_playing(&self, playing: bool) {
        self.shared.playing.store(playing, Ordering::Release);
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Acquire)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn configure_clip(
        &self,
        pad: usize,
        kind: i32,
        root: i32,
        length: i32,
        gain: f32,
        pan: f32,
        fx_type: usize,
        fx_enabled: bool,
        sample_loop: bool,
        mute: bool,
        solo: bool,
        params: &[f32; 32],
    ) {
        let mut command = Command::new(CommandType::Configure, pad);
        command.clip_kind = kind.clamp(0, ClipKind::Plugin.ordinal()) as u8;
        command.root = root.clamp(0, 127) as i16;
        command.length = length.clamp(1, 16) as i16;
        command.fx_type = (fx_type.min(FX_COUNT - 1)) as i16;
        command.fx_enabled = fx_enabled;
        command.flags = (if sample_loop { 0x08 } else { 0 })
            | (if mute { 0x01 } else { 0 })
            | (if solo { 0x02 } else { 0 });
        command.gain = gain.clamp(0.0, 1.5);
        command.pan = pan.clamp(-1.0, 1.0);
        command.params = *params;
        self.push(command);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_params(
        &self,
        pad: usize,
        params: &[f32; 32],
        gain: f32,
        pan: f32,
        mute: bool,
        solo: bool,
        fx_enabled: bool,
    ) {
        let mut command = Command::new(CommandType::Params, pad);
        command.flags = (if mute { 0x01 } else { 0 }) | (if solo { 0x02 } else { 0 });
        command.fx_enabled = fx_enabled;
        command.gain = gain.clamp(0.0, 1.5);
        command.pan = pan.clamp(-1.0, 1.0);
        command.params = *params;
        self.push(command);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_step(
        &self,
        pad: usize,
        step: usize,
        gate: bool,
        note: i32,
        velocity: i32,
        accent: bool,
        slide: bool,
    ) {
        let mut command = Command::new(CommandType::Step, pad);
        command.step = step as i16;
        command.flags =
            (if gate { 1 } else { 0 }) | (if accent { 2 } else { 0 }) | (if slide { 4 } else { 0 });
        command.note = note.clamp(0, 127) as i16;
        command.velocity = velocity.clamp(1, 127) as i16;
        self.push(command);
    }

    pub fn set_drum_step(&self, pad: usize, voice: usize, step: usize, on: bool, velocity: i32) {
        let mut command = Command::new(CommandType::DrumStep, pad);
        command.voice = voice as i16;
        command.step = step as i16;
        command.flags = if on { 1 } else { 0 };
        command.velocity = velocity.clamp(1, 127) as i16;
        self.push(command);
    }

    pub fn set_midi_note(
        &self,
        pad: usize,
        on: bool,
        note: i32,
        start: i32,
        length: i32,
        velocity: i32,
    ) {
        let mut command = Command::new(CommandType::MidiNote, pad);
        command.flags = if on { 1 } else { 0 };
        command.note = note.clamp(0, 127) as i16;
        command.step = start.clamp(0, 15) as i16;
        command.length = length.clamp(1, 16) as i16;
        command.velocity = velocity.clamp(1, 127) as i16;
        self.push(command);
    }

    pub fn clear_midi_notes(&self, pad: usize) {
        self.push(Command::new(CommandType::ClearMidi, pad));
    }

    pub fn preview_note(&self, pad: usize, on: bool, note: i32, velocity: i32) {
        let mut command = Command::new(CommandType::Preview, pad);
        command.flags = if on { 1 } else { 0 };
        command.note = note.clamp(0, 127) as i16;
        command.velocity = velocity.clamp(1, 127) as i16;
        self.push(command);
    }

    pub fn launch_pad(&self, pad: usize) {
        self.push(Command::new(CommandType::Launch, pad));
    }

    pub fn stop_pad(&self, pad: usize) {
        self.push(Command::new(CommandType::Stop, pad));
    }

    pub fn launch_scene(&self, row: usize) {
        self.push(Command::new(CommandType::LaunchScene, row));
    }

    pub fn stop_scene(&self, row: usize) {
        self.push(Command::new(CommandType::StopScene, row));
    }

    pub fn stop_all(&self) {
        self.push(Command::new(CommandType::StopAll, 0));
    }

    pub fn panic(&self) {
        self.push(Command::new(CommandType::Panic, 0));
    }

    pub fn clear_clip(&self, pad: usize) {
        self.push(Command::new(CommandType::Clear, pad));
    }

    /// Publish a decoded sample to a pad. The audio thread only reads a pointer.
    pub fn load_sample(&self, pad: usize, sample: Arc<SampleData>) {
        self.shared.samples.publish(pad, sample);
    }

    pub fn clear_sample(&self, pad: usize) {
        self.shared.samples.clear(pad);
    }

    pub fn set_pad_record_target(&self, pad: Option<usize>) {
        self.shared
            .pad_record_index
            .store(pad.map(|p| p as i32).unwrap_or(-1), Ordering::Release);
    }
}
