//! Hosted-plugin plumbing for the audio thread.
//!
//! A plugin runs on its own worker thread: the audio callback never locks,
//! allocates or calls into third-party code. Instead it pushes fixed-size
//! blocks through a bounded channel and reads the block the worker finished one
//! step earlier, so a misbehaving plugin can only ever cost one block of audio.
//!
//! The trade-off is one block of latency (128 frames ≈ 2.7 ms at 48 kHz).

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};

/// Frames per plugin block. Small enough that the pipeline adds only a couple of
/// milliseconds, large enough that the channel traffic is negligible.
pub const PLUGIN_BLOCK_FRAMES: usize = 128;
/// Note events carried by one block.
pub const PLUGIN_BLOCK_EVENTS: usize = 32;
/// Parameter changes carried by one block.
pub const PLUGIN_BLOCK_PARAMS: usize = 8;
/// Queued blocks in each direction.
pub const PLUGIN_QUEUE_DEPTH: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct NoteEvent {
    pub on: bool,
    pub note: u8,
    pub velocity: u8,
    /// Offset in frames from the start of the block.
    pub offset: u16,
}

impl NoteEvent {
    pub fn note_on(note: u8, velocity: u8, offset: u16) -> Self {
        Self {
            on: true,
            note,
            velocity: velocity.max(1),
            offset,
        }
    }

    pub fn note_off(note: u8, offset: u16) -> Self {
        Self {
            on: false,
            note,
            velocity: 0,
            offset,
        }
    }
}

/// A normalized parameter change bound for one plugin parameter.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParamEvent {
    pub id: u32,
    pub value: f32,
}

impl ParamEvent {
    pub const NONE: ParamEvent = ParamEvent {
        id: u32::MAX,
        value: 0.0,
    };
}

/// One fixed-size slice of audio plus the events that belong to it.
#[derive(Clone)]
pub struct PluginBlock {
    pub frames: usize,
    pub left: [f32; PLUGIN_BLOCK_FRAMES],
    pub right: [f32; PLUGIN_BLOCK_FRAMES],
    pub events: [NoteEvent; PLUGIN_BLOCK_EVENTS],
    pub event_count: usize,
    pub params: [ParamEvent; PLUGIN_BLOCK_PARAMS],
    pub param_count: usize,
    /// Playhead in frames, so plugins that follow the transport stay in sync.
    pub position: u64,
    pub bpm: f32,
    pub playing: bool,
}

impl Default for PluginBlock {
    fn default() -> Self {
        Self {
            frames: 0,
            left: [0.0; PLUGIN_BLOCK_FRAMES],
            right: [0.0; PLUGIN_BLOCK_FRAMES],
            events: [NoteEvent::default(); PLUGIN_BLOCK_EVENTS],
            event_count: 0,
            params: [ParamEvent::NONE; PLUGIN_BLOCK_PARAMS],
            param_count: 0,
            position: 0,
            bpm: 120.0,
            playing: false,
        }
    }
}

impl PluginBlock {
    /// Reset the per-block content while keeping the allocation.
    pub fn clear(&mut self) {
        self.frames = 0;
        self.event_count = 0;
        self.param_count = 0;
        self.params = [ParamEvent::NONE; PLUGIN_BLOCK_PARAMS];
        self.left = [0.0; PLUGIN_BLOCK_FRAMES];
        self.right = [0.0; PLUGIN_BLOCK_FRAMES];
    }

    pub fn push_event(&mut self, event: NoteEvent) {
        if self.event_count < PLUGIN_BLOCK_EVENTS {
            self.events[self.event_count] = event;
            self.event_count += 1;
        }
    }

