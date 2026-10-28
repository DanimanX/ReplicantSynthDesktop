//! Desktop application state and layout.

use crate::audio::{list_output_devices, start_audio, AudioHost, AudioOptions};
use crate::decode::decode_file;
use crate::midi::{self, ApcPadMode, Arrow, IncomingMidi, MidiHub, MidiMessage, PortRole};
use crate::plugins::{descriptor_from_reference, PluginManager};
use crate::ui::editor;
use crate::ui::inspector;
use crate::ui::pad_grid::{self, Snapshot};
use crate::ui::sheets;
use crate::ui::theme;
use crate::ui::widgets;
use eframe::egui;
use egui::{Align, Layout, RichText, Vec2};
use replicantsynth_core::engine::{self, EngineHandles, RecordingWriter};
use replicantsynth_core::model::{
    self, apply_kind_defaults, demo_project, quantize_label, AcidPreset, ClipKind, DrumPreset,
    ProjectState, WavePreset,
};
use replicantsynth_core::ClipState;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorTab {
    Sound,
    Fx,
    Pattern,
    Mix,
}

impl InspectorTab {
    pub const ALL: [InspectorTab; 4] = [
        InspectorTab::Sound,
        InspectorTab::Fx,
        InspectorTab::Pattern,
        InspectorTab::Mix,
    ];

    pub fn label(self) -> &'static str {
        match self {
            InspectorTab::Sound => "SOUND",
            InspectorTab::Fx => "FX",
            InspectorTab::Pattern => "PATTERN",
            InspectorTab::Mix => "MIX",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PresetBank {
    #[default]
    Acid,
    Drums,
    Wave,
}

#[derive(Default)]
pub struct Windows {
    pub editor: bool,
    pub presets: bool,
    pub fx_picker: bool,
    pub midi: bool,
    pub recordings: bool,
    pub project: bool,
    pub settings: bool,
    pub diagnostics: bool,
    pub plugin_browser: bool,
    pub pad_menu: Option<usize>,
}

pub struct Take {
    pub path: PathBuf,
    pub name: String,
    pub frames: u64,
}

pub struct ActiveRecording {
    pub writer: Option<RecordingWriter>,
    pub target: Option<usize>,
    pub label: String,
    pub started: Instant,
}

pub struct App {
    pub project: ProjectState,
    pub selected: usize,
    pub tab: InspectorTab,
    pub windows: Windows,
    pub handles: EngineHandles,
    pub audio: AudioHost,
    pub midi: MidiHub,
    pub snapshot: Snapshot,
    pub status: String,
    pub status_at: Instant,
    pub project_path: PathBuf,
    pub recordings_dir: PathBuf,
    pub takes: Vec<Take>,
    pub recording: Option<ActiveRecording>,
    pub preset_filter: String,
    pub preset_bank: PresetBank,
    pub fx_filter: String,
    pub device_choice: Option<String>,
    pub device_rate: Option<u32>,
    pub apc_led_at: Instant,
    pub dirty: bool,
    pub last_save: Instant,
    pub last_led_key: String,
    pub plugins: PluginManager,
    /// Which pad's plugin editor is currently open, if any.
    pub plugin_editor: Option<usize>,
    pub plugin_filter: String,
    /// Set once the window handle is known, so the first embed can happen.
    pub window_parent: Option<u64>,
    plugins_restored: bool,
    screenshot: Option<ScreenshotPlan>,
}

/// Headless verification: render for a moment, save a PNG and exit.
pub struct ScreenshotPlan {
    pub path: PathBuf,
    pub frames: u32,
    pub requested: bool,
}

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("REPLICANTSYNTH_HOME") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("ReplicantSynth");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("ReplicantSynth");
        }
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("ReplicantSynth");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("ReplicantSynth");
    }
    PathBuf::from(".").join("ReplicantSynth")
}

