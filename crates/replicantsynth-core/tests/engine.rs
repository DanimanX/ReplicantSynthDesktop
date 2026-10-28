//! Regression suite for the DSP core and the clip engine.
//!
//! Run with `cargo test -p replicantsynth-core --release`.

use replicantsynth_core::dsp::{
    fx_catalog, fx_info, shared_wavetable_banks, Acid303, Effect, WaveParams, WaveSynth,
    WavetableBanks, FX_COUNT,
};
use replicantsynth_core::engine::{create_engine, sync_project, EngineHandles, SampleData};
use replicantsynth_core::model::{demo_project, AcidPresets, ClipKind, DrumPresets, ProjectState};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Checker {
    checks: AtomicUsize,
    failures: AtomicUsize,
}

impl Checker {
    fn check(&self, condition: bool, what: &str) {
        self.checks.fetch_add(1, Ordering::Relaxed);
        if !condition {
            self.failures.fetch_add(1, Ordering::Relaxed);
            println!("FAIL  {what}");
        }
    }
}

struct Peaks {
    peak: f32,
    energy: f32,
    finite: bool,
}

fn render(engine: &mut replicantsynth_core::ClipEngine, seconds: f64, block: usize) -> Peaks {
    let frames = (engine.sample_rate() as f64 * seconds) as usize;
    let mut buffer = vec![0.0f32; block * 2];
    let mut peaks = Peaks {
        peak: 0.0,
        energy: 0.0,
        finite: true,
    };
    let mut done = 0;
    while done < frames {
        let n = block.min(frames - done);
        engine.process(&mut buffer, n);
        for i in 0..n {
            let l = buffer[i * 2];
            let r = buffer[i * 2 + 1];
            if !l.is_finite() || !r.is_finite() {
                peaks.finite = false;
            }
            peaks.peak = peaks.peak.max(l.abs()).max(r.abs());
            peaks.energy += l * l + r * r;
        }
        done += n;
    }
    peaks
}

fn configure_acid(handles: &EngineHandles, project: &mut ProjectState, pad: usize, root: i32) {
    let clip = &mut project.clips[pad];
    clip.kind = ClipKind::Acid;
    clip.root = root;
    clip.length = 16;
    clip.gain = 0.8;
    for (i, slot) in clip.steps.iter_mut().enumerate() {
        let gate = i % 4 == 0 || i == 6 || i == 11;
        *slot = replicantsynth_core::model::Step {
            gate,
            note: (root + (i as i32 % 12)) as u8,
            velocity: if i % 8 == 0 { 118 } else { 92 },
            accent: i % 8 == 0,
            slide: i == 6,
        };
    }
    clip.params[0] = 0.0;
    clip.params[1] = 420.0;
    clip.params[2] = 0.72;
    clip.params[3] = 0.68;
    clip.params[4] = 0.55;
    clip.params[5] = 0.82;
    clip.params[7] = 0.55;
    clip.params[8] = 0.55;
    clip.params[9] = 0.62;
    clip.params[10] = 0.9;
    let clip = project.clips[pad].clone();
    replicantsynth_core::sync_clip(handles, &clip);
}

fn configure_wave(handles: &EngineHandles, project: &mut ProjectState, pad: usize) {
    let clip = &mut project.clips[pad];
    clip.kind = ClipKind::Wave;
    clip.root = 48;
    clip.length = 16;
    clip.params[0] = 4.0;
    clip.params[1] = 0.25;
    clip.params[2] = 3.0;
    clip.params[3] = 14.0;
    clip.params[4] = 0.5;
    clip.params[5] = 1800.0;
    clip.params[6] = 0.7;
    clip.params[7] = 1.2;
    clip.params[8] = 0.005;
    clip.params[9] = 0.4;
    clip.params[10] = 0.6;
    clip.params[11] = 0.5;
    clip.params[12] = 0.2;
    clip.params[14] = 0.8;
    clip.params[16] = 2.0;
    clip.midi_notes = vec![
        replicantsynth_core::model::MidiNote::new(48, 0, 4, 100),
        replicantsynth_core::model::MidiNote::new(51, 0, 4, 100),
        replicantsynth_core::model::MidiNote::new(55, 4, 4, 100),
        replicantsynth_core::model::MidiNote::new(60, 8, 8, 100),
    ];
    let clip = project.clips[pad].clone();
    replicantsynth_core::sync_clip(handles, &clip);
}

