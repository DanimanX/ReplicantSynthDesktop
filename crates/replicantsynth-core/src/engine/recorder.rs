//! 24-bit capture.
//!
//! The audio thread never allocates, locks or touches the filesystem: it pushes
//! interleaved frames into a lock-free ring. A writer thread owns the file and
//! streams little-endian 24-bit PCM.

use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const RING_FRAMES: usize = 1 << 15;
const MASK: u32 = (RING_FRAMES as u32) - 1;

/// Single-producer / single-consumer frame ring shared by the audio thread and
/// the writer thread. Frames are packed as two `f32` bit patterns in one u64.
pub struct RecordTap {
    ring: Box<[AtomicU64]>,
    head: AtomicU32,
    tail: AtomicU32,
    active: AtomicBool,
    blocking: AtomicBool,
    pushed: AtomicU64,
    dropped: AtomicU64,
}

impl Default for RecordTap {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordTap {
    pub fn new() -> Self {
        Self {
            ring: (0..RING_FRAMES)
                .map(|_| AtomicU64::new(0))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            active: AtomicBool::new(false),
            blocking: AtomicBool::new(false),
            pushed: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    fn reset(&self) {
        self.head.store(0, Ordering::Relaxed);
        self.tail.store(0, Ordering::Relaxed);
        self.pushed.store(0, Ordering::Relaxed);
        self.dropped.store(0, Ordering::Relaxed);
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    /// In blocking mode a full ring spins instead of dropping frames.
    ///
    /// The audio thread must never block, so this stays off for live capture.
    /// Offline renders (the demo renderer and the test suite) push far faster
    /// than any writer thread can drain, and there correctness beats
    /// non-blocking: they set it, live capture does not.
    pub fn set_blocking(&self, blocking: bool) {
        self.blocking.store(blocking, Ordering::Release);
    }

    pub fn is_blocking(&self) -> bool {
        self.blocking.load(Ordering::Relaxed)
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn pushed(&self) -> u64 {
        self.pushed.load(Ordering::Relaxed)
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Push `frames` interleaved stereo frames. Called only from the audio thread.
    pub fn push_interleaved(&self, samples: &[f32], frames: usize) {
        if !self.is_active() || frames == 0 {
            return;
        }
        let blocking = self.blocking.load(Ordering::Relaxed);
        let mut head = self.head.load(Ordering::Relaxed);
        for i in 0..frames {
            let next = (head + 1) & MASK;
            if next == self.tail.load(Ordering::Acquire) {
                if !blocking {
                    self.dropped
                        .fetch_add((frames - i) as u64, Ordering::Relaxed);
                    self.head.store(head, Ordering::Release);
                    self.pushed.fetch_add(i as u64, Ordering::Relaxed);
                    return;
                }
                while next == self.tail.load(Ordering::Acquire) {
                    std::hint::spin_loop();
                }
            }
            let l = samples[i * 2].to_bits() as u64;
            let r = samples[i * 2 + 1].to_bits() as u64;
            self.ring[head as usize].store((l << 32) | r, Ordering::Relaxed);
            head = next;
        }
        self.head.store(head, Ordering::Release);
        self.pushed.fetch_add(frames as u64, Ordering::Relaxed);
    }

    fn pop(&self) -> Option<(f32, f32)> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }
        let packed = self.ring[tail as usize].load(Ordering::Relaxed);
        self.tail.store((tail + 1) & MASK, Ordering::Release);
        let l = f32::from_bits((packed >> 32) as u32);
        let r = f32::from_bits((packed & 0xFFFF_FFFF) as u32);
        Some((l, r))
    }
}

fn put_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn wav_header(sample_rate: u32, channels: u16, data_bytes: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    put_u32(&mut h, 4, 36 + data_bytes);
    h[8..16].copy_from_slice(b"WAVEfmt ");
    put_u32(&mut h, 16, 16);
    put_u16(&mut h, 20, 1); // PCM
    put_u16(&mut h, 22, channels);
    put_u32(&mut h, 24, sample_rate);
    put_u32(&mut h, 28, sample_rate * channels as u32 * 3);
    put_u16(&mut h, 32, channels * 3);
    put_u16(&mut h, 34, 24);
    h[36..40].copy_from_slice(b"data");
    put_u32(&mut h, 40, data_bytes);
    h
}

/// Capture session owned by the UI thread; dropping it stops and patches the file.
pub struct RecordingWriter {
    tap: Arc<RecordTap>,
    thread: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
    frames_written: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    path: PathBuf,
    data_bytes: usize,
    sample_rate: u32,
    channels: u16,
}

impl RecordingWriter {
    /// Start a capture.
    ///
    /// `blocking` must be `false` for live capture (the audio callback may never
    /// wait) and `true` for offline rendering, where the producer is faster than
    /// the disk and dropping would silently truncate the take.
    pub fn start(
        path: &Path,
        tap: Arc<RecordTap>,
        sample_rate: u32,
        channels: u16,
        blocking: bool,
    ) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let channels = channels.clamp(1, 2);
        let mut file = File::create(path)?;
        file.write_all(&wav_header(sample_rate, channels, 0))?;

        tap.reset();
        tap.set_blocking(blocking);
        tap.set_active(true);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let frames_written = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicBool::new(false));

        let thread_tap = tap.clone();
        let thread_stop = stop_flag.clone();
        let thread_frames = frames_written.clone();
        let thread_failed = failed.clone();

        let thread = std::thread::Builder::new()
            .name("replicant-capture".to_string())
            .spawn(move || {
                let mut file = file;
                let mut scratch = vec![0u8; 1024 * 3 * 2];
                let mut last_data = Instant::now();
                loop {
                    let mut count = 0usize;
                    while count < 1024 {
                        match thread_tap.pop() {
                            Some((l, r)) => {
                                let li = (l.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                                let ri = (r.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                                let base = count * 3 * channels as usize;
                                scratch[base] = (li & 0xFF) as u8;
                                scratch[base + 1] = ((li >> 8) & 0xFF) as u8;
                                scratch[base + 2] = ((li >> 16) & 0xFF) as u8;
                                if channels == 2 {
                                    scratch[base + 3] = (ri & 0xFF) as u8;
                                    scratch[base + 4] = ((ri >> 8) & 0xFF) as u8;
                                    scratch[base + 5] = ((ri >> 16) & 0xFF) as u8;
                                }
                                count += 1;
                            }
                            None => break,
                        }
                    }

                    if count > 0 {
                        last_data = Instant::now();
                        let bytes = count * 3 * channels as usize;
                        if file.write_all(&scratch[..bytes]).is_err() {
                            thread_failed.store(true, Ordering::Relaxed);
                        }
                        thread_frames.fetch_add(count as u64, Ordering::Relaxed);
                        continue;
                    }

                    if thread_stop.load(Ordering::Acquire)
                        && last_data.elapsed() > Duration::from_millis(25)
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }

                // Patch the RIFF/data sizes now that the take length is known.
                if let Ok(len) = file.seek(SeekFrom::End(0)) {
                    let data = len.saturating_sub(44) as u32;
                    let header = wav_header(sample_rate, channels, data);
                    let _ = file.seek(SeekFrom::Start(0));
                    let _ = file.write_all(&header);
                }
                let _ = file.flush();
            })?;

        Ok(Self {
            tap,
            thread: Some(thread),
            stop_flag,
            frames_written,
            failed,
            path: path.to_path_buf(),
            data_bytes: 0,
            sample_rate,
            channels,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn frames(&self) -> u64 {
        self.frames_written.load(Ordering::Relaxed)
    }

    pub fn dropped(&self) -> u64 {
        self.tap.dropped()
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn is_active(&self) -> bool {
        self.thread.is_some()
    }

    /// Stop the writer, flush the ring and patch the WAV header.
    pub fn stop(mut self) -> Self {
        self.finish();
        self
    }

    fn finish(&mut self) {
        if self.thread.is_none() {
            return;
        }
        self.stop_flag.store(true, Ordering::Release);
        self.tap.set_active(false);
        self.tap.set_blocking(false);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        self.data_bytes =
            self.frames_written.load(Ordering::Relaxed) as usize * 3 * self.channels as usize;
        let _ = (self.sample_rate, self.data_bytes);
    }
}

impl Drop for RecordingWriter {
    fn drop(&mut self) {
        self.finish();
    }
}
