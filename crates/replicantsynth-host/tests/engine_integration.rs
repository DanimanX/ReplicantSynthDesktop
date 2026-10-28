//! End-to-end: a hosted plugin inside the real clip engine.
//!
//! This is the claim that matters for the feature — that choosing a VST3 or CLAP
//! instrument on a pad makes the pad sound through the same mixer, meters and
//! capture path as the built-in engines.

use replicantsynth_core::engine::{create_engine, sync_clip, EngineHandles, PluginHostCommand};
use replicantsynth_core::model::{apply_kind_defaults, ClipKind, PluginFormat, ProjectState};
use replicantsynth_host::{default_scan_dirs, scan, HostedPlugin, PluginDescriptor};
use std::time::{Duration, Instant};

fn instrument(format: PluginFormat) -> Option<PluginDescriptor> {
    scan(&default_scan_dirs())
        .into_iter()
        .find(|plugin| plugin.format == format && plugin.is_instrument)
}

/// Render for `seconds`, returning the master peak and whether it stayed finite.
fn render(engine: &mut replicantsynth_core::ClipEngine, seconds: f64) -> (f32, bool) {
    let frames = (engine.sample_rate() as f64 * seconds) as usize;
    let mut buffer = vec![0.0f32; 512 * 2];
    let mut peak = 0.0f32;
    let mut finite = true;
    let mut done = 0;
    while done < frames {
        let n = 512.min(frames - done);
        engine.process(&mut buffer, n);
        for i in 0..n {
            let l = buffer[i * 2];
            let r = buffer[i * 2 + 1];
            if !l.is_finite() || !r.is_finite() {
                finite = false;
            }
            peak = peak.max(l.abs()).max(peak);
            peak = peak.max(r.abs());
        }
        done += n;
        // Keep pace with real time so the plugin worker can keep up, the way the
        // device callback would.
        std::thread::sleep(Duration::from_micros(500));
    }
    (peak, finite)
}

fn play_plugin_pad(format: PluginFormat) {
    let Some(descriptor) = instrument(format) else {
        println!("no {} instrument installed; skipping", format.label());
        return;
    };
    println!("{}: {}", format.label(), descriptor.name);

    let (mut engine, handles): (_, EngineHandles) = create_engine(48_000.0, 512);
    let (plugin, bridge) =
        HostedPlugin::load(&descriptor, 48_000.0, 512, true).expect("plugin loads");
    handles
        .plugin_sender()
        .send(PluginHostCommand::Install {
            pad: 0,
            bridge: Box::new(bridge),
        })
        .expect("engine accepts the bridge");

    // A plugin pad with a four-note phrase in its piano roll.
    let mut project = ProjectState::new();
    {
        let clip = &mut project.clips[0];
        apply_kind_defaults(clip, ClipKind::Plugin);
        clip.midi_notes.clear();
        clip.midi_notes
            .push(replicantsynth_core::model::MidiNote::new(60, 0, 4, 110));
        clip.midi_notes
            .push(replicantsynth_core::model::MidiNote::new(64, 4, 4, 110));
        clip.gain = 0.9;
    }
    let clip = project.clips[0].clone();
    sync_clip(&handles, &clip);

    handles.launch_pad(0);
    handles.set_playing(true);

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut peak = 0.0f32;
    let mut finite = true;
    while Instant::now() < deadline {
        let (block_peak, block_finite) = render(&mut engine, 0.2);
        peak = peak.max(block_peak);
        finite &= block_finite;
    }

    println!(
        "peak {peak:.4} · {} plugin blocks · pad state {:?}",
        plugin.blocks_processed(),
        engine.pad_telemetry(0).state
    );

    assert!(finite, "the engine stayed finite");
    assert!(plugin.blocks_processed() > 0, "the plugin rendered blocks");
    assert!(
        peak > 0.0008,
        "the plugin's audio reached the master, peak {peak}"
    );
    assert!(peak <= 1.01, "the master stayed bounded, peak {peak}");

    // Stopping the transport must silence it again.
    handles.set_playing(false);
    let (silent_peak, _) = render(&mut engine, 0.5);
    assert!(
        silent_peak < 0.05,
        "stop released the plugin's notes, peak {silent_peak}"
    );
}

#[test]
fn a_clap_plugin_plays_through_the_engine() {
    play_plugin_pad(PluginFormat::Clap);
}

#[test]
fn a_vst3_plugin_plays_through_the_engine() {
    play_plugin_pad(PluginFormat::Vst3);
}
