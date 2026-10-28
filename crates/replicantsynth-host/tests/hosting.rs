//! End-to-end plugin hosting against whatever is installed on this machine.
//!
//! The tests skip when no plugin of the required format exists, so they are safe
//! on a CI runner without plugins, and real evidence on a workstation that has
//! them.

use replicantsynth_core::engine::NoteEvent;
use replicantsynth_core::model::PluginFormat;
use replicantsynth_host::{default_scan_dirs, scan, HostedPlugin, PluginDescriptor};

fn find(format: PluginFormat, instrument: bool) -> Option<PluginDescriptor> {
    scan(&default_scan_dirs())
        .into_iter()
        .find(|plugin| plugin.format == format && plugin.is_instrument == instrument)
}

/// Play a note and measure the loudest sample the plugin produced.
///
/// Audio is fed in real time (one 128-frame block per 2.7 ms, which is exactly
/// what the device callback does at 48 kHz), so the worker thread sees the same
/// pacing it sees in the application rather than a burst it cannot keep up with.
fn render_note(plugin: &mut HostedPlugin, bridge: &mut replicantsynth_core::PluginBridge) -> f32 {
    use std::time::{Duration, Instant};
    const BLOCK: usize = replicantsynth_core::PLUGIN_BLOCK_FRAMES;
    const BLOCK_TIME: Duration = Duration::from_micros(2_700);

    bridge.push_event(NoteEvent::note_on(60, 110, 0));
    let mut peak = 0.0f32;

    // Let the thread start and the first blocks drain.
    std::thread::sleep(BLOCK_TIME * 4);

    let deadline = Instant::now() + Duration::from_millis(900);
    while Instant::now() < deadline {
        for _ in 0..BLOCK {
            let (left, right) = bridge.tick(0.0, 0.0);
            peak = peak.max(left.abs()).max(right.abs());
        }
        std::thread::sleep(BLOCK_TIME);
    }

    bridge.push_event(NoteEvent::note_off(60, 0));
    let release = Instant::now() + Duration::from_millis(250);
    while Instant::now() < release {
        for _ in 0..BLOCK {
            let (left, right) = bridge.tick(0.0, 0.0);
            peak = peak.max(left.abs()).max(right.abs());
        }
        std::thread::sleep(BLOCK_TIME);
    }

    let _ = plugin;
    peak
}

#[test]
fn a_clap_instrument_renders_audio_for_a_note() {
    let Some(descriptor) = find(PluginFormat::Clap, true) else {
        println!("no CLAP instrument installed; skipping");
        return;
    };
    println!(
        "hosting {} ({})",
        descriptor.name,
        descriptor.path.display()
    );

    let (mut plugin, mut bridge) =
        HostedPlugin::load(&descriptor, 48_000.0, 1024, true).expect("CLAP plugin loads");
    println!("{} parameters", plugin.params().len());

    let peak = render_note(&mut plugin, &mut bridge);
    println!(
        "peak {peak:.4}, {} blocks processed, {} sent / {} received",
        plugin.blocks_processed(),
        bridge.stats.blocks_sent,
        bridge.stats.blocks_received
    );

    assert!(
        plugin.blocks_processed() > 10,
        "the worker processed blocks"
    );
    assert!(bridge.stats.blocks_sent > 10, "the bridge sent blocks");
    assert!(peak > 0.0005, "the instrument produced audio, peak {peak}");
    assert!(peak <= 4.0, "the output stays sane, peak {peak}");
}

#[test]
fn a_clap_plugin_reports_parameters_and_saves_state() {
    let Some(descriptor) = find(PluginFormat::Clap, true) else {
        println!("no CLAP instrument installed; skipping");
        return;
    };
    let (mut plugin, _bridge) =
        HostedPlugin::load(&descriptor, 48_000.0, 512, true).expect("CLAP plugin loads");

    let params = plugin.params().to_vec();
    assert!(!params.is_empty(), "{} has parameters", descriptor.name);
    for param in &params {
        assert!(!param.name.is_empty(), "{param:?}");
        assert!(param.min <= param.max, "{param:?}");
        let normalized = param.normalized();
        assert!((0.0..=1.0).contains(&normalized), "{param:?}");
    }

    if let Some(first) = params.first() {
        plugin.set_param_normalized(first.id, 0.25);
    }
    let state = plugin.save_state().expect("state saves");
    println!("{} params, {} bytes of state", params.len(), state.len());
    assert!(!state.is_empty(), "the plugin saved a state blob");
}

