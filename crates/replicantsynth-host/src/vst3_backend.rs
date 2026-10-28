//! The VST3 backend: discovery from the bundle manifest, hosting through
//! `vst3-host`, and the plugin's own editor as a child X11 window.
//!
//! VST3 parameters are normalized 0..1 in this API, which matches the value
//! space the inspector edits, so no scaling is needed between them.

use crate::gui::EditorRect as HostRect;
use crate::worker::PluginProcessor;
use crate::{EditorEvent, PluginError, PluginParam};
use replicantsynth_core::model::PluginFormat;
use std::path::Path;
use std::sync::{Arc, Mutex};
use vst3_host::audio::AudioBuffers;
use vst3_host::midi::{MidiChannel, MidiEvent};
use vst3_host::plugin::Plugin;

/// Enumerate the audio classes in one `.vst3` bundle.
///
/// This reads `Contents/Resources/moduleinfo.json`, which every VST3 3.7+ bundle
/// ships.
///
/// Scanning deliberately never loads the plugin: a bundle without a manifest is
/// reported as one unnamed entry rather than opened. That matters in practice —
/// a yabridge bundle is a Windows DLL, and asking the native loader about it
/// drags in a whole Wine process that can abort the scanner. Listing plugins
/// must not execute them.
pub fn list(path: &Path) -> Result<Vec<crate::PluginDescriptor>, String> {
    let manifest = path.join("Contents/Resources/moduleinfo.json");
    if let Ok(text) = std::fs::read_to_string(&manifest) {
        match parse_manifest(&text, path) {
            Ok(classes) if !classes.is_empty() => return Ok(classes),
            Ok(_) => {}
            Err(error) => {
                return Err(format!("cannot parse {}: {error}", manifest.display()));
            }
        }
    }
    Ok(vec![unlisted_bundle(path)])
}

/// A bundle we cannot describe without loading it. Shown so the user can still
/// pick it, with the name taken from the file system.
fn unlisted_bundle(path: &Path) -> crate::PluginDescriptor {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "VST3 plugin".to_string());
    crate::PluginDescriptor {
        format: PluginFormat::Vst3,
        path: path.to_path_buf(),
        // An empty class id tells the loader to use the bundle's default class.
        id: String::new(),
        name,
        vendor: String::new(),
        category: "Plugin".to_string(),
        is_instrument: false,
    }
}

/// Whether the bundle ships a library for this platform.
///
/// This is advisory only: a yabridge bundle also ships a native `.so`, but it is
/// a shim that starts Wine, so presence of a library does not prove the plugin
/// runs natively. That is why scanning uses the manifest rather than this.
pub fn has_native_library(path: &Path) -> bool {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64-linux"
    } else if cfg!(target_os = "macos") {
        "MacOS"
    } else if cfg!(target_os = "windows") {
        "x86_64-win"
    } else {
        "x86_64-linux"
    };
    let dir = path.join("Contents").join(arch);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let extension = entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        matches!(extension.as_str(), "so" | "dylib" | "dll" | "vst3")
    })
}

