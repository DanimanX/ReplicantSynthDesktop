//! The plugin worker thread.
//!
//! Third-party plugin code runs here, never on the audio callback. The worker
//! owns the processor, drains parameter changes, and turns each incoming audio
//! block around as fast as it can. If a plugin blocks or takes too long, the
//! bridge simply underruns for that block instead of stalling the device.

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};
use crossbeam_queue::ArrayQueue;
use replicantsynth_core::engine::PluginBlock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// What a backend has to provide to run on the worker thread.
pub trait PluginProcessor: Send {
    /// Render one block in place.
    fn process(&mut self, block: &mut PluginBlock);
    /// Apply a normalized parameter value before the next block.
    fn set_param(&mut self, id: u32, value: f32);
    /// Called once when the worker is shutting down, before the processor drops.
    fn deactivate(&mut self) {}
}

const PARAM_QUEUE: usize = 256;

pub struct PluginWorker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    params: Arc<ArrayQueue<(u32, f32)>>,
    pub blocks: Arc<AtomicU64>,
    pub overruns: Arc<AtomicU64>,
}

impl PluginWorker {
    pub fn spawn(
        mut processor: Box<dyn PluginProcessor>,
        blocks_in: Receiver<PluginBlock>,
        blocks_out: Sender<PluginBlock>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let params: Arc<ArrayQueue<(u32, f32)>> = Arc::new(ArrayQueue::new(PARAM_QUEUE));
        let blocks = Arc::new(AtomicU64::new(0));
        let overruns = Arc::new(AtomicU64::new(0));

        let thread_stop = stop.clone();
        let thread_params = params.clone();
        let thread_blocks = blocks.clone();
        let thread_overruns = overruns.clone();

        let thread = std::thread::Builder::new()
            .name("replicant-plugin".to_string())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    let mut block = match blocks_in.recv_timeout(Duration::from_millis(40)) {
                        Ok(block) => block,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };

                    while let Some((id, value)) = thread_params.pop() {
                        processor.set_param(id, value);
                    }

                    processor.process(&mut block);
                    thread_blocks.fetch_add(1, Ordering::Relaxed);

                    // Never block on the way back: a full queue means the audio
                    // thread stopped reading (transport stopped, or the engine is
                    // being torn down). Blocking here would deadlock `stop`.
                    match blocks_out.try_send(block) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {
                            thread_overruns.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
                // Drop the processor here, on the worker thread, before the
                // runtime is dropped on the UI thread.
                processor.deactivate();
                drop(processor);
            })
            .expect("plugin worker thread");

        Self {
            stop,
            thread: Some(thread),
            params,
            blocks,
            overruns,
        }
    }

    /// Queue a normalized parameter change for the next block.
    pub fn set_param(&self, id: u32, value: f32) {
        // On overflow the oldest change is the one to lose.
        let _ = self.params.force_push((id, value));
    }

    pub fn blocks_processed(&self) -> u64 {
        self.blocks.load(Ordering::Relaxed)
    }

    /// Blocks the audio thread never collected, because it had stopped reading.
    pub fn overruns(&self) -> u64 {
        self.overruns.load(Ordering::Relaxed)
    }

    /// Signal the worker and wait for it to drop the plugin.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for PluginWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use replicantsynth_core::engine::bridge_pair;

    struct Doubler {
        gain: f32,
    }

    impl PluginProcessor for Doubler {
        fn process(&mut self, block: &mut PluginBlock) {
            for i in 0..block.frames {
                block.left[i] *= self.gain;
                block.right[i] *= self.gain;
            }
        }

        fn set_param(&mut self, _id: u32, value: f32) {
            self.gain = value;
        }
    }

    #[test]
    fn a_worker_renders_and_applies_parameters() {
        use replicantsynth_core::engine::PLUGIN_BLOCK_FRAMES;
        use std::time::{Duration, Instant};

        let (mut bridge, rx, tx) = bridge_pair(7);
        let mut worker = PluginWorker::spawn(Box::new(Doubler { gain: 1.0 }), rx, tx);

        // Let the thread start and prime its queue, the way a real engine does
        // before audio begins to flow.
        std::thread::sleep(Duration::from_millis(50));
        worker.set_param(0, 2.0);

        // Then feed it in something close to real time: 128 frames at 48 kHz is
        // 2.7 ms, which is the interval the audio callback would use.
        let mut peak = 0.0f32;
        let deadline = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < deadline {
            for _ in 0..PLUGIN_BLOCK_FRAMES {
                let (left, _) = bridge.tick(0.25, 0.25);
                peak = peak.max(left);
            }
            std::thread::sleep(Duration::from_micros(2700));
        }

        assert!(worker.blocks_processed() > 0, "worker ran");
        assert!(
            bridge.stats.blocks_received > 0,
            "blocks came back: {:?}",
            bridge.stats
        );
        assert!(peak >= 0.5, "gain applied, peak {peak}");
        worker.stop();
    }
}
