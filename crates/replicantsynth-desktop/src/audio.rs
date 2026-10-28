//! Audio output: one cpal stream owns the clip engine.
//!
//! The engine is moved into the device callback, so the UI only ever talks to
//! it through `EngineHandles` (commands, atomics, sample publication).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use replicantsynth_core::engine::{create_engine, EngineHandles};
use replicantsynth_core::ClipEngine;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct AudioOptions {
    pub device_name: Option<String>,
    pub sample_rate: Option<u32>,
}

pub struct AudioHost {
    stream: Option<cpal::Stream>,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub status: String,
    pub error: Option<String>,
    error_slot: Arc<Mutex<Option<String>>>,
}

impl AudioHost {
    pub fn silent(status: &str) -> Self {
        Self {
            stream: None,
            device_name: "—".to_string(),
            sample_rate: 48_000,
            channels: 2,
            status: status.to_string(),
            error: None,
            error_slot: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    pub fn poll_error(&mut self) {
        if let Ok(mut slot) = self.error_slot.try_lock() {
            if let Some(message) = slot.take() {
                self.error = Some(message.clone());
                self.status = format!("AUDIO ERROR: {message}");
            }
        }
    }
}

/// Enumerate the output devices of the default host.
pub fn list_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for device in devices {
            if let Ok(name) = device.name() {
                names.push(name);
            }
        }
    }
    names
}

/// Start audio for `handles`, replacing whatever stream was running.
///
/// A fresh engine is created (the old one belongs to the old stream). The
/// caller re-syncs the project and samples afterwards.
pub fn start_audio(options: &AudioOptions) -> (AudioHost, Option<EngineHandles>) {
    let host = cpal::default_host();
    let device = match &options.device_name {
        Some(name) => host.output_devices().ok().and_then(|mut devices| {
            devices.find(|d| d.name().ok().as_deref() == Some(name.as_str()))
        }),
        None => None,
    }
    .or_else(|| host.default_output_device());

    let Some(device) = device else {
        return (
            AudioHost::silent("no output device — the UI still runs, audio is offline"),
            None,
        );
    };

    let device_name = device.name().unwrap_or_else(|_| "output".to_string());
    let supported = match device.default_output_config() {
        Ok(config) => config,
        Err(err) => {
            return (
                AudioHost::silent(&format!("device config failed: {err}")),
                None,
            )
        }
    };

    let sample_format = supported.sample_format();
    let mut stream_config: cpal::StreamConfig = supported.config();
    if let Some(rate) = options.sample_rate {
        stream_config.sample_rate = cpal::SampleRate(rate);
    }
    let sample_rate = stream_config.sample_rate.0;
    let channels = stream_config.channels;

    let max_block = 1024usize;
    let (engine, handles) = create_engine(sample_rate as f64, max_block);
    let error_slot = Arc::new(Mutex::new(None));
    let scratch = vec![0.0f32; max_block * 2];

    let built = match sample_format {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &stream_config, engine, scratch, error_slot.clone())
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, &stream_config, engine, scratch, error_slot.clone())
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, &stream_config, engine, scratch, error_slot.clone())
        }
        cpal::SampleFormat::I32 => {
            build_stream::<i32>(&device, &stream_config, engine, scratch, error_slot.clone())
        }
        cpal::SampleFormat::F64 => {
            build_stream::<f64>(&device, &stream_config, engine, scratch, error_slot.clone())
        }
        _other => Err(cpal::BuildStreamError::StreamConfigNotSupported),
    };

    match built {
        Ok(stream) => {
            if let Err(err) = stream.play() {
                return (
                    AudioHost {
                        stream: None,
                        device_name,
                        sample_rate,
                        channels,
                        status: format!("stream play failed: {err}"),
                        error: Some(err.to_string()),
                        error_slot,
                    },
                    Some(handles),
                );
            }
            (
                AudioHost {
                    stream: Some(stream),
                    device_name,
                    sample_rate,
                    channels,
                    status: "RUNNING".to_string(),
                    error: None,
                    error_slot,
                },
                Some(handles),
            )
        }
        Err(err) => (
            AudioHost {
                stream: None,
                device_name,
                sample_rate,
                channels,
                status: format!("stream build failed: {err}"),
                error: Some(err.to_string()),
                error_slot,
            },
            Some(handles),
        ),
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut engine: ClipEngine,
    mut scratch: Vec<f32>,
    error_slot: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], _info| {
            if channels == 0 {
                return;
            }
            let frames = data.len() / channels;
            if frames == 0 {
                return;
            }
            if scratch.len() < frames * 2 {
                // Only happens if the driver hands us a bigger block than the
                // engine was prepared for; grow once and keep going.
                scratch.resize(frames * 2, 0.0);
            }
            engine.process(&mut scratch[..frames * 2], frames);
            for frame in 0..frames {
                let l = scratch[frame * 2];
                let r = scratch[frame * 2 + 1];
                if channels == 1 {
                    data[frame] = T::from_sample((l + r) * 0.5);
                } else {
                    data[frame * channels] = T::from_sample(l);
                    data[frame * channels + 1] = T::from_sample(r);
                    for channel in 2..channels {
                        data[frame * channels + channel] = T::from_sample(0.0);
                    }
                }
            }
        },
        move |err| {
            if let Ok(mut slot) = error_slot.lock() {
                *slot = Some(err.to_string());
            }
        },
        None,
    )
}
