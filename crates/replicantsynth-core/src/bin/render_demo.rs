//! Offline demo renderer.
//!
//! Builds the factory demo project, renders an arrangement through the real
//! clip engine and captures the master with the same 24-bit writer the app
//! uses. No audio device is opened, so this runs on CI.
//!
//! ```text
//! cargo run --release -p replicantsynth-core --bin render-demo -- \
//!     --out assets/replicantsynth_demo.wav --seconds 42
//! ```

use replicantsynth_core::engine::{create_engine, sync_project, RecordingWriter, SampleData};
use replicantsynth_core::model::{demo_project, ClipKind};
use std::path::PathBuf;
use std::sync::Arc;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK: usize = 512;

struct Args {
    out: PathBuf,
    seconds: f64,
    sample: Option<PathBuf>,
    bpm: f32,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            out: PathBuf::from("assets/replicantsynth_demo.wav"),
            seconds: 42.0,
            sample: None,
            bpm: 126.0,
        }
    }
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--out" => {
                if let Some(value) = argv.next() {
                    args.out = PathBuf::from(value);
                }
            }
            "--seconds" => {
                if let Some(value) = argv.next() {
                    args.seconds = value.parse().unwrap_or(args.seconds);
                }
            }
            "--bpm" => {
                if let Some(value) = argv.next() {
                    args.bpm = value.parse().unwrap_or(args.bpm);
                }
            }
            "--sample" => {
                if let Some(value) = argv.next() {
                    args.sample = Some(PathBuf::from(value));
                }
            }
            other => eprintln!("unknown argument: {other}"),
        }
    }
    args
}

fn main() {
    let args = parse_args();
    let sample_path = args.sample.as_ref().map(|path| path.display().to_string());
    let mut project = demo_project(sample_path.as_deref());
    project.bpm = args.bpm;

    let (mut engine, handles) = create_engine(SAMPLE_RATE as f64, BLOCK);

    if let Some(path) = &args.sample {
        match read_wav(path) {
            Ok(sample) => {
                let frames = sample.frames();
                let data = Arc::new(SampleData {
                    stereo: sample.samples,
                    channels: sample.channels,
                    sample_rate: sample.sample_rate,
                });
                handles.load_sample(16, data.clone());
                handles.load_sample(43, data);
                println!("sample: {} ({} frames)", path.display(), frames);
            }
            Err(err) => {
                eprintln!("sample decode failed for {}: {err}", path.display());
                for clip in project.clips.iter_mut() {
                    if clip.kind == ClipKind::Sample {
                        clip.sample_missing = true;
                        clip.sample_name = Some("NO SAMPLE".to_string());
                    }
                }
            }
        }
    } else {
        println!("no --sample given: the two sample pads stay empty");
        for clip in project.clips.iter_mut() {
            if clip.kind == ClipKind::Sample {
                clip.kind = ClipKind::Empty;
            }
        }
    }

    sync_project(&handles, &project);

    if let Some(parent) = args.out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let writer = match RecordingWriter::start(&args.out, handles.master_tap(), SAMPLE_RATE, 2) {
        Ok(writer) => writer,
        Err(err) => {
            eprintln!("cannot start capture: {err}");
            std::process::exit(1);
        }
    };

    let blocks_per_bar =
        ((60.0 / args.bpm as f64 * 4.0) * SAMPLE_RATE as f64 / BLOCK as f64).round() as usize;
    let total_blocks = (args.seconds * SAMPLE_RATE as f64 / BLOCK as f64).round() as usize;

    handles.set_playing(true);
    let mut buffer = vec![0.0f32; BLOCK * 2];
    let mut current_scene: Option<usize> = None;

    for block in 0..total_blocks {
        let bar = block / blocks_per_bar.max(1);
        if block % blocks_per_bar.max(1) == 0 && bar % 2 == 0 {
            let scene = (bar / 2) % 6;
            if current_scene != Some(scene) {
                handles.launch_scene(scene);
                current_scene = Some(scene);
            }
        }
        engine.process(&mut buffer, BLOCK);
    }

    let writer = writer.stop();
    println!(
        "rendered {} s -> {} ({} frames, {} dropped)",
        args.seconds,
        args.out.display(),
        writer.frames(),
        writer.dropped()
    );
    if writer.failed() {
        eprintln!("capture reported an I/O failure");
        std::process::exit(1);
    }
}

struct WavData {
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
}

impl WavData {
    fn frames(&self) -> usize {
        self.samples.len().checked_div(self.channels).unwrap_or(0)
    }
}

/// Minimal RIFF/WAVE reader for the bundled demo sample (PCM 16/24/32 and
/// float 32). The app itself uses a full decoder.
fn read_wav(path: &std::path::Path) -> Result<WavData, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".to_string());
    }
    let mut offset = 12usize;
    let mut format = 0u16;
    let mut channels = 2usize;
    let mut sample_rate = SAMPLE_RATE;
    let mut bits = 16u16;
    let mut data: Option<&[u8]> = None;

    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let body = offset + 8;
        if body + size > bytes.len() {
            break;
        }
        match id {
            b"fmt " => {
                format = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
                channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]) as usize;
                sample_rate = u32::from_le_bytes([
                    bytes[body + 4],
                    bytes[body + 5],
                    bytes[body + 6],
                    bytes[body + 7],
                ]);
                bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            }
            b"data" => data = Some(&bytes[body..body + size]),
            _ => {}
        }
        offset = body + size + (size & 1);
    }

    let data = data.ok_or("no data chunk")?;
    let mut samples = Vec::with_capacity(data.len() / 2);
    match (format, bits) {
        (1, 16) => {
            for chunk in data.chunks_exact(2) {
                let v = i16::from_le_bytes([chunk[0], chunk[1]]);
                samples.push(v as f32 / 32768.0);
            }
        }
        (1, 24) => {
            for chunk in data.chunks_exact(3) {
                let v = ((chunk[0] as i32) << 8)
                    | ((chunk[1] as i32) << 16)
                    | ((chunk[2] as i32) << 24);
                samples.push((v >> 8) as f32 / 8_388_608.0);
            }
        }
        (1, 32) => {
            for chunk in data.chunks_exact(4) {
                let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                samples.push(v as f32 / 2_147_483_648.0);
            }
        }
        (3, 32) => {
            for chunk in data.chunks_exact(4) {
                samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        _ => return Err(format!("unsupported PCM format {format} / {bits} bit")),
    }

    Ok(WavData {
        samples,
        channels: channels.max(1),
        sample_rate,
    })
}