fn configure_drums(handles: &EngineHandles, project: &mut ProjectState, pad: usize, busy: bool) {
    let clip = &mut project.clips[pad];
    clip.kind = ClipKind::Drums;
    clip.root = 36;
    clip.params[0] = 0.5;
    clip.params[1] = 0.5;
    clip.params[2] = 0.5;
    clip.params[3] = 0.5;
    clip.params[4] = 0.7;
    clip.params[5] = 0.9;
    clip.params[6] = 18000.0;
    clip.params[7] = 0.6;
    for step in [0usize, 4, 8, 12] {
        clip.set_drum_step(0, step, true);
        clip.set_drum_velocity(0, step, 122);
    }
    for step in [4usize, 12] {
        clip.set_drum_step(1, step, true);
        clip.set_drum_velocity(1, step, 112);
    }
    for step in (0..16).step_by(2) {
        clip.set_drum_step(7, step, true);
        clip.set_drum_velocity(7, step, if step % 4 == 0 { 92 } else { 70 });
    }
    if busy {
        for step in [2usize, 6, 10, 14] {
            clip.set_drum_step(8, step, true);
            clip.set_drum_velocity(8, step, 118);
        }
        clip.set_drum_step(9, 0, true);
        clip.set_drum_velocity(9, 0, 120);
        clip.set_drum_step(10, 2, true);
        clip.set_drum_velocity(10, 2, 118);
    }
    let clip = project.clips[pad].clone();
    replicantsynth_core::sync_clip(handles, &clip);
}

#[test]
fn wavetable_banks_are_band_limited() {
    let c = Checker {
        checks: AtomicUsize::new(0),
        failures: AtomicUsize::new(0),
    };
    let banks = shared_wavetable_banks();
    c.check(banks.ready(), "wavetable banks built");
    c.check(
        WavetableBanks::bank_name(0) == "ANALOG MORPH",
        "bank 0 name",
    );
    c.check(WavetableBanks::bank_name(7) == "SUB ZERO", "bank 7 name");

    for bank in 0..8 {
        for mip in 0..7 {
            let mut peak = 0.0f32;
            let mut prev = banks.sample(bank, 0.5, mip, 0.0);
            let mut discontinuities = 0;
            for i in 0..1024 {
                let phase = i as f32 / 1024.0;
                let s = banks.sample(bank, 0.5, mip, phase);
                c.check(s.is_finite(), "wavetable sample finite");
                peak = peak.max(s.abs());
                if (s - prev).abs() > 0.9 {
                    discontinuities += 1;
                }
                prev = s;
            }
            c.check(peak > 0.05, "wavetable bank has signal");
            c.check(peak <= 1.0, "wavetable bank stays in range");
            c.check(discontinuities <= 2, "wavetable bank is continuous");
        }
    }

    // Frame morph must change the spectrum, i.e. the tables are not copies.
    let a = banks.sample(0, 0.0, 3, 0.25);
    let b = banks.sample(0, 1.0, 3, 0.25);
    c.check((a - b).abs() > 1.0e-4, "frame morph changes the table");

    println!(
        "wavetable: {} checks, {} failures",
        c.checks.load(Ordering::Relaxed),
        c.failures.load(Ordering::Relaxed)
    );
    assert_eq!(c.failures.load(Ordering::Relaxed), 0);
}

#[test]
fn acid_voice_is_alive_and_bounded() {
    let mut voice = Acid303::new();
    voice.prepare(48_000.0);
    voice.set_cutoff(420.0);
    voice.set_resonance(0.72);
    voice.set_env_mod(0.68);
    voice.set_decay(0.55);
    voice.set_accent(0.82);
    voice.set_volume(0.9);
    voice.set_distortion(true, 0, 0.55, 0.62, 0.55);
    voice.note_on(36, 118);

    let mut peak = 0.0f32;
    let mut energy = 0.0f32;
    for i in 0..48_000 {
        let s = voice.process();
        assert!(s.is_finite(), "acid sample finite");
        peak = peak.max(s.abs());
        energy += s * s;
        if i == 24_000 {
            voice.note_off(36);
        }
    }
    assert!(peak > 0.05, "acid voice produces signal, peak={peak}");
    assert!(peak <= 1.5, "acid voice bounded, peak={peak}");
    assert!(energy > 0.5, "acid voice has energy");
    assert_eq!(voice.held_notes(), 0, "acid voice released");
}

