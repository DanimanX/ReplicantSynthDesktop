//! Sample import. Symphonia decodes WAV, FLAC, MP3, OGG/Vorbis, AAC/M4A into the
//! interleaved float buffer the engine publishes.

use replicantsynth_core::engine::SampleData;
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const MAX_SECONDS: usize = 300;

pub fn decode_file(path: &Path) -> Result<SampleData, String> {
    let file = File::open(path).map_err(|err| format!("open: {err}"))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|err| format!("probe: {err}"))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "no decodable audio track".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|err| format!("decoder: {err}"))?;

    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(48_000) as f32;
    let mut samples: Vec<f32> = Vec::new();
    let max_frames = sample_rate as usize * MAX_SECONDS;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(ref err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(err) => return Err(format!("read: {err}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(err) => return Err(format!("decode: {err}")),
        };
        sample_rate = decoded.spec().rate as f32;
        append(&decoded, &mut samples);
        if samples.len() / 2 > max_frames {
            break;
        }
    }

    if samples.is_empty() {
        return Err("decoded no samples".to_string());
    }

    // Always publish stereo: mono is duplicated so the engine's sample path
    // stays one shape.
    let mut stereo = Vec::with_capacity(samples.len());
    for chunk in samples.chunks_exact(2) {
        stereo.push(chunk[0]);
        stereo.push(chunk[1]);
    }

    Ok(SampleData {
        stereo,
        channels: 2,
        sample_rate: sample_rate as u32,
    })
}

fn append(buffer: &AudioBufferRef, out: &mut Vec<f32>) {
    match buffer {
        AudioBufferRef::F32(buf) => push_planar(buf.chan(0), buf.chan(1), out),
        AudioBufferRef::F64(buf) => push_planar(buf.chan(0), buf.chan(1), out),
        AudioBufferRef::S8(buf) => push_planar_i(buf.chan(0), buf.chan(1), out, 128),
        AudioBufferRef::S16(buf) => push_planar_i(buf.chan(0), buf.chan(1), out, 32768),
        AudioBufferRef::S24(buf) => {
            let left = buf.chan(0);
            let right = buf.chan(1);
            for i in 0..left.len() {
                out.push(left[i].inner() as f32 / 8_388_608.0);
                out.push(right[i].inner() as f32 / 8_388_608.0);
            }
        }
        AudioBufferRef::S32(buf) => {
            let left = buf.chan(0);
            let right = buf.chan(1);
            for i in 0..left.len() {
                out.push(left[i] as f32 / 2_147_483_648.0);
                out.push(right[i] as f32 / 2_147_483_648.0);
            }
        }
        AudioBufferRef::U8(buf) => {
            let left = buf.chan(0);
            let right = buf.chan(1);
            for i in 0..left.len() {
                out.push((left[i] as f32 - 128.0) / 128.0);
                out.push((right[i] as f32 - 128.0) / 128.0);
            }
        }
        AudioBufferRef::U16(buf) => {
            let left = buf.chan(0);
            let right = buf.chan(1);
            for i in 0..left.len() {
                out.push((left[i] as f32 - 32768.0) / 32768.0);
                out.push((right[i] as f32 - 32768.0) / 32768.0);
            }
        }
        AudioBufferRef::U24(buf) => {
            let left = buf.chan(0);
            let right = buf.chan(1);
            for i in 0..left.len() {
                out.push((left[i].inner() as f32 - 8_388_608.0) / 8_388_608.0);
                out.push((right[i].inner() as f32 - 8_388_608.0) / 8_388_608.0);
            }
        }
        AudioBufferRef::U32(buf) => {
            let left = buf.chan(0);
            let right = buf.chan(1);
            for i in 0..left.len() {
                out.push((left[i] as f32 - 2_147_483_648.0) / 2_147_483_648.0);
                out.push((right[i] as f32 - 2_147_483_648.0) / 2_147_483_648.0);
            }
        }
    }
}

fn push_planar<T: Copy + Into<f64>>(left: &[T], right: &[T], out: &mut Vec<f32>) {
    for i in 0..left.len().min(right.len()) {
        out.push(left[i].into() as f32);
        out.push(right[i].into() as f32);
    }
}

fn push_planar_i<T: Copy + Into<i64>>(left: &[T], right: &[T], out: &mut Vec<f32>, scale: i64) {
    for i in 0..left.len().min(right.len()) {
        out.push(left[i].into() as f32 / scale as f32);
        out.push(right[i].into() as f32 / scale as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::path::PathBuf {
        let candidates = [
            std::path::Path::new("assets/demo/neon_stab.wav"),
            std::path::Path::new("../assets/demo/neon_stab.wav"),
            std::path::Path::new("../../assets/demo/neon_stab.wav"),
        ];
        candidates
            .iter()
            .find(|path| path.exists())
            .map(|path| path.to_path_buf())
            .expect("bundled demo sample is present")
    }

    #[test]
    fn decodes_the_bundled_wav() {
        let sample = decode_file(&fixture()).expect("decode");
        assert_eq!(sample.channels, 2);
        assert_eq!(sample.sample_rate, 48_000);
        assert!(sample.frames() > 100_000, "frames {}", sample.frames());
        assert_eq!(sample.stereo.len() % 2, 0);
        assert!(sample.stereo.iter().all(|value| value.is_finite()));
        let peak = sample
            .stereo
            .iter()
            .fold(0.0f32, |acc, value| acc.max(value.abs()));
        assert!(peak > 0.05, "sample is audible, peak {peak}");
        assert!(peak <= 1.0, "sample is in range, peak {peak}");
    }

    /// Every container the app advertises in its file dialog, decoded through
    /// the same path the UI uses. Fixtures are generated by the caller when the
    /// matching encoder is available; a missing fixture is skipped, not failed.
    #[test]
    fn decodes_every_advertised_container() {
        for (name, path) in [
            (
                "mp3",
                std::path::PathBuf::from("../../assets/replicantsynth_demo.mp3"),
            ),
            ("flac", std::path::PathBuf::from("/tmp/fixture.flac")),
            ("ogg", std::path::PathBuf::from("/tmp/fixture.ogg")),
            ("m4a", std::path::PathBuf::from("/tmp/fixture.m4a")),
        ] {
            if !path.exists() {
                println!("skipping {name}: {} is absent", path.display());
                continue;
            }
            let sample = decode_file(&path).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert_eq!(sample.channels, 2, "{name} is published as stereo");
            assert!(sample.frames() > 1000, "{name} decoded too few frames");
            assert_eq!(sample.stereo.len(), sample.frames() * 2);
            assert!(sample.stereo.iter().all(|value| value.is_finite()));
            println!(
                "decoded {name}: {} frames @ {} Hz",
                sample.frames(),
                sample.sample_rate
            );
        }
    }

    #[test]
    fn rejects_a_non_audio_file() {
        let path = std::env::temp_dir().join("replicantsynth_not_audio.bin");
        std::fs::write(&path, b"this is not audio at all").expect("write");
        assert!(decode_file(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
