//! Procedural 6-bit cymbal ROM. The hardware stores hats, crash and ride as
//! companded PCM on three HN61256P chips; the tables are rebuilt here with the
//! same modal frequencies and envelopes so the character survives the port.

use super::{fast_tanh, FastNoise, TWO_PI_F};
use std::sync::OnceLock;

pub const ROM_SAMPLE_RATE: f64 = 32_000.0;
const CH_SAMPLES: usize = 4_096;
const OH_SAMPLES: usize = 16_384;
const CRASH_SAMPLES: usize = 32_768;
const RIDE_SAMPLES: usize = 32_768;

pub struct RomData {
    pub closed_hat: Vec<i8>,
    pub open_hat: Vec<i8>,
    pub crash: Vec<i8>,
    pub ride: Vec<i8>,
}

#[inline]
fn quantize_6bit(sample: f32) -> i8 {
    let clamped = sample.clamp(-0.999, 0.999);
    let q = (clamped * 31.5).round() as i32;
    q.clamp(-32, 31) as i8
}

pub fn rom_data() -> &'static RomData {
    static ROM: OnceLock<RomData> = OnceLock::new();
    ROM.get_or_init(RomData::build)
}

impl RomData {
    fn build() -> Self {
        let mut rng = FastNoise::new(0x9090_9090);
        let mut closed_hat = vec![0i8; CH_SAMPLES];
        let mut open_hat = vec![0i8; OH_SAMPLES];
        let mut crash = vec![0i8; CRASH_SAMPLES];
        let mut ride = vec![0i8; RIDE_SAMPLES];

        const HAT_MODES: [f32; 6] = [317.0, 466.0, 628.0, 795.0, 1045.0, 1412.0];
        const SHIMMER_MODES: [f32; 5] = [2850.0, 4120.0, 6380.0, 8900.0, 12_400.0];

        for (i, slot) in closed_hat.iter_mut().enumerate() {
            let t = i as f32 / ROM_SAMPLE_RATE as f32;
            let mut metal = 0.0;
            for (m, mode) in HAT_MODES.iter().enumerate() {
                let phase = (t * mode).fract();
                metal += (if phase < 0.5 { 0.6 } else { -0.6 }) * (1.0 / (1.0 + 0.15 * m as f32));
            }
            for (s, mode) in SHIMMER_MODES.iter().enumerate() {
                metal += 0.22 * (t * mode * TWO_PI_F + 0.0).sin();
                let _ = s;
            }
            let noise = rng.next_sample() * 0.75;
            let attack_env = (t * 1800.0).min(1.0);
            let rom_env = attack_env * (-t * 22.0).exp();
            let raw = (metal * 0.45 + noise * 0.55) * rom_env;
            *slot = quantize_6bit(raw * 1.55);
        }

        for (i, slot) in open_hat.iter_mut().enumerate() {
            let t = i as f32 / ROM_SAMPLE_RATE as f32;
            let mut metal = 0.0;
            for (m, mode) in HAT_MODES.iter().enumerate() {
                let phase = (t * mode).fract();
                metal += (if phase < 0.5 { 0.55 } else { -0.55 }) * (1.0 / (1.0 + 0.12 * m as f32));
            }
            for (s, mode) in SHIMMER_MODES.iter().enumerate() {
                metal += 0.28 * (t * mode * TWO_PI_F + 0.3 * s as f32).sin();
            }
            let noise = rng.next_sample() * 0.70;
            let attack_env = (t * 1500.0).min(1.0);
            let rom_env = attack_env * (-t * 4.5).exp();
            let raw = (metal * 0.42 + noise * 0.58) * rom_env;
            *slot = quantize_6bit(raw * 1.45);
        }

        const CRASH_MODES: [f32; 12] = [
            245.0, 380.0, 512.0, 720.0, 960.0, 1340.0, 1820.0, 2450.0, 3600.0, 5200.0, 7800.0,
            11_200.0,
        ];
        for (i, slot) in crash.iter_mut().enumerate() {
            let t = i as f32 / ROM_SAMPLE_RATE as f32;
            let mut modes = 0.0;
            for (m, mode) in CRASH_MODES.iter().enumerate() {
                let detune = 1.0 + 0.003 * (t * 7.5 + m as f32).sin();
                modes += 0.18 * (t * mode * detune * TWO_PI_F).sin();
            }
            let noise = rng.next_sample() * 0.82;
            let attack_env = (t * 1200.0).min(1.0);
            let rom_env = attack_env * (-t * 2.2).exp();
            let raw = (modes * 0.40 + noise * 0.60) * rom_env;
            *slot = quantize_6bit(raw * 1.5);
        }

        const RIDE_MODES: [f32; 10] = [
            310.0, 485.0, 715.0, 1120.0, 1680.0, 2450.0, 2840.0, 3950.0, 5800.0, 9400.0,
        ];
        for (i, slot) in ride.iter_mut().enumerate() {
            let t = i as f32 / ROM_SAMPLE_RATE as f32;
            let mut modes = 0.0;
            for (m, mode) in RIDE_MODES.iter().enumerate() {
                let w = if m == 6 { 0.35 } else { 0.12 };
                modes += w * (t * mode * TWO_PI_F).sin();
            }
            let noise = rng.next_sample() * 0.48;
            let attack_env = (t * 1600.0).min(1.0);
            let rom_env = attack_env * (-t * 1.8).exp();
            let raw = (modes * 0.62 + noise * 0.38) * rom_env;
            *slot = quantize_6bit(raw * 1.4);
        }

        let _ = fast_tanh(0.0);
        Self {
            closed_hat,
            open_hat,
            crash,
            ride,
        }
    }
}

/// Expand a 6-bit ROM sample back to float, matching the vintage DAC ladder.
#[inline]
pub fn decode_6bit(value: i8) -> f32 {
    value as f32 * (1.0 / 31.5)
}