#[test]
fn wave_synth_is_polyphonic() {
    let mut synth = WaveSynth::new();
    synth.prepare(48_000.0);
    synth.set_params(WaveParams {
        bank: 4,
        position: 0.25,
        unison: 3,
        cutoff: 1800.0,
        resonance: 0.7,
        drive: 1.2,
        chord: 2,
        ..WaveParams::default()
    });
    synth.note_on(48, 100);
    assert_eq!(synth.active_voices(), 3, "minor chord starts three voices");
    let mut peak = 0.0f32;
    for _ in 0..24_000 {
        let (mut l, mut r) = (0.0, 0.0);
        synth.render(&mut l, &mut r);
        assert!(l.is_finite() && r.is_finite());
        peak = peak.max(l.abs()).max(r.abs());
    }
    assert!(peak > 0.01, "wave synth audible, peak={peak}");
    synth.all_notes_off();
    for _ in 0..96_000 {
        let (mut l, mut r) = (0.0, 0.0);
        synth.render(&mut l, &mut r);
    }
    assert_eq!(synth.active_voices(), 0, "voices released");
}

#[test]
fn all_forty_eight_fx_slots_render() {
    let c = Checker {
        checks: AtomicUsize::new(0),
        failures: AtomicUsize::new(0),
    };
    c.check(fx_catalog().len() == FX_COUNT, "48 FX slots");
    for id in 0..FX_COUNT {
        let info = fx_info(id);
        c.check(!info.name.is_empty(), "fx has a name");
        c.check(info.code.len() == 3, "fx has a 3-character code");

        let mut fx = Effect::new();
        fx.prepare(48_000.0);
        fx.set_type(id);
        fx.set_all_params(&info.defaults);
        fx.set_enabled(true);
        let mut peak = 0.0f32;
        for i in 0..4_800 {
            let phase = i as f32 / 48_000.0 * 220.0;
            let input = (phase * std::f32::consts::TAU).sin() * 0.35;
            let (mut l, mut r) = (input, input * 0.8);
            fx.process(&mut l, &mut r, 126.0);
            c.check(l.is_finite() && r.is_finite(), "fx output finite");
            c.check(l.abs() <= 8.0 && r.abs() <= 8.0, "fx output bounded");
            peak = peak.max(l.abs()).max(r.abs());
        }
        c.check(peak > 0.0, "fx slot produces output");
    }
    println!(
        "fx: {} checks, {} failures",
        c.checks.load(Ordering::Relaxed),
        c.failures.load(Ordering::Relaxed)
    );
    assert_eq!(c.failures.load(Ordering::Relaxed), 0);
}

#[test]
fn clip_engine_launches_and_renders() {
    let c = Checker {
        checks: AtomicUsize::new(0),
        failures: AtomicUsize::new(0),
    };
    let (mut engine, handles) = create_engine(48_000.0, 512);
    let mut project = ProjectState::new();
    configure_acid(&handles, &mut project, 0, 36);
    configure_drums(&handles, &mut project, 1, true);
    configure_wave(&handles, &mut project, 2);

    handles.launch_scene(0);
    handles.set_playing(true);

    let peaks = render(&mut engine, 4.0, 256);
    c.check(peaks.finite, "engine output finite");
    c.check(peaks.peak > 0.02, "engine output audible");
    c.check(peaks.peak <= 1.01, "engine output bounded");

    let states: Vec<u8> = (0..3)
        .map(|pad| engine.pad_telemetry(pad).state as u8)
        .collect();
    c.check(states.iter().all(|s| *s == 2), "scene row is playing");

    // Stop everything: the master must fall silent within the tails.
    handles.stop_all();
    render(&mut engine, 2.0, 256);
    let stopped = render(&mut engine, 0.5, 256);
    c.check(stopped.peak < 0.02, "stop all silences the master");

    println!(
        "engine: {} checks, {} failures",
        c.checks.load(Ordering::Relaxed),
        c.failures.load(Ordering::Relaxed)
    );
    assert_eq!(c.failures.load(Ordering::Relaxed), 0);
}

#[test]
fn launch_quantization_queues_start_and_stop() {
    let (mut engine, handles) = create_engine(48_000.0, 512);
    let mut project = ProjectState::new();
    configure_acid(&handles, &mut project, 0, 36);
    handles.shared().set_quantize(16);

    handles.launch_pad(0);
    render(&mut engine, 0.05, 128);
    let first = engine.pad_telemetry(0);
    assert!(
        first.state == replicantsynth_core::ClipState::QueuedStart
            || first.state == replicantsynth_core::ClipState::Playing,
        "first launch is queued or playing, got {:?}",
        first.state
    );
    render(&mut engine, 1.2, 128);
    assert_eq!(
        engine.pad_telemetry(0).state,
        replicantsynth_core::ClipState::Playing,
        "clip starts on the next bar"
    );
    handles.launch_pad(0);
    render(&mut engine, 0.05, 128);
    assert_eq!(
        engine.pad_telemetry(0).state,
        replicantsynth_core::ClipState::QueuedStop,
        "second press queues the stop"
    );
    render(&mut engine, 2.4, 128);
    assert_eq!(
        engine.pad_telemetry(0).state,
        replicantsynth_core::ClipState::Stopped,
        "queued stop fires on the bar"
    );
}