    pub fn push_param(&mut self, param: ParamEvent) {
        if self.param_count < PLUGIN_BLOCK_PARAMS && param.id != u32::MAX {
            self.params[self.param_count] = param;
            self.param_count += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BridgeStats {
    pub blocks_sent: u64,
    pub blocks_received: u64,
    pub blocks_dropped: u64,
    pub underruns: u64,
    pub disconnected: bool,
}

/// The audio-thread side of a hosted plugin.
///
/// Every method here is non-blocking: `try_send` and `try_recv` only.
pub struct PluginBridge {
    to_worker: Sender<PluginBlock>,
    from_worker: Receiver<PluginBlock>,
    /// Recycled output blocks, so the audio thread never allocates.
    spare: Vec<PluginBlock>,
    in_block: PluginBlock,
    out_block: Option<PluginBlock>,
    out_pos: usize,
    pub stats: BridgeStats,
    pub id: u64,
    /// Transport values stamped onto every outgoing block.
    pub position: u64,
    pub bpm: f32,
    pub playing: bool,
}

impl PluginBridge {
    pub fn new(
        to_worker: Sender<PluginBlock>,
        from_worker: Receiver<PluginBlock>,
        id: u64,
    ) -> Self {
        let mut bridge = Self {
            to_worker,
            from_worker,
            spare: Vec::with_capacity(PLUGIN_QUEUE_DEPTH),
            in_block: PluginBlock::default(),
            out_block: None,
            out_pos: PLUGIN_BLOCK_FRAMES,
            stats: BridgeStats::default(),
            id,
            position: 0,
            bpm: 120.0,
            playing: false,
        };
        bridge.poll();
        bridge
    }

    /// Queue a note event for the block currently being filled.
    pub fn push_event(&mut self, event: NoteEvent) {
        let offset = self.in_block.frames.min(PLUGIN_BLOCK_FRAMES - 1) as u16;
        self.in_block.push_event(NoteEvent { offset, ..event });
    }

    pub fn push_param(&mut self, param: ParamEvent) {
        self.in_block.push_param(param);
    }

    /// Feed one input frame and read one output frame.
    ///
    /// The output belongs to the block sent one flush earlier, which is what
    /// keeps the callback from ever waiting on the plugin.
    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let out = self.read_output();

        if self.in_block.frames < PLUGIN_BLOCK_FRAMES {
            let index = self.in_block.frames;
            self.in_block.left[index] = in_l;
            self.in_block.right[index] = in_r;
            self.in_block.frames += 1;
            if self.in_block.frames == PLUGIN_BLOCK_FRAMES {
                self.flush();
                self.poll();
            }
        }

        out
    }

    #[inline]
    fn read_output(&mut self) -> (f32, f32) {
        match &self.out_block {
            Some(block) if self.out_pos < block.frames => {
                let out = (block.left[self.out_pos], block.right[self.out_pos]);
                self.out_pos += 1;
                out
            }
            _ => (0.0, 0.0),
        }
    }

    fn flush(&mut self) {
        if self.in_block.frames == 0 {
            return;
        }
        let spare = self.take_spare();
        let mut block = std::mem::replace(&mut self.in_block, spare);
        block.position = self.position;
        block.bpm = self.bpm;
        block.playing = self.playing;
        if let Err(error) = self.to_worker.try_send(block) {
            self.stats.blocks_dropped += 1;
            match error {
                TrySendError::Full(returned) | TrySendError::Disconnected(returned) => {
                    self.recycle(returned)
                }
            }
        } else {
            self.stats.blocks_sent += 1;
        }
    }

    fn poll(&mut self) {
        match self.from_worker.try_recv() {
            Ok(block) => {
                if let Some(old) = self.out_block.replace(block) {
                    self.recycle(old);
                }
                self.out_pos = 0;
                self.stats.blocks_received += 1;
            }
            Err(TryRecvError::Empty) => {
                self.out_pos = PLUGIN_BLOCK_FRAMES;
                self.stats.underruns += 1;
            }
            Err(TryRecvError::Disconnected) => {
                self.out_pos = PLUGIN_BLOCK_FRAMES;
                self.stats.disconnected = true;
            }
        }
    }

    fn take_spare(&mut self) -> PluginBlock {
        match self.spare.pop() {
            Some(mut block) => {
                block.clear();
                block
            }
            None => PluginBlock::default(),
        }
    }

    fn recycle(&mut self, mut block: PluginBlock) {
        if self.spare.len() < PLUGIN_QUEUE_DEPTH {
            block.clear();
            self.spare.push(block);
        }
    }
}

/// Create both ends of a bridge: the audio-thread handle plus the worker's
/// channels.
pub fn bridge_pair(id: u64) -> (PluginBridge, Receiver<PluginBlock>, Sender<PluginBlock>) {
    let (to_worker_tx, to_worker_rx) = bounded(PLUGIN_QUEUE_DEPTH);
    let (from_worker_tx, from_worker_rx) = bounded(PLUGIN_QUEUE_DEPTH);
    (
        PluginBridge::new(to_worker_tx, from_worker_rx, id),
        to_worker_rx,
        from_worker_tx,
    )
}

/// A message from the UI thread to the engine about hosted plugins.
///
/// Plugins are not `Copy`, so they travel on their own channel instead of the
/// fixed-size command queue.
pub enum PluginHostCommand {
    /// Attach a freshly built bridge to a pad.
    ///
    /// The bridge is boxed because it carries its own block buffers, and a
    /// [`PluginBridge`] inline would make every `Remove` pay for that size.
    Install { pad: u8, bridge: Box<PluginBridge> },
    /// Detach the bridge from a pad; the engine hands it back for the UI to drop.
    Remove { pad: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed the bridge at roughly real time, the way the device callback does.
    ///
    /// A tight loop would outrun the thread start-up and produce nothing but
    /// drops; the pacing is what makes this test deterministic.
    fn pump(bridge: &mut PluginBridge, blocks: usize, value: f32) -> f32 {
        let mut heard = 0.0f32;
        for _ in 0..blocks {
            for _ in 0..PLUGIN_BLOCK_FRAMES {
                let (left, _) = bridge.tick(value, value);
                heard += left;
            }
            std::thread::sleep(std::time::Duration::from_micros(1_500));
        }
        heard
    }

    fn run_bridge(transform: impl Fn(f32) -> f32 + Send + 'static) -> PluginBridge {
        let (bridge, rx, tx) = bridge_pair(1);
        std::thread::spawn(move || {
            while let Ok(mut block) = rx.recv() {
                for i in 0..block.frames {
                    block.left[i] = transform(block.left[i]);
                    block.right[i] = transform(block.right[i]);
                }
                if tx.send(block).is_err() {
                    break;
                }
            }
        });
        bridge
    }

    #[test]
    fn a_bridge_round_trips_audio_one_block_late() {
        let mut bridge = run_bridge(|value| value * 0.5);
        let heard = pump(&mut bridge, 12, 1.0);
        assert!(bridge.stats.blocks_sent >= 8, "{:?}", bridge.stats);
        assert!(heard > 0.0, "the transformed audio came back");
        assert!(
            bridge.stats.blocks_received > 0,
            "worker returned blocks: {:?}",
            bridge.stats
        );
        assert!(!bridge.stats.disconnected);
    }

    #[test]
    fn a_bridge_without_a_worker_stays_silent_and_never_blocks() {
        let (mut bridge, _rx, _tx) = bridge_pair(2);
        for _ in 0..PLUGIN_BLOCK_FRAMES * 8 {
            assert_eq!(bridge.tick(1.0, 1.0), (0.0, 0.0));
        }
        assert!(bridge.stats.blocks_sent > 0);
        assert_eq!(bridge.stats.blocks_received, 0);
    }

    #[test]
    fn events_are_attached_to_the_block_being_filled() {
        let (mut bridge, rx, _tx) = bridge_pair(3);
        for _ in 0..10 {
            bridge.tick(0.0, 0.0);
        }
        bridge.push_event(NoteEvent::note_on(60, 100, 0));
        bridge.push_param(ParamEvent { id: 7, value: 0.25 });
        for _ in 0..PLUGIN_BLOCK_FRAMES {
            bridge.tick(0.0, 0.0);
        }
        let block = rx.try_recv().expect("a block was flushed");
        assert_eq!(block.event_count, 1);
        assert_eq!(block.events[0].note, 60);
        assert_eq!(block.events[0].offset, 10);
        assert_eq!(block.param_count, 1);
        assert_eq!(block.params[0].id, 7);
    }
}