fn parse_manifest(text: &str, path: &Path) -> Result<Vec<crate::PluginDescriptor>, String> {
    // VST3 writes a relaxed JSON dialect that allows trailing commas.
    let cleaned = strip_trailing_commas(text);
    let value: serde_json::Value =
        serde_json::from_str(&cleaned).map_err(|error| error.to_string())?;
    let factory_vendor = value
        .get("Factory Info")
        .and_then(|info| info.get("Vendor"))
        .and_then(|vendor| vendor.as_str())
        .unwrap_or("")
        .to_string();

    let mut out = Vec::new();
    let Some(classes) = value.get("Classes").and_then(|classes| classes.as_array()) else {
        return Ok(out);
    };
    for class in classes {
        let category = class
            .get("Category")
            .and_then(|category| category.as_str())
            .unwrap_or("");
        if category != "Audio Module Class" {
            continue;
        }
        let Some(id) = class.get("CID").and_then(|id| id.as_str()) else {
            continue;
        };
        let name = class
            .get("Name")
            .and_then(|name| name.as_str())
            .unwrap_or_default()
            .to_string();
        if name.trim().is_empty() {
            continue;
        }
        let sub_categories: Vec<String> = class
            .get("Sub Categories")
            .and_then(|list| list.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|entry| entry.as_str())
                    .map(|entry| entry.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let is_instrument = sub_categories.iter().any(|entry| entry == "instrument");
        let first_sub_category = sub_categories.first().cloned().unwrap_or_default();

        out.push(crate::PluginDescriptor {
            format: PluginFormat::Vst3,
            path: path.to_path_buf(),
            id: id.to_ascii_uppercase(),
            name,
            vendor: class
                .get("Vendor")
                .and_then(|vendor| vendor.as_str())
                .map(|vendor| vendor.to_string())
                .filter(|vendor| !vendor.trim().is_empty())
                .unwrap_or_else(|| factory_vendor.clone()),
            category: if is_instrument {
                "Instrument".to_string()
            } else if first_sub_category.is_empty() {
                "Effect".to_string()
            } else {
                format!("Effect · {first_sub_category}")
            },
            is_instrument,
        });
    }
    Ok(out)
}

/// VST3's JSON writer emits trailing commas; serde_json does not accept them.
fn strip_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            out.push(byte as char);
            if byte == b'\\' {
                index += 1;
                if index < bytes.len() {
                    out.push(bytes[index] as char);
                }
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            out.push('"');
            index += 1;
            continue;
        }
        if byte == b',' {
            let mut lookahead = index + 1;
            while lookahead < bytes.len() && (bytes[lookahead] as char).is_ascii_whitespace() {
                lookahead += 1;
            }
            if lookahead < bytes.len() && (bytes[lookahead] == b'}' || bytes[lookahead] == b']') {
                index += 1;
                continue;
            }
        }
        out.push(byte as char);
        index += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

/// The VST3 audio processor, owned by the plugin worker thread.
pub struct Vst3Processor {
    plugin: Arc<Mutex<Plugin>>,
    buffers: AudioBuffers,
    pending: Vec<(u32, f32)>,
    output_channels: usize,
    frames: usize,
}

impl Vst3Processor {
    fn new(plugin: Arc<Mutex<Plugin>>, frames: usize) -> Self {
        Self {
            plugin,
            buffers: AudioBuffers::new(2, 2, frames, 48_000.0),
            pending: Vec::new(),
            output_channels: 2,
            frames,
        }
    }
}

impl PluginProcessor for Vst3Processor {
    fn set_param(&mut self, id: u32, value: f32) {
        self.pending.push((id, value));
    }

    fn process(&mut self, block: &mut replicantsynth_core::engine::PluginBlock) {
        let frames = block.frames.min(self.frames);
        if frames == 0 {
            return;
        }
        // `try_lock` on purpose: the editor may hold the plugin while it waits
        // for the UI thread, and blocking here would risk a deadlock.
        let Ok(mut plugin) = self.plugin.try_lock() else {
            block.left[..frames].fill(0.0);
            block.right[..frames].fill(0.0);
            return;
        };

        for (id, value) in self.pending.drain(..) {
            let _ = plugin.set_parameter(id, value.clamp(0.0, 1.0) as f64);
        }
        for event in block.events[..block.event_count].iter() {
            let message = if event.on {
                MidiEvent::NoteOn {
                    channel: MidiChannel::Ch1,
                    note: event.note & 0x7F,
                    velocity: event.velocity & 0x7F,
                }
            } else {
                MidiEvent::NoteOff {
                    channel: MidiChannel::Ch1,
                    note: event.note & 0x7F,
                    velocity: 0,
                }
            };
            let _ = plugin.send_live_midi_event(message);
        }

        self.buffers.block_size = frames;
        for channel in 0..2 {
            let source = if channel == 0 {
                &block.left
            } else {
                &block.right
            };
            let destination = &mut self.buffers.inputs[channel];
            destination[..frames].copy_from_slice(&source[..frames]);
            destination[frames..].fill(0.0);
        }

        if plugin.process_audio(&mut self.buffers).is_err() {
            block.left[..frames].fill(0.0);
            block.right[..frames].fill(0.0);
            return;
        }

        block.left[..frames].copy_from_slice(&self.buffers.outputs[0][..frames]);
        let right = if self.output_channels > 1 { 1 } else { 0 };
        block.right[..frames].copy_from_slice(&self.buffers.outputs[right][..frames]);
    }

    fn deactivate(&mut self) {
        if let Ok(mut plugin) = self.plugin.try_lock() {
            let _ = plugin.stop_processing();
        }
    }
}

// ---------------------------------------------------------------------------
// Main-thread runtime
// ---------------------------------------------------------------------------

/// The UI-thread half of a loaded VST3 plugin.
pub struct Vst3Runtime {
    plugin: Arc<Mutex<Plugin>>,
    params: Vec<PluginParam>,
    editor_window: Option<crate::gui::EditorWindow>,
    float_window: Option<vst3_host::window::PluginWindow>,
    editor_open: bool,
    prefer_floating: bool,
}

impl Vst3Runtime {
    pub fn load(
        path: &Path,
        class_id: &str,
        sample_rate: f64,
        max_block: usize,
        prefer_floating: bool,
    ) -> Result<(Self, Box<dyn PluginProcessor>), PluginError> {
        let mut host = vst3_host::Vst3Host::builder()
            .sample_rate(sample_rate)
            .block_size(max_block.max(64))
            .build()
            .map_err(|error| PluginError::Load(error.to_string()))?;

        let mut plugin = if class_id.is_empty() {
            host.load_plugin(path)
                .map_err(|error| PluginError::Load(error.to_string()))?
        } else {
            host.load_plugin_class(path, class_id)
                .map_err(|error| PluginError::Load(error.to_string()))?
        };

        plugin
            .start_processing()
            .map_err(|error| PluginError::Audio(error.to_string()))?;

        let params = plugin
            .get_parameters()
            .map_err(|error| PluginError::Instantiate(error.to_string()))?
            .into_iter()
            .map(|parameter| PluginParam {
                id: parameter.id,
                name: parameter.name,
                group: parameter.unit,
                min: 0.0,
                max: 1.0,
                default: parameter.default,
                stepped: parameter.step_count > 0,
                value: parameter.value,
            })
            .collect();

        let shared = Arc::new(Mutex::new(plugin));
        let processor = Vst3Processor::new(shared.clone(), max_block.max(64));
        Ok((
            Self {
                plugin: shared,
                params,
                editor_window: None,
                float_window: None,
                editor_open: false,
                prefer_floating,
            },
            Box::new(processor),
        ))
    }

    pub fn params(&self) -> &[PluginParam] {
        &self.params
    }

    pub fn open_editor(
        &mut self,
        parent: Option<u64>,
        rect: HostRect,
        _title: &str,
    ) -> Result<(), PluginError> {
        if self.editor_open {
            return Ok(());
        }

        #[cfg(target_os = "linux")]
        if let Some(parent) = parent.filter(|_| !self.prefer_floating) {
            // The child window is ours, not the helper's: it must inherit the
            // parent's visual and depth, or the X server rejects the create
            // with BadMatch when the app's GL surface is 32-bit and the root
            // visual is not.
            let window = crate::gui::EditorWindow::open(
                parent,
                "plugin editor",
                rect.width.max(160),
                rect.height.max(120),
                rect.x,
                rect.y,
            )?;
            let handle = vst3_host::plugin::WindowHandle::from_x11(window.handle());
            let result = self
                .plugin
                .lock()
                .map_err(|_| PluginError::Embed("plugin lock poisoned".to_string()))
                .and_then(|mut plugin| {
                    plugin
                        .open_editor(handle)
                        .map_err(|error| PluginError::Embed(error.to_string()))
                });
            match result {
                Ok(()) => {
                    self.editor_window = Some(window);
                    self.editor_open = true;
                    // VSTGUI editors only paint once the host drives the run loop.
                    if let Ok(mut plugin) = self.plugin.lock() {
                        plugin.service_run_loop();
                    }
                    return Ok(());
                }
                Err(error) => {
                    drop(window);
                    eprintln!("vst3: the plugin refused the editor ({error})");
                    return Err(error);
                }
            }
        }

        // Fallback: let the plugin own a top-level window.
        let mut window = vst3_host::window::PluginWindow::new(self.plugin.clone());
        window
            .open()
            .map_err(|error| PluginError::Embed(error.to_string()))?;
        self.float_window = Some(window);
        self.editor_open = true;
        Ok(())
    }

    /// Keep the embedded editor aligned with the region the host reserved.
    ///
    /// A detached editor (its own top-level window) is never moved or resized to
    /// the UI's coordinates; it only needs its run loop serviced.
    pub fn set_editor_rect(&mut self, rect: HostRect) {
        if let Some(window) = &self.editor_window {
            window.move_to(rect.x, rect.y);
            window.resize(rect.width.max(160), rect.height.max(120));
        }
        if let Ok(mut plugin) = self.plugin.try_lock() {
            if self.editor_window.is_some() {
                let _ =
                    plugin.resize_editor(rect.width.max(160) as i32, rect.height.max(120) as i32);
            }
            plugin.service_run_loop();
        }
    }

    /// Drive the VST3 run loop; VSTGUI editors need this every frame.
    pub fn service(&mut self) {
        if !self.editor_open {
            return;
        }
        if let Ok(mut plugin) = self.plugin.try_lock() {
            plugin.service_run_loop();
        }
    }

    pub fn close_editor(&mut self) {
        if self.editor_window.is_some() {
            if let Ok(mut plugin) = self.plugin.lock() {
                let _ = plugin.close_editor();
            }
            // The plugin's view is gone, so the child window can be destroyed.
            self.editor_window = None;
        }
        if let Some(mut window) = self.float_window.take() {
            window.close();
        }
        self.editor_open = false;
    }

    pub fn editor_open(&self) -> bool {
        self.editor_open
    }

    /// The size the plugin wants its editor to be.
    pub fn preferred_size(&mut self) -> Option<(u32, u32)> {
        let plugin = self.plugin.lock().ok()?;
        let (width, height) = plugin.get_editor_size().ok()?;
        if width <= 0 || height <= 0 {
            return None;
        }
        Some((width as u32, height as u32))
    }

    pub fn is_floating(&self) -> bool {
        self.float_window.is_some()
    }

    pub fn poll_events(&mut self) -> Vec<EditorEvent> {
        if let Some(window) = &self.float_window {
            if window.closed_by_user() {
                self.close_editor();
                return vec![EditorEvent::Closed];
            }
            let _ = window.service_platform_events();
        }
        Vec::new()
    }

    pub fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        let plugin = self
            .plugin
            .lock()
            .map_err(|_| PluginError::State("plugin lock poisoned".to_string()))?;
        plugin
            .save_state()
            .map_err(|error| PluginError::State(error.to_string()))
    }

    pub fn restore_state(&mut self, data: &[u8]) -> Result<(), PluginError> {
        if data.is_empty() {
            return Ok(());
        }
        let mut plugin = self
            .plugin
            .lock()
            .map_err(|_| PluginError::State("plugin lock poisoned".to_string()))?;
        plugin
            .load_state(data)
            .map_err(|error| PluginError::State(error.to_string()))
    }

    /// Parameter edits the plugin's own editor reported since the last call,
    /// as `(id, normalized value)`.
    ///
    /// `vst3-host` records them through the component handler the plugin calls
    /// from its GUI (`performEdit`) and, during `process()`, routes the same
    /// values into the processor's input queue — so the DSP and this list stay
    /// in sync. This host keeps every `PluginParam`'s own range at 0..1, which
    /// makes the normalized value the native value.
    ///
    /// `try_lock`, not `lock`: the audio thread owns the plugin most of the
    /// time, and a poll that lands mid-block must not stall a UI frame. A missed
    /// poll is not a lost edit — the stash is drained by the next one.
    pub fn poll_param_changes(&mut self) -> Vec<(u32, f64)> {
        match self.plugin.try_lock() {
            Ok(plugin) => plugin.get_parameter_changes(),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use replicantsynth_core::engine::PluginBlock;

    #[test]
    fn trailing_commas_are_tolerated() {
        let source = r#"{"a": 1, "b": [1, 2,], "c": {"d": "e,},",},}"#;
        let cleaned = strip_trailing_commas(source);
        assert!(
            serde_json::from_str::<serde_json::Value>(&cleaned).is_ok(),
            "{cleaned}"
        );
    }

    #[test]
    fn a_real_manifest_is_parsed_when_present() {
        // Read a bundle that ships a manifest, if this machine has one. A
        // runner with no plugins must not pretend it verified the parser.
        let mut checked = 0;
        for dir in crate::default_scan_dirs() {
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
                if !path.join("Contents/Resources/moduleinfo.json").exists() {
                    continue;
                }
                for descriptor in list(&path).unwrap_or_default() {
                    assert!(!descriptor.id.is_empty(), "{}", path.display());
                    assert!(!descriptor.name.is_empty(), "{}", path.display());
                    checked += 1;
                }
            }
        }
        println!("parsed {checked} class(es) from real manifests");
    }

    /// The first installed VST3 instrument on this machine, if there is one.
    fn an_installed_instrument() -> Option<(std::path::PathBuf, String)> {
        crate::scan(&crate::default_scan_dirs())
            .into_iter()
            .find(|descriptor| descriptor.format == PluginFormat::Vst3 && descriptor.is_instrument)
            .map(|descriptor| (descriptor.path, descriptor.id))
    }

    fn render(processor: &mut dyn PluginProcessor, blocks: usize) {
        for _ in 0..blocks {
            let mut block = PluginBlock {
                frames: 128,
                ..PluginBlock::default()
            };
            processor.process(&mut block);
        }
    }

    #[test]
    fn a_value_the_host_writes_reaches_the_plugin_and_does_not_echo_back() {
        let _serial = crate::test_support::one_plugin_at_a_time();
        let Some((path, id)) = an_installed_instrument() else {
            println!("no VST3 instrument installed; skipping");
            return;
        };
        let (mut runtime, mut processor) = Vst3Runtime::load(&path, &id, 48_000.0, 256, true)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));

        let Some(param) = runtime.params.iter().find(|param| !param.stepped).cloned() else {
            println!("{} exposes no editable parameter; skipping", path.display());
            return;
        };
        let target = if param.value > 0.5 { 0.0 } else { 1.0 };

        processor.set_param(param.id, target as f32);
        render(processor.as_mut(), 4);

        // The write landed in the plugin's own controller, so its editor and its
        // saved state agree with the knob the user turned here.
        let current = runtime
            .plugin
            .lock()
            .expect("plugin lock")
            .get_parameters()
            .expect("parameters")
            .into_iter()
            .find(|parameter| parameter.id == param.id)
            .map(|parameter| parameter.value)
            .expect("parameter");
        assert!(
            (current - target).abs() < 1e-3,
            "plugin holds {current}, expected {target}"
        );

        // ...and it is not reported as an edit, which would otherwise drag the
        // inspector's knob back to a stale value.
        assert!(
            runtime.poll_param_changes().is_empty(),
            "a value the host itself wrote must not come back as a plugin edit"
        );
    }
}