#[test]
fn sample_pad_plays_published_buffer() {
    let (mut engine, handles) = create_engine(48_000.0, 512);
    let mut project = ProjectState::new();
    {
        let clip = &mut project.clips[0];
        clip.kind = ClipKind::Sample;
        clip.params[0] = 1.0;
        clip.sample_loop = false;
    }
    let clip = project.clips[0].clone();
    replicantsynth_core::sync_clip(&handles, &clip);

    // 0.5 s of 220 Hz sine, mono like an imported stem.
    let frames = 24_000;
    let mut data = Vec::with_capacity(frames);
    for i in 0..frames {
        let phase = i as f32 / 48_000.0 * 220.0;
        data.push((phase * std::f32::consts::TAU).sin() * 0.5);
    }
    handles.load_sample(
        0,
        Arc::new(SampleData {
            stereo: data,
            channels: 1,
            sample_rate: 48_000,
        }),
    );

    handles.launch_pad(0);
    handles.set_playing(true);
    let peaks = render(&mut engine, 0.4, 256);
    assert!(peaks.finite, "sample playback finite");
    assert!(
        peaks.peak > 0.1,
        "sample playback audible, peak={}",
        peaks.peak
    );
    assert!(peaks.peak <= 1.01, "sample playback bounded");
}

#[test]
fn capture_writes_a_24_bit_wav() {
    let (mut engine, handles) = create_engine(48_000.0, 512);
    let mut project = ProjectState::new();
    configure_drums(&handles, &mut project, 0, true);
    handles.launch_pad(0);
    handles.set_playing(true);

    let path = std::env::temp_dir().join("replicantsynth_capture_test.wav");
    let _ = std::fs::remove_file(&path);
    // Blocking capture: an offline renderer outruns any writer thread, and this
    // test is about the WAV the recorder produces, not about ring throughput.
    let writer =
        replicantsynth_core::RecordingWriter::start(&path, handles.master_tap(), 48_000, 2, true)
            .expect("capture starts");

    render(&mut engine, 1.5, 256);
    let writer = writer.stop();
    assert!(
        writer.frames() >= 48_000 * 3 / 2 - 1024,
        "capture kept the whole take: {} frames",
        writer.frames()
    );
    assert_eq!(writer.dropped(), 0, "blocking capture drops nothing");

    let bytes = std::fs::read(&path).expect("capture file exists");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
    assert_eq!(bits, 24, "capture is 24-bit");
    let data_size = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
    assert!(data_size > 100_000, "capture data size {data_size}");
    assert_eq!(data_size + 44, bytes.len(), "wav header matches file size");
}

#[test]
fn transport_stop_is_silent_and_reprepare_works() {
    let (mut engine, handles) = create_engine(48_000.0, 512);
    let mut project = ProjectState::new();
    configure_drums(&handles, &mut project, 0, true);
    handles.launch_pad(0);
    handles.set_playing(true);
    render(&mut engine, 0.5, 256);

    handles.set_playing(false);
    render(&mut engine, 0.1, 256);
    let stopped = render(&mut engine, 0.25, 256);
    assert!(stopped.peak < 1.0e-6, "transport stop silences the master");

    handles.set_playing(true);
    handles.launch_pad(0);
    let peaks = render(&mut engine, 0.5, 256);
    assert!(peaks.peak > 0.0, "engine restarts after stop");
}

#[test]
fn peak_guard_holds_with_eight_drum_clips() {
    let (mut engine, handles) = create_engine(48_000.0, 512);
    let mut project = ProjectState::new();
    for pad in 0..8 {
        configure_drums(&handles, &mut project, pad * 8, true);
        handles.launch_pad(pad * 8);
    }
    handles.shared().set_master_gain(1.0);
    handles.set_playing(true);
    let peaks = render(&mut engine, 2.0, 256);
    assert!(peaks.finite, "eight drum clips finite");
    assert!(
        peaks.peak <= 1.01,
        "eight drum clips bounded: {}",
        peaks.peak
    );
}