fn bundled_sample() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("assets/demo/neon_stab.wav"),
        PathBuf::from("../assets/demo/neon_stab.wav"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn copy_bundled_sample(source: &std::path::Path, destination: &std::path::Path) -> Option<PathBuf> {
    if std::fs::create_dir_all(destination.parent()?).is_err() {
        return None;
    }
    match std::fs::copy(source, destination) {
        Ok(_) => Some(destination.to_path_buf()),
        Err(_) => None,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);

        let data = data_dir();
        let _ = std::fs::create_dir_all(&data);
        let recordings_dir = data.join("Recordings");
        let _ = std::fs::create_dir_all(&recordings_dir);
        let project_path = data.join("project.json");

        let (audio, handles) = start_audio(&AudioOptions::default());
        // No device: keep an engine-less handle so the UI still edits and saves.
        let handles = handles.unwrap_or_else(|| {
            EngineHandles::new(Arc::new(replicantsynth_core::EngineShared::new()))
        });

        let project = model::store::load(&project_path)
            .ok()
            .filter(|project| project.has_any_clip())
            .unwrap_or_else(|| {
                let sample = bundled_sample().and_then(|source| {
                    copy_bundled_sample(&source, &data.join("Samples/neon_stab.wav"))
                });
                demo_project(sample.as_ref().and_then(|p| p.to_str()))
            });

        let mut app = Self {
            project,
            selected: 0,
            tab: InspectorTab::Sound,
            windows: Windows::default(),
            handles,
            audio,
            midi: MidiHub::new(),
            snapshot: Snapshot::default(),
            status: "READY".to_string(),
            status_at: Instant::now(),
            project_path,
            recordings_dir,
            takes: Vec::new(),
            recording: None,
            preset_filter: String::new(),
            preset_bank: PresetBank::Acid,
            fx_filter: String::new(),
            device_choice: None,
            device_rate: None,
            apc_led_at: Instant::now(),
            dirty: false,
            last_save: Instant::now(),
            last_led_key: String::new(),
            plugins: PluginManager::new(),
            plugin_editor: None,
            plugin_filter: String::new(),
            window_parent: None,
            plugins_restored: false,
            screenshot: std::env::var("REPLICANTSYNTH_SCREENSHOT").ok().map(|path| {
                ScreenshotPlan {
                    path: PathBuf::from(path),
                    frames: std::env::var("REPLICANTSYNTH_SCREENSHOT_FRAMES")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(40),
                    requested: false,
                }
            }),
        };

        app.refresh_takes();
        app.sync_all();
        app.load_project_samples();
        app.snapshot = app.read_telemetry();

        // Headless verification can start a scene so screenshots show a running
        // transport with live meters.
        if let Ok(scene) = std::env::var("REPLICANTSYNTH_SCREENSHOT_SCENE") {
            if let Ok(row) = scene.parse::<usize>() {
                app.handles.launch_scene(row.min(7));
                app.handles.set_playing(true);
            }
        }
        if let Ok(pad) = std::env::var("REPLICANTSYNTH_SCREENSHOT_PAD") {
            if let Ok(index) = pad.parse::<usize>() {
                app.select(index.min(63));
            }
        }
        if let Ok(tab) = std::env::var("REPLICANTSYNTH_SCREENSHOT_TAB") {
            app.tab = match tab.to_lowercase().as_str() {
                "fx" => InspectorTab::Fx,
                "pattern" | "steps" => InspectorTab::Pattern,
                "mix" => InspectorTab::Mix,
                _ => InspectorTab::Sound,
            };
        }
        if std::env::var("REPLICANTSYNTH_SCREENSHOT_EDITOR").is_ok() {
            app.windows.editor = true;
        }
        if std::env::var("REPLICANTSYNTH_SCREENSHOT_REC").is_ok() {
            // Exercise the app's own capture wiring in headless runs.
            let target = std::env::var("REPLICANTSYNTH_SCREENSHOT_REC")
                .ok()
                .and_then(|value| value.parse::<usize>().ok());
            app.start_recording(target);
        }
        if std::env::var("REPLICANTSYNTH_SCREENSHOT_FX").is_ok() {
            app.windows.fx_picker = true;
        }
        if std::env::var("REPLICANTSYNTH_SCREENSHOT_PLUGINS").is_ok() {
            app.plugins.start_scan(false);
            app.windows.plugin_browser = true;
        }
        // Load a plugin onto the selected pad before the first frame, so a
        // screenshot shows its editor embedded in the window.
        if let Ok(query) = std::env::var("REPLICANTSYNTH_PLUGIN") {
            let needle = query.to_lowercase();
            let matches = |descriptor: &replicantsynth_host::PluginDescriptor| {
                descriptor.name.to_lowercase().contains(&needle)
                    || descriptor
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&needle)
            };
            let mut found = app
                .plugins
                .available
                .iter()
                .find(|descriptor| matches(descriptor))
                .cloned();
            if found.is_none() {
                app.plugins.start_scan(false);
                // The scan is asynchronous; block for it, since this is a
                // headless verification path, not the interactive one.
                for _ in 0..400 {
                    if app.plugins.poll_scan() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                found = app
                    .plugins
                    .available
                    .iter()
                    .find(|descriptor| matches(descriptor))
                    .cloned();
            }
            match found {
                Some(descriptor) => {
                    let pad = app.selected;
                    app.load_plugin(pad, &descriptor);
                    app.plugin_editor = Some(pad);
                }
                None => eprintln!("REPLICANTSYNTH_PLUGIN={query}: no matching plugin"),
            }
        }
        app
    }

    // -----------------------------------------------------------------------
    // Engine sync
    // -----------------------------------------------------------------------

    pub fn sync_pad(&mut self, pad: usize) {
        if let Some(clip) = self.project.clips.get(pad) {
            engine::sync_clip(&self.handles, clip);
        }
        self.dirty = true;
    }

    pub fn sync_all(&mut self) {
        engine::sync_project(&self.handles, &self.project);
        self.dirty = true;
    }

    pub fn params_only(&mut self, pad: usize) {
        if let Some(clip) = self.project.clips.get(pad) {
            self.handles.update_params(
                pad,
                &clip.params,
                clip.gain,
                clip.pan,
                clip.mute,
                clip.solo,
                clip.fx_enabled,
            );
        }
        self.dirty = true;
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_at = Instant::now();
    }

    pub fn select(&mut self, pad: usize) {
        self.selected = pad.min(63);
    }

    pub fn set_kind(&mut self, pad: usize, kind: ClipKind) {
        if kind == ClipKind::Plugin {
            // A plugin pad is defined by the plugin it hosts, so choosing the
            // engine means choosing the plugin.
            self.select(pad);
            self.plugins.start_scan(false);
            self.windows.plugin_browser = true;
            self.set_status("choose a VST3 or CLAP plugin for this pad");
            return;
        }
        self.plugins.unload(pad, &self.handles);
        if self.plugin_editor == Some(pad) {
            self.plugin_editor = None;
        }
        if let Some(clip) = self.project.clips.get_mut(pad) {
            apply_kind_defaults(clip, kind);
        }
        self.sync_pad(pad);
        self.set_status(format!("pad {:02} → {}", pad + 1, kind.label()));
    }

    /// Load a plugin onto a pad and make the pad a plugin clip.
    pub fn load_plugin(&mut self, pad: usize, descriptor: &replicantsynth_host::PluginDescriptor) {
        // A Wayland window cannot parent an X11 child, so ask for a detached
        // editor there instead of failing to embed.
        let prefer_floating = self.window_parent.is_none();

        match self.plugins.load(
            pad,
            descriptor,
            self.audio.sample_rate,
            1024,
            prefer_floating,
            &self.handles,
        ) {
            Ok(reference) => {
                if let Some(clip) = self.project.clips.get_mut(pad) {
                    apply_kind_defaults(clip, ClipKind::Plugin);
                    clip.name = descriptor.name.clone();
                    // Instruments get an empty piano roll ready to play; effects
                    // start bypassed-by-default only in the sense that they pass
                    // audio through, which a plugin does on its own.
                    clip.midi_notes.clear();
                    clip.plugin = Some(reference);
                }
                self.select(pad);
                self.sync_pad(pad);
                self.set_status(format!(
                    "{} {} loaded on pad {:02}",
                    descriptor.format.label(),
                    descriptor.name,
                    pad + 1
                ));
                self.plugin_editor = Some(pad);
                self.windows.plugin_browser = false;
                self.dirty = true;
            }
            Err(error) => self.set_status(format!("cannot load plugin: {error}")),
        }
    }

    /// Re-open the plugins a project references, restoring their saved state.
    pub fn restore_project_plugins(&mut self) {
        let pending: Vec<(usize, replicantsynth_core::model::PluginRef)> = self
            .project
            .clips
            .iter()
            .enumerate()
            .filter(|(_, clip)| clip.kind == ClipKind::Plugin)
            .filter_map(|(pad, clip)| clip.plugin.clone().map(|reference| (pad, reference)))
            .collect();

        for (pad, reference) in pending {
            let descriptor = descriptor_from_reference(&reference);
            if !std::path::Path::new(&reference.path).exists() {
                self.set_status(format!(
                    "pad {:02}: plugin missing at {}",
                    pad + 1,
                    reference.path
                ));
                continue;
            }
            let floating = self.window_parent.is_none();
            match self.plugins.load(
                pad,
                &descriptor,
                self.audio.sample_rate,
                1024,
                floating,
                &self.handles,
            ) {
                Ok(_) => {
                    if let Some(state) = reference.state.as_deref() {
                        if let Some(bytes) = crate::plugins::base64_decode(state) {
                            if let Some(loaded) = self.plugins.get_mut(pad) {
                                if let Err(error) = loaded.plugin.restore_state(&bytes) {
                                    eprintln!("pad {pad}: cannot restore plugin state: {error}");
                                }
                            }
                        }
                    }
                    // Keep the saved name and reference, but sync the engine so
                    // the pad plays.
                    if let Some(clip) = self.project.clips.get(pad).cloned() {
                        engine::sync_clip(&self.handles, &clip);
                    }
                }
                Err(error) => self.set_status(format!("pad {:02}: {error}", pad + 1)),
            }
        }
    }

    pub fn launch(&mut self, pad: usize) {
        self.handles.launch_pad(pad);
        if let Some(clip) = self.project.clip(pad) {
            if clip.kind != ClipKind::Empty && !self.transport() {
                self.handles.set_playing(true);
            }
        }
    }

    pub fn stop(&mut self, pad: usize) {
        self.handles.stop_pad(pad);
    }

    pub fn transport(&self) -> bool {
        self.handles.is_playing()
    }

    pub fn toggle_transport(&mut self) {
        let playing = self.handles.is_playing();
        self.handles.set_playing(!playing);
        self.set_status(if playing {
            "transport stop"
        } else {
            "transport play"
        });
    }

    pub fn apply_acid_preset(&mut self, preset: &AcidPreset) {
        let pad = self.selected;
        if let Some(clip) = self.project.clips.get_mut(pad) {
            clip.apply_acid_preset(preset);
        }
        self.sync_pad(pad);
        self.set_status(format!("preset: {}", preset.display_name()));
    }

    pub fn apply_drum_preset(&mut self, preset: &DrumPreset) {
        let pad = self.selected;
        if let Some(clip) = self.project.clips.get_mut(pad) {
            clip.apply_drum_preset(preset);
        }
        self.sync_pad(pad);
        self.set_status(format!("preset: {}", preset.name));
    }

    pub fn apply_wave_preset(&mut self, preset: &WavePreset) {
        let pad = self.selected;
        if let Some(clip) = self.project.clips.get_mut(pad) {
            clip.apply_wave_preset(preset);
        }
        self.sync_pad(pad);
        self.set_status(format!("preset: {}", preset.name));
    }

    pub fn set_fx(&mut self, pad: usize, fx_type: usize) {
        if let Some(clip) = self.project.clips.get_mut(pad) {
            clip.apply_fx_defaults(fx_type);
        }
        self.sync_pad(pad);
    }

    // -----------------------------------------------------------------------
    // Samples and takes
    // -----------------------------------------------------------------------

    pub fn import_sample(&mut self, pad: usize) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Audio",
                &["wav", "flac", "mp3", "ogg", "m4a", "aac", "aif", "aiff"],
            )
            .set_title("Import a sample into the selected pad")
            .pick_file()
        else {
            return;
        };
        self.load_sample_file(pad, &path);
    }

    pub fn load_sample_file(&mut self, pad: usize, path: &std::path::Path) {
        match decode_file(path) {
            Ok(sample) => {
                let frames = sample.frames();
                let channels = sample.channels;
                let seconds = frames as f32 / sample.sample_rate.max(1) as f32;
                self.handles.load_sample(pad, Arc::new(sample));
                if let Some(clip) = self.project.clips.get_mut(pad) {
                    clip.kind = ClipKind::Sample;
                    clip.sample_path = Some(path.display().to_string());
                    clip.sample_name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string());
                    clip.sample_missing = false;
                    clip.sample_loop = true;
                    if clip.name.trim().is_empty() {
                        clip.name = format!("SMP {:02}", pad + 1);
                    }
                }
                self.sync_pad(pad);
                self.set_status(format!(
                    "sample: {} ({:.1}s, {} ch, {} Hz)",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    seconds,
                    channels,
                    self.audio.sample_rate
                ));
            }
            Err(err) => self.set_status(format!("sample import failed: {err}")),
        }
    }

    pub fn load_project_samples(&mut self) {
        for pad in 0..64 {
            let Some(clip) = self.project.clips.get(pad) else {
                continue;
            };
            if clip.kind != ClipKind::Sample {
                continue;
            }
            let Some(path) = clip.sample_path.clone() else {
                continue;
            };
            match decode_file(std::path::Path::new(&path)) {
                Ok(sample) => {
                    self.handles.load_sample(pad, Arc::new(sample));
                }
                Err(err) => {
                    self.set_status(format!("missing sample for pad {:02}: {err}", pad + 1));
                    if let Some(clip) = self.project.clips.get_mut(pad) {
                        clip.sample_missing = true;
                    }
                }
            }
        }
    }

    pub fn refresh_takes(&mut self) {
        let mut takes = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.recordings_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("wav") {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let frames = std::fs::metadata(&path)
                    .map(|meta| meta.len().saturating_sub(44) / 6)
                    .unwrap_or(0);
                takes.push(Take { path, name, frames });
            }
        }
        takes.sort_by(|a, b| b.name.cmp(&a.name));
        self.takes = takes;
    }

    pub fn start_recording(&mut self, target: Option<usize>) {
        if self.recording.is_some() {
            self.stop_recording();
            return;
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let label = match target {
            Some(pad) => format!("stem_{:02}_{stamp}.wav", pad + 1),
            None => format!("master_{stamp}.wav"),
        };
        let path = self.recordings_dir.join(&label);
        let tap = if target.is_some() {
            self.handles.pad_tap()
        } else {
            self.handles.master_tap()
        };
        self.handles.set_pad_record_target(target);
        // Live capture: never block the audio callback, drop instead.
        match RecordingWriter::start(&path, tap, self.audio.sample_rate, 2, false) {
            Ok(writer) => {
                self.set_status(format!(
                    "capturing {} → {}",
                    if target.is_some() { "stem" } else { "master" },
                    label
                ));
                self.recording = Some(ActiveRecording {
                    writer: Some(writer),
                    target,
                    label,
                    started: Instant::now(),
                });
            }
            Err(err) => self.set_status(format!("capture failed: {err}")),
        }
    }

    pub fn stop_recording(&mut self) {
        let Some(mut active) = self.recording.take() else {
            return;
        };
        if let Some(writer) = active.writer.take() {
            let writer = writer.stop();
            let frames = writer.frames();
            let dropped = writer.dropped();
            self.set_status(format!(
                "captured {} ({} frames, {} dropped)",
                active.label, frames, dropped
            ));
        }
        self.handles.set_pad_record_target(None);
        self.refresh_takes();
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    // -----------------------------------------------------------------------
    // Project I/O
    // -----------------------------------------------------------------------

    pub fn save_project(&mut self, save_as: bool) {
        if save_as {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ReplicantSynth project", &["json"])
                .set_file_name("project.json")
                .save_file()
            {
                self.project_path = path;
            } else {
                return;
            }
        }
        self.plugins.capture_states(&mut self.project.clips);
        match model::store::save(&self.project, &self.project_path) {
            Ok(()) => {
                self.dirty = false;
                self.last_save = Instant::now();
                self.set_status(format!("saved {}", self.project_path.display()));
            }
            Err(err) => self.set_status(format!("save failed: {err}")),
        }
    }

    pub fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("ReplicantSynth project", &["json"])
            .pick_file()
        else {
            return;
        };
        match model::store::load(&path) {
            Ok(project) => {
                self.plugins.shut_all(&self.handles);
                self.plugin_editor = None;
                self.project = project;
                self.project_path = path;
                self.sync_all();
                self.load_project_samples();
                self.restore_project_plugins();
                self.set_status(format!("loaded {}", self.project_path.display()));
            }
            Err(err) => self.set_status(format!("load failed: {err}")),
        }
    }

    pub fn new_project(&mut self) {
        self.plugins.shut_all(&self.handles);
        self.plugin_editor = None;
        self.project = ProjectState::new();
        self.sync_all();
        self.set_status("new project");
    }

    pub fn load_demo(&mut self) {
        self.plugins.shut_all(&self.handles);
        self.plugin_editor = None;
        let sample = bundled_sample().and_then(|source| {
            copy_bundled_sample(&source, &data_dir().join("Samples/neon_stab.wav"))
        });
        self.project = demo_project(sample.as_ref().and_then(|p| p.to_str()));
        self.sync_all();
        self.load_project_samples();
        self.set_status("NEON GRID demo loaded");
    }

    pub fn restart_audio(&mut self) {
        let options = AudioOptions {
            device_name: self.device_choice.clone(),
            sample_rate: self.device_rate,
        };
        let (host, handles) = start_audio(&options);
        self.audio = host;
        if let Some(handles) = handles {
            self.handles = handles;
            self.sync_all();
            self.load_project_samples();
        }
        let status = self.audio.status.clone();
        self.set_status(format!("audio: {status}"));
    }

    // -----------------------------------------------------------------------
    // Telemetry and MIDI
    // -----------------------------------------------------------------------

    pub fn read_telemetry(&self) -> Snapshot {
        let telemetry = self.handles.telemetry();
        let mut snapshot = Snapshot {
            master_meter: telemetry.master_meter(),
            playhead: telemetry.playhead(),
            absolute_step: telemetry.absolute_step(),
            ..Snapshot::default()
        };
        for pad in 0..64 {
            snapshot.pads[pad] = telemetry.pad(pad);
            snapshot.meters[pad] = telemetry.pad_meter(pad);
        }
        snapshot
    }

    fn handle_midi(&mut self) {
        let events = self.midi.poll();
        if events.is_empty() {
            return;
        }
        for event in events {
            self.midi.record_event(&event.message);
            self.apply_midi_event(event);
        }
    }

    fn apply_midi_event(&mut self, event: IncomingMidi) {
        let role = midi::port_role(&event.port_name, event.port_index);
        match event.message {
            MidiMessage::SysEx(bytes) => {
                if let Some(mode) = midi::pad_mode_from_sysex(&bytes) {
                    self.midi.apc.mode = mode;
                    self.set_status(format!("APC64 mode: {}", mode.label()));
                }
                if let Some(firmware) = midi::firmware_from_identity(&bytes) {
                    self.midi.apc.firmware = Some(firmware.clone());
                    self.set_status(format!("APC64 firmware {firmware}"));
                }
            }
            MidiMessage::NoteOn { note, velocity, .. } => {
                self.apc_note_on(role, note, velocity);
            }
            MidiMessage::NoteOff { note, velocity, .. } => {
                if role == PortRole::Notes {
                    self.handles
                        .preview_note(self.selected, false, note, velocity);
                }
            }
            MidiMessage::ControlChange {
                controller, value, ..
            } => {
                if controller == midi::CC_ENCODER {
                    let delta = if value < 64 { value } else { value - 128 };
                    let bpm = (self.project.bpm + delta as f32 * 0.5).clamp(30.0, 300.0);
                    self.project.bpm = bpm;
                    self.handles.shared().set_tempo(bpm);
                    self.set_status(format!("BPM {bpm:.1}"));
                }
            }
            MidiMessage::PitchBend { channel, value } => {
                if (channel as usize) < midi::STRIP_COUNT {
                    let normalized = (value + 8192) as f32 / 16383.0;
                    if channel == 0 {
                        self.project.master_gain = normalized * 1.5;
                        self.handles
                            .shared()
                            .set_master_gain(self.project.master_gain);
                    } else if channel == 7 {
                        if let Some(clip) = self.project.clips.get_mut(self.selected) {
                            clip.gain = normalized * 1.2;
                        }
                        self.params_only(self.selected);
                    }
                }
            }
            MidiMessage::Other => {}
        }
    }

    fn apc_note_on(&mut self, role: PortRole, note: i32, velocity: i32) {
        let mode = self.midi.apc.mode;
        match role {
            PortRole::Notes => {
                if mode == ApcPadMode::Step {
                    self.step_sequencer_note(note, velocity);
                } else {
                    self.handles
                        .preview_note(self.selected, true, note, velocity);
                }
            }
            _ => {
                if let Some(pad) =
                    midi::pad_for_note(note, self.midi.apc.base_note, self.midi.apc.flip_rows)
                {
                    if mode.host_owns_pads() || mode == ApcPadMode::Unknown {
                        self.select(pad);
                        self.launch(pad);
                    } else if mode.key_surface() {
                        self.handles
                            .preview_note(self.selected, true, note, velocity);
                    }
                    return;
                }
                if let Some(scene) = midi::scene_for_note(note) {
                    self.handles.launch_scene(scene);
                    return;
                }
                match note {
                    midi::NOTE_PLAY => self.handles.set_playing(true),
                    midi::NOTE_STOP => self.handles.set_playing(false),
                    midi::NOTE_RECORD => self.start_recording(None),
                    midi::NOTE_CLIP_STOP => self.handles.stop_all(),
                    midi::NOTE_MUTE => self.toggle_mute(self.selected),
                    midi::NOTE_SOLO => self.toggle_solo(self.selected),
                    midi::NOTE_ENCODER_BUTTON => {
                        self.tab = match self.tab {
                            InspectorTab::Sound => InspectorTab::Fx,
                            InspectorTab::Fx => InspectorTab::Pattern,
                            InspectorTab::Pattern => InspectorTab::Mix,
                            InspectorTab::Mix => InspectorTab::Sound,
                        };
                        self.set_status(format!("inspector: {}", self.tab.label()));
                    }
                    _ => {
                        if let Some(strip) = midi::strip_touch_for_note(note) {
                            self.midi.touched_strip = Some(strip);
                            self.set_status(format!("strip {} touched", strip + 1));
                        } else if let Some(arrow) = midi::arrow_for_note(note) {
                            self.move_selection(arrow);
                        }
                    }
                }
            }
        }
    }

    fn step_sequencer_note(&mut self, note: i32, velocity: i32) {
        let pad = self.selected;
        let Some(clip) = self.project.clips.get(pad) else {
            return;
        };
        match clip.kind {
            ClipKind::Acid => {
                let step = ((note - 36).rem_euclid(16)) as usize;
                let mut current = clip.steps[step];
                current.gate = !current.gate;
                current.velocity = velocity.clamp(1, 127) as u8;
                let clip = &mut self.project.clips[pad];
                clip.steps[step] = current;
            }
            ClipKind::Drums => {
                let voice = (note - 36).rem_euclid(11) as usize;
                let step = ((note - 36) / 11).rem_euclid(16) as usize;
                let on = !self.project.clips[pad].drum_on(voice, step);
                self.project.clips[pad].set_drum_step(voice, step, on);
                let velocity = velocity.clamp(1, 127) as u8;
                self.project.clips[pad].set_drum_velocity(voice, step, velocity as i32);
            }
            _ => {}
        }
        self.sync_pad(pad);
    }

    pub fn toggle_mute(&mut self, pad: usize) {
        if let Some(clip) = self.project.clips.get_mut(pad) {
            clip.mute = !clip.mute;
        }
        self.params_only(pad);
    }

    pub fn toggle_solo(&mut self, pad: usize) {
        if let Some(clip) = self.project.clips.get_mut(pad) {
            clip.solo = !clip.solo;
        }
        self.params_only(pad);
    }

    fn move_selection(&mut self, arrow: Arrow) {
        let row = self.selected / 8;
        let col = self.selected % 8;
        self.selected = match arrow {
            Arrow::Up => row.saturating_sub(1) * 8 + col,
            Arrow::Down => ((row + 1).min(7)) * 8 + col,
            Arrow::Left => row * 8 + col.saturating_sub(1),
            Arrow::Right => row * 8 + (col + 1).min(7),
        };
    }

    fn push_apc_feedback(&mut self) {
        if !self.midi.is_connected() {
            return;
        }
        if self.apc_led_at.elapsed() < Duration::from_millis(90) {
            return;
        }
        self.apc_led_at = Instant::now();
        let snapshot = self.snapshot.clone();
        let mut key = String::with_capacity(256);
        for pad in 0..64 {
            key.push_str(&format!(
                "{}{}{}",
                snapshot.pads[pad].state as i32,
                snapshot.pads[pad].kind.ordinal(),
                pad == self.selected
            ));
        }
        if key == self.last_led_key {
            return;
        }
        self.last_led_key = key;
        for pad in 0..64 {
            let state = snapshot.pads[pad].state as i32;
            let kind = snapshot.pads[pad].kind.ordinal();
            let (color, brightness) = midi::led_color_for(state, kind, pad == self.selected);
            self.midi.send_pad_led(pad, color, brightness);
        }
        // Scene rail lamps mirror the row state.
        for row in 0..8 {
            let active = (0..8).any(|col| snapshot.pads[row * 8 + col].state == ClipState::Playing);
            let note = midi::SCENE_FIRST + row as i32;
            let color = if active {
                midi::COLOR_GREEN
            } else {
                midi::COLOR_GREY
            };
            self.midi.send(&midi::led(note, color, midi::LED_FULL)).ok();
        }
        self.midi.send_oled(0, "REPLICANT");
        self.midi
            .send_oled(1, &format!("{} BPM", self.project.bpm as i32));
        self.midi
            .send_oled(2, quantize_label(self.project.quantize));
        self.midi
            .send_oled(3, &format!("PAD {:02}", self.selected + 1));
        let strip = (self.project.master_gain / 1.5).clamp(0.0, 1.0);
        self.midi.send_strip_led(0, strip, false);
    }

    fn autosave(&mut self) {
        if self.dirty
            && self.last_save.elapsed() > Duration::from_secs(15)
            && model::store::save(&self.project, &self.project_path).is_ok()
        {
            self.dirty = false;
            self.last_save = Instant::now();
        }
    }

    // -----------------------------------------------------------------------
    // Layout
    // -----------------------------------------------------------------------

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("transport")
            .exact_height(52.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .stroke(egui::Stroke::new(1.0_f32, theme::LINE)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new("REPLICANT")
                            .size(15.0)
                            .strong()
                            .color(theme::CYAN),
                    );
                    ui.label(
                        RichText::new("SYNTH")
                            .size(15.0)
                            .strong()
                            .color(theme::PINK),
                    );
                    ui.label(RichText::new("DESKTOP").size(9.0).color(theme::DIM));
                    ui.add_space(10.0);

                    let playing = self.transport();
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(if playing { "■" } else { "▶" }).size(15.0),
                            )
                            .min_size(Vec2::new(32.0, 24.0))
                            .fill(if playing {
                                theme::dim(theme::PINK, 0.35)
                            } else {
                                theme::dim(theme::CYAN, 0.22)
                            }),
                        )
                        .on_hover_text("Play / stop (Space)")
                        .clicked()
                    {
                        self.toggle_transport();
                    }
                    let recording = self.is_recording();
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("REC").size(11.0).color(if recording {
                                    theme::RED
                                } else {
                                    theme::TEXT
                                }),
                            )
                            .min_size(Vec2::new(38.0, 24.0))
                            .fill(if recording {
                                theme::dim(theme::RED, 0.30)
                            } else {
                                theme::PANEL_HI
                            }),
                        )
                        .on_hover_text("Capture the master (R)")
                        .clicked()
                    {
                        self.start_recording(None);
                    }
                    if widgets::chip(ui, "STEM", false)
                        .on_hover_text("Capture the selected pad only")
                        .clicked()
                    {
                        self.start_recording(Some(self.selected));
                    }
                    if widgets::chip(ui, "STOP ALL", false).clicked() {
                        self.handles.stop_all();
                    }

                    ui.add_space(10.0);
                    ui.label(RichText::new("BPM").size(9.5).color(theme::DIM));
                    let mut bpm = self.project.bpm;
                    if ui
                        .add(
                            egui::DragValue::new(&mut bpm)
                                .speed(0.2)
                                .range(30.0..=300.0)
                                .fixed_decimals(1),
                        )
                        .changed()
                    {
                        self.project.bpm = bpm;
                        self.handles.shared().set_tempo(bpm);
                        self.dirty = true;
                    }
                    ui.label(RichText::new("SWING").size(9.5).color(theme::DIM));
                    let mut swing = self.project.swing * 100.0;
                    if ui
                        .add(
                            egui::DragValue::new(&mut swing)
                                .speed(0.6)
                                .range(0.0..=66.0)
                                .suffix("%"),
                        )
                        .changed()
                    {
                        self.project.swing = swing / 100.0;
                        self.handles.shared().set_swing(self.project.swing);
                        self.dirty = true;
                    }
                    if widgets::chip(ui, quantize_label(self.project.quantize), true).clicked() {
                        self.project.next_quantize();
                        self.handles.shared().set_quantize(self.project.quantize);
                        self.dirty = true;
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("MASTER").size(9.5).color(theme::DIM));
                    let mut gain = self.project.master_gain;
                    if ui
                        .add(
                            egui::DragValue::new(&mut gain)
                                .speed(0.01)
                                .range(0.0..=1.5)
                                .fixed_decimals(2),
                        )
                        .changed()
                    {
                        self.project.master_gain = gain;
                        self.handles.shared().set_master_gain(gain);
                        self.dirty = true;
                    }
                    pad_grid::master_meter(ui, self.snapshot.master_meter);

                    // The right-hand group adapts to the window width: the most
                    // important controls are always present, the rest appear as
                    // space allows so a narrow window never overlaps.
                    let room = ui.available_width();
                    let wide = room > 360.0;
                    let very_wide = room > 520.0;
                    if room > 90.0 {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if very_wide && self.audio.is_running() {
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {:.1} kHz",
                                        self.audio.device_name,
                                        self.audio.sample_rate as f32 / 1000.0
                                    ))
                                    .size(9.0)
                                    .color(theme::DIM),
                                );
                            }
                            if wide && widgets::chip(ui, "SETTINGS", false).clicked() {
                                self.windows.settings = true;
                            }
                            if widgets::chip(ui, "MIDI", self.midi.is_connected()).clicked() {
                                self.windows.midi = true;
                            }
                            if wide && widgets::chip(ui, "PROJECT", false).clicked() {
                                self.windows.project = true;
                            }
                            if very_wide && widgets::chip(ui, "TAKES", false).clicked() {
                                self.refresh_takes();
                                self.windows.recordings = true;
                            }
                        });
                    }
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .stroke(egui::Stroke::new(1.0_f32, theme::LINE)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    widgets::status_dot(
                        ui,
                        self.audio.is_running(),
                        theme::GREEN,
                        if self.audio.is_running() {
                            "ENGINE"
                        } else {
                            "OFFLINE"
                        },
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "{:.1} kHz · {} ch · {}",
                            self.audio.sample_rate as f32 / 1000.0,
                            self.audio.channels,
                            self.audio.status
                        ))
                        .size(10.0)
                        .color(theme::DIM),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "STEP {:02} · {} · {}",
                            self.snapshot.playhead + 1,
                            if self.transport() { "PLAY" } else { "STOP" },
                            if self.midi.is_connected() {
                                self.midi.apc.mode.label()
                            } else {
                                "NO MIDI"
                            }
                        ))
                        .size(10.0)
                        .color(theme::DIM),
                    );
                    if self.is_recording() {
                        if let Some(recording) = &self.recording {
                            ui.separator();
                            let target = match recording.target {
                                Some(pad) => format!("STEM {:02}", pad + 1),
                                None => "MASTER".to_string(),
                            };
                            ui.label(
                                RichText::new(format!(
                                    "● REC {target} · {} · {:.1}s",
                                    recording.label,
                                    recording.started.elapsed().as_secs_f32()
                                ))
                                .size(10.0)
                                .color(theme::RED),
                            );
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.status_at.elapsed() < Duration::from_secs(12) {
                            ui.label(RichText::new(&self.status).size(10.0).color(theme::CYAN));
                        } else {
                            ui.label(
                                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                    .size(10.0)
                                    .color(theme::DIM),
                            );
                        }
                        if self.dirty {
                            ui.label(RichText::new("unsaved").size(10.0).color(theme::AMBER));
                        }
                    });
                });
            });
    }

    fn central(&mut self, ctx: &egui::Context) {
        // A plugin editor gets the whole window: these editors are often wider
        // than the grid column, and a clipped synth panel is useless.
        let editing = self
            .plugin_editor
            .filter(|pad| self.plugins.is_loaded(*pad));
        if editing.is_none() {
            self.plugin_editor = None;
        }
        if let Some(pad) = editing {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(theme::BG)
                        .inner_margin(egui::Margin {
                            left: 10,
                            right: 10,
                            top: 8,
                            bottom: 8,
                        }),
                )
                .show(ctx, |ui| {
                    self.plugin_editor_area(ui, pad);
                });
            return;
        }

        egui::SidePanel::right("inspector")
            .exact_width(378.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin {
                        left: 4,
                        right: 10,
                        top: 8,
                        bottom: 8,
                    }),
            )
            .show(ctx, |ui| {
                inspector::draw(ui, self);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 6,
                        top: 8,
                        bottom: 8,
                    }),
            )
            .show(ctx, |ui| {
                let response = pad_grid::draw(ui, &self.project, &self.snapshot, self.selected);
                if let Some(pad) = response.select {
                    self.select(pad);
                }
                if let Some(pad) = response.launch {
                    self.launch(pad);
                }
                if let Some(pad) = response.editor {
                    self.select(pad);
                    self.windows.editor = true;
                }
                if let Some(pad) = response.plugin_editor {
                    self.plugin_editor = Some(pad);
                }
                if let Some(pad) = response.context {
                    self.windows.pad_menu = Some(pad);
                }
                if let Some(row) = response.scene_launch {
                    self.handles.launch_scene(row);
                    if !self.transport() {
                        self.handles.set_playing(true);
                    }
                }
                if let Some(row) = response.scene_stop {
                    self.handles.stop_scene(row);
                }
            });
    }

    /// Header plus the region reserved for an embedded plugin editor.
    fn plugin_editor_area(&mut self, ui: &mut egui::Ui, pad: usize) {
        let name = self
            .project
            .clips
            .get(pad)
            .and_then(|clip| clip.plugin.as_ref())
            .map(|plugin| plugin.name.clone())
            .unwrap_or_else(|| "plugin".to_string());
        let accent = crate::plugins::plugin_accent();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("PLUGIN · PAD {:02}", pad + 1))
                    .size(9.5)
                    .color(theme::DIM),
            );
            ui.label(RichText::new(&name).size(14.0).strong().color(accent));
            if let Some(loaded) = self.plugins.get(pad) {
                if loaded.plugin.is_floating() {
                    ui.label(
                        RichText::new("editor is a separate window")
                            .size(9.5)
                            .color(theme::AMBER),
                    );
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::chip(ui, "CLOSE", true).clicked() {
                    self.plugins.close_editor(pad);
                    self.plugin_editor = None;
                }
                if widgets::chip(ui, "PIANO ROLL", false).clicked() {
                    self.plugins.close_editor(pad);
                    self.plugin_editor = None;
                    self.windows.editor = true;
                }
                if widgets::chip(ui, "RELOAD STATE", false).clicked() {
                    self.reload_plugin_state(pad);
                }
            });
        });
        ui.add_space(4.0);

        let area = ui.available_rect_before_wrap();
        if area.width() < 40.0 || area.height() < 40.0 {
            return;
        }
        let scale = ui.ctx().pixels_per_point();
        let available = (
            (area.width() * scale).round() as u32,
            (area.height() * scale).round() as u32,
        );

        // A plugin editor has a size it wants. Reserve exactly that, centred:
        // stretching a fixed-size editor leaves a dead black border instead.
        // Once the editor exists, prefer the size it actually took, because a
        // fixed-size plugin does not honour the size we asked for.
        let preferred = self.plugins.preferred_size(pad).unwrap_or((640, 480));
        let width = preferred.0.min(available.0).max(80);
        let height = preferred.1.min(available.1).max(60);
        let rect = replicantsynth_host::EditorRect {
            x: ((area.left() * scale) + (available.0.saturating_sub(width)) as f32 / 2.0).round()
                as i32,
            y: ((area.top() * scale) + (available.1.saturating_sub(height)) as f32 / 2.0).round()
                as i32,
            width,
            height,
        };

        // Paint the frame the child window sits in, so the editor looks placed
        // rather than floating in a void.
        let frame_rect = egui::Rect::from_min_size(
            egui::pos2(
                area.left() + (available.0.saturating_sub(width)) as f32 / 2.0 / scale,
                area.top() + (available.1.saturating_sub(height)) as f32 / 2.0 / scale,
            ),
            egui::vec2(width as f32 / scale, height as f32 / scale),
        );
        let painter = ui.painter();
        painter.rect_filled(
            frame_rect,
            egui::CornerRadius::same(6),
            egui::Color32::from_rgb(11, 14, 20),
        );
        painter.rect_stroke(
            frame_rect,
            egui::CornerRadius::same(6),
            egui::Stroke::new(1.0_f32, theme::LINE),
            egui::StrokeKind::Inside,
        );

        let Some(parent) = self.window_parent else {
            painter.text(
                frame_rect.center(),
                egui::Align2::CENTER_CENTER,
                "This session has no X11 window to embed into.\nUse OPEN EDITOR to detach it.",
                egui::FontId::proportional(12.0),
                theme::DIM,
            );
            return;
        };

        if let Some(loaded) = self.plugins.get(pad) {
            if loaded.plugin.editor_open() {
                // Track what the plugin actually occupies: a fixed-size editor
                // ignores the size we ask for, and the frame must match it.
                if let Some((actual_w, actual_h)) = self.plugins.preferred_size(pad) {
                    if actual_w > 0 && actual_h > 0 && (actual_w, actual_h) != (width, height) {
                        let rect = replicantsynth_host::EditorRect {
                            x: rect.x + ((width.saturating_sub(actual_w)) / 2) as i32,
                            y: rect.y + ((height.saturating_sub(actual_h)) / 2) as i32,
                            width: actual_w.min(available.0),
                            height: actual_h.min(available.1),
                        };
                        self.plugins.set_editor_rect(pad, rect);
                        return;
                    }
                }
                self.plugins.set_editor_rect(pad, rect);
                return;
            }
        }
        // First open asks for the whole area: CLAP only reports its real size
        // once the editor exists, and the tracking above then centres it.
        if !self.plugins.open_editor(pad, Some(parent), rect) {
            let message = self
                .plugins
                .get(pad)
                .and_then(|loaded| loaded.error.clone())
                .unwrap_or_else(|| "the plugin refused to open its editor".to_string());
            painter.text(
                frame_rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{message}\n\nUse PIANO ROLL to edit notes instead."),
                egui::FontId::proportional(12.0),
                theme::AMBER,
            );
        }
    }

    /// Re-apply the state saved in the project to the live plugin.
    pub fn reload_plugin_state(&mut self, pad: usize) {
        let state = self
            .project
            .clips
            .get(pad)
            .and_then(|clip| clip.plugin.as_ref())
            .and_then(|plugin| plugin.state.as_deref())
            .and_then(crate::plugins::base64_decode);
        let Some(bytes) = state else {
            self.set_status("no saved state for this plugin yet");
            return;
        };
        match self.plugins.get_mut(pad) {
            Some(loaded) => match loaded.plugin.restore_state(&bytes) {
                Ok(()) => self.set_status("plugin state restored"),
                Err(error) => self.set_status(format!("restore failed: {error}")),
            },
            None => self.set_status("plugin is not loaded"),
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        if self.windows.editor {
            editor::draw(ctx, self);
        }
        sheets::draw(ctx, self);
    }

    /// Find the native window so a plugin editor can be embedded as its child.
    ///
    /// Only X11 can host an embedded editor here: a Wayland surface has no
    /// window id to parent, so in that case we leave this unset and plugins open
    /// detached editors.
    fn resolve_window_parent(&mut self, frame: &eframe::Frame) {
        if self.window_parent.is_some() {
            return;
        }
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = frame.window_handle() else {
            return;
        };
        match handle.as_raw() {
            RawWindowHandle::Xlib(xlib) => self.window_parent = Some(xlib.window),
            RawWindowHandle::Xcb(xcb) => self.window_parent = Some(u64::from(xcb.window.get())),
            _ => {}
        }
    }

    /// Re-attach the plugins the project references, once the window is known so
    /// they can be created with the right editor mode.
    fn restore_plugins_once(&mut self) {
        if self.plugins_restored {
            return;
        }
        let has_plugin_clips = self
            .project
            .clips
            .iter()
            .any(|clip| clip.kind == ClipKind::Plugin);
        // Wait one frame for the handle to be resolved; by then a Wayland
        // session has also had its chance to report a non-X11 handle.
        if has_plugin_clips {
            self.restore_project_plugins();
        }
        self.plugins_restored = true;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|memory| memory.focused().is_some());
        if typing {
            return;
        }
        let (space, record, save, open, new, left, right, up, down) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::R),
                i.modifiers.command && i.key_pressed(egui::Key::S),
                i.modifiers.command && i.key_pressed(egui::Key::O),
                i.modifiers.command && i.key_pressed(egui::Key::N),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
            )
        });
        if space {
            self.toggle_transport();
        }
        if record {
            self.start_recording(None);
        }
        if save {
            self.save_project(false);
        }
        if open {
            self.open_project();
        }
        if new {
            self.new_project();
        }
        if left {
            self.move_selection(Arrow::Left);
        }
        if right {
            self.move_selection(Arrow::Right);
        }
        if up {
            self.move_selection(Arrow::Up);
        }
        if down {
            self.move_selection(Arrow::Down);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
            self.handles.clear_clip(self.selected);
            self.set_kind(self.selected, ClipKind::Empty);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.resolve_window_parent(frame);
        self.restore_plugins_once();

        self.snapshot = self.read_telemetry();
        self.audio.poll_error();
        self.handle_midi();
        self.push_apc_feedback();
        self.plugins.poll(&self.handles);
        if self.plugins.poll_scan() {
            let count = self.plugins.available.len();
            self.set_status(format!("{count} plugin(s) found"));
        }
        self.autosave();

        if self.audio.error.is_some() {
            let message = self.audio.error.clone().unwrap_or_default();
            self.audio.error = None;
            self.set_status(format!("audio: {message}"));
        }

        self.shortcuts(ctx);
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.central(ctx);
        self.windows(ctx);

        if let Some(plan) = &mut self.screenshot {
            plan.frames = plan.frames.saturating_sub(1);
            if plan.frames == 0 && !plan.requested {
                plan.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            if plan.requested {
                let shot = ctx.input(|i| {
                    i.events.iter().find_map(|event| match event {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(image) = shot {
                    let rgba: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|p| [p.r(), p.g(), p.b(), p.a()])
                        .collect();
                    if let Some(buffer) =
                        image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, rgba)
                    {
                        if let Some(parent) = plan.path.parent() {
                            if !parent.as_os_str().is_empty() {
                                if let Err(err) = std::fs::create_dir_all(parent) {
                                    eprintln!(
                                        "screenshot directory {} failed: {err}",
                                        parent.display()
                                    );
                                }
                            }
                        }
                        match buffer.save(&plan.path) {
                            Ok(()) => println!("screenshot: {}", plan.path.display()),
                            Err(err) => {
                                eprintln!("screenshot {} failed: {err}", plan.path.display())
                            }
                        }
                    }
                    // Headless runs can also prove the save path, which is how
                    // CI checks that plugin state survives a project file.
                    if std::env::var("REPLICANTSYNTH_SCREENSHOT_SAVE").is_ok() {
                        self.save_project(false);
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint();
            return;
        }

        if self.transport() || self.is_recording() {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }
}

/// Enumerate devices for the settings sheet.
pub fn output_devices() -> Vec<String> {
    list_output_devices()
}