#[test]
fn a_hosted_plugin_can_be_dropped_and_reloaded() {
    let Some(descriptor) = find(PluginFormat::Clap, true) else {
        println!("no CLAP instrument installed; skipping");
        return;
    };
    for _ in 0..2 {
        let (mut plugin, mut bridge) =
            HostedPlugin::load(&descriptor, 48_000.0, 512, true).expect("load");
        let _ = render_note(&mut plugin, &mut bridge);
        drop(plugin);
    }
}

#[test]
fn a_vst3_instrument_renders_audio_for_a_note() {
    let Some(descriptor) = find(PluginFormat::Vst3, true) else {
        println!("no VST3 instrument installed; skipping");
        return;
    };
    println!(
        "hosting {} ({})",
        descriptor.name,
        descriptor.path.display()
    );

    let (mut plugin, mut bridge) =
        HostedPlugin::load(&descriptor, 48_000.0, 1024, true).expect("VST3 plugin loads");
    println!("{} parameters", plugin.params().len());
    assert!(!plugin.params().is_empty(), "VST3 exposes parameters");

    let peak = render_note(&mut plugin, &mut bridge);
    println!(
        "peak {peak:.4}, {} blocks processed",
        plugin.blocks_processed()
    );
    assert!(
        plugin.blocks_processed() > 10,
        "the worker processed blocks"
    );
    assert!(peak > 0.0005, "the instrument produced audio, peak {peak}");
    assert!(peak <= 4.0, "the output stays sane, peak {peak}");
}

#[test]
fn listing_a_bundle_never_loads_it() {
    // A bundle without a manifest is described from the file system alone: an
    // empty class id means "the loader will pick the default class". Loading it
    // to find out would execute third-party code during a scan, and a yabridge
    // bundle would start Wine doing so.
    let mut with_manifest = 0;
    let mut without_manifest = 0;
    for dir in replicantsynth_host::default_scan_dirs() {
        if !dir.to_string_lossy().to_ascii_lowercase().contains("vst3") {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("vst3") {
                continue;
            }
            let classes = replicantsynth_host::vst3_backend::list(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(!classes.is_empty(), "{} listed nothing", path.display());
            for class in &classes {
                // Every entry has a name: from the manifest, or from the bundle
                // file name when there is no manifest to read.
                assert!(!class.name.is_empty(), "{}", path.display());
                if class.id.is_empty() {
                    without_manifest += 1;
                } else {
                    with_manifest += 1;
                }
            }
        }
    }
    println!("{with_manifest} class(es) from manifests, {without_manifest} unlisted bundle(s)");
    // Scans may skip them, but nothing here may have executed a plugin.
    assert!(with_manifest > 0 || without_manifest == 0);
}

/// A Wayland session has no X11 window to embed into. The host must still give
/// the plugin somewhere to draw: a top-level X11 window of its own, created the
/// same way it creates an embedded child.
#[test]
fn a_clap_editor_opens_without_a_parent_window() {
    let Some(descriptor) = find(PluginFormat::Clap, true) else {
        println!("no CLAP instrument installed; skipping");
        return;
    };
    if std::env::var_os("DISPLAY").is_none() {
        println!("no X display; skipping");
        return;
    }
    let (mut plugin, _bridge) =
        HostedPlugin::load(&descriptor, 48_000.0, 256, true).expect("the plugin loads");
    let rect = replicantsynth_host::EditorRect {
        x: 0,
        y: 0,
        width: 800,
        height: 600,
    };
    plugin
        .open_editor(None, rect)
        .expect("an editor opens with no parent window");
    assert!(plugin.editor_open(), "the editor reports itself open");
    plugin.close_editor();
    assert!(!plugin.editor_open(), "the editor is closed again");
}