#[test]
fn demo_project_and_presets_are_complete() {
    let project = demo_project(Some("/tmp/neon_stab.wav"));
    assert_eq!(project.bpm, 126.0);
    assert!(project.has_any_clip());
    let filled = project
        .clips
        .iter()
        .filter(|clip| clip.kind != ClipKind::Empty)
        .count();
    assert_eq!(filled, 18, "demo fills 18 pads");

    let acids = AcidPresets::all();
    assert_eq!(acids.len(), 512);
    for preset in acids.iter().take(64) {
        assert_eq!(preset.steps.len(), 16);
        assert!(preset.length >= 1 && preset.length <= 16);
        assert!(preset.params.iter().all(|v| v.is_finite()));
    }
    let drums = DrumPresets::all();
    assert_eq!(drums.len(), 8);

    let wave = replicantsynth_core::model::wave_presets();
    assert_eq!(wave.len(), 44);
    for preset in wave {
        assert_eq!(preset.params.len(), 32);
        assert!(!preset.notes.is_empty());
    }
}

#[test]
fn project_round_trips_through_json() {
    let mut project = demo_project(None);
    project.bpm = 133.0;
    project.clips[0].name = "ROUND TRIP".to_string();
    project.clips[3].fx_type = 17;
    project.clips[3].params[24] = 0.25;
    let text = replicantsynth_core::model::store::encode(&project);
    let decoded = replicantsynth_core::model::store::decode(&text).expect("decode");
    assert_eq!(decoded.bpm, 133.0);
    assert_eq!(decoded.clips[0].name, "ROUND TRIP");
    assert_eq!(decoded.clips[3].fx_type, 17);
    assert!((decoded.clips[3].params[24] - 0.25).abs() < 1.0e-6);
    assert_eq!(decoded.clips.len(), 64);
}

#[test]
fn demo_project_renders_through_the_engine() {
    let (mut engine, handles) = create_engine(48_000.0, 512);
    let project = demo_project(None);
    sync_project(&handles, &project);
    handles.launch_scene(0);
    handles.set_playing(true);
    let peaks = render(&mut engine, 3.0, 512);
    assert!(peaks.finite, "demo render finite");
    assert!(peaks.peak > 0.05, "demo render audible: {}", peaks.peak);
    assert!(peaks.peak <= 1.01, "demo render bounded: {}", peaks.peak);
}

/// A stopped pad with a hosted plugin does not run the plugin — until its editor
/// is open, which is the only thing that keeps a plugin's own GUI alive and its
/// parameter reports flowing.
#[test]
fn a_stopped_plugin_pad_runs_only_while_its_editor_is_open() {
    use replicantsynth_core::engine::{bridge_pair, PluginHostCommand};
    use replicantsynth_core::model::ClipKind;

    let (mut engine, handles) = create_engine(48_000.0, 512);
    let mut project = ProjectState::new();
    project.clips[0].kind = ClipKind::Plugin;
    sync_project(&handles, &project);

    let (bridge, blocks_in, blocks_out) = bridge_pair(1);
    drop(blocks_out);
    handles
        .plugin_sender()
        .send(PluginHostCommand::Install {
            pad: 0,
            bridge: Box::new(bridge),
        })
        .expect("install");

    // Stopped, no editor: the plugin is left alone, block by block.
    let picked_up = render(&mut engine, 0.25, 512);
    assert!(picked_up.peak.abs() < 1.0e-6, "a stopped pad stays silent");
    let queued = {
        let mut count = 0;
        while blocks_in.try_recv().is_ok() {
            count += 1;
        }
        count
    };
    assert_eq!(
        queued, 0,
        "a stopped pad with a closed editor sends nothing"
    );

    // Editor open: the plugin runs so that it can report what its GUI does...
    handles.set_plugin_awake(0, true);
    let awake = render(&mut engine, 0.25, 512);
    let mut blocks = 0;
    while let Ok(block) = blocks_in.try_recv() {
        assert_eq!(
            block.frames,
            replicantsynth_core::engine::PLUGIN_BLOCK_FRAMES
        );
        blocks += 1;
    }
    assert!(blocks > 0, "an awake plugin pad feeds its worker");
    // ...and stays silent, because the pad is still not playing.
    assert!(
        awake.peak.abs() < 1.0e-6,
        "an awake but stopped pad is not mixed in: {}",
        awake.peak
    );

    // Closing the editor puts it back to sleep.
    handles.set_plugin_awake(0, false);
    let _ = render(&mut engine, 0.25, 512);
    let mut late = 0;
    while blocks_in.try_recv().is_ok() {
        late += 1;
    }
    assert_eq!(late, 0, "a closed editor stops the idle processing again");
}
