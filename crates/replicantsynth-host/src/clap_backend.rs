//! The CLAP backend: descriptor listing, instantiation, audio, parameters,
//! state and the plugin's own editor.
//!
//! Lifecycle follows the CLAP model: the instance lives on the UI thread, the
//! audio processor is `Send` and lives on the plugin's worker thread. When the
//! worker stops it drops the processor first, which is what lets the instance be
//! deactivated safely on the UI thread afterwards.

use crate::gui::{EditorRect, EditorWindow};
use crate::worker::PluginProcessor;
use crate::{EditorEvent, PluginError, PluginParam};
use clack_extensions::audio_ports::{AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui, Window as ClapWindow,
};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::note_ports::{NoteDialects, NotePortInfoBuffer, PluginNotePorts};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamInfoBuffer,
    ParamInfoFlags, ParamRescanFlags, PluginParams,
};
use clack_extensions::posix_fd::{FdFlags, HostPosixFd, HostPosixFdImpl, PluginPosixFd};
use clack_extensions::state::PluginState;
use clack_extensions::timer::{HostTimer, HostTimerImpl, PluginTimer, TimerId};
use clack_host::events::event_types::{NoteOffEvent, NoteOnEvent, ParamValueEvent};
use clack_host::events::io::EventBuffer;
use clack_host::events::{EventFlags, Match, Pckn};
use clack_host::prelude::*;
use clack_host::process::audio_buffers::{
    AudioPortBuffer, AudioPortBufferType, AudioPorts, InputChannel,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use replicantsynth_core::model::PluginFormat;
use std::cell::{Cell, OnceCell};
use std::ffi::CString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Enumerate every plugin contained in one `.clap` library.
///
/// This loads the library, because CLAP has no manifest to read instead. It
/// therefore only ever runs on the scanner thread.
pub fn list(path: &Path) -> Result<Vec<crate::PluginDescriptor>, String> {
    // SAFETY: loading a plugin library runs its code. Scanners do this; we only
    // ever do it from the scanner thread, never from the audio thread.
    let entry = unsafe { PluginEntry::load(path) }.map_err(|error| error.to_string())?;
    let Some(factory) = entry.get_plugin_factory() else {
        return Err("no plugin factory".to_string());
    };

    let mut out = Vec::new();
    for descriptor in factory.plugin_descriptors() {
        let (Some(id), Some(name)) = (descriptor.id(), descriptor.name()) else {
            continue;
        };
        let features: Vec<String> = descriptor
            .features()
            .map(|feature| feature.to_string_lossy().to_ascii_lowercase())
            .collect();
        let is_instrument = features.iter().any(|feature| feature == "instrument");
        let category = if is_instrument {
            "Instrument".to_string()
        } else if features.iter().any(|feature| feature == "audio-effect") {
            "Effect".to_string()
        } else if features.iter().any(|feature| feature == "note-effect") {
            "Note effect".to_string()
        } else {
            "Plugin".to_string()
        };
        out.push(crate::PluginDescriptor {
            format: PluginFormat::Clap,
            path: path.to_path_buf(),
            id: id.to_string_lossy().to_string(),
            name: name.to_string_lossy().to_string(),
            vendor: descriptor
                .vendor()
                .map(|vendor| vendor.to_string_lossy().to_string())
                .unwrap_or_default(),
            category,
            is_instrument,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Host callbacks
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum GuiRequest {
    Resize(u32, u32),
    Show,
    Hide,
    Closed,
}

struct ClapHost;

/// A timer the plugin asked the host to tick.
struct RegisteredTimer {
    id: u32,
    period: Duration,
    next: Instant,
}

/// State the plugin's callbacks and the UI thread both need.
struct HostState {
    requests: Sender<GuiRequest>,
    restart: AtomicBool,
    /// Timers the plugin registered; the host pumps them while an editor is open.
    timers: Mutex<Vec<RegisteredTimer>>,
    next_timer_id: AtomicU32,
    /// File descriptors the plugin wants watched (VSTGUI uses one for its
    /// connection to the X server).
    fds: Mutex<Vec<std::os::fd::RawFd>>,
}

struct ClapHostShared {
    state: std::sync::Arc<HostState>,
}

struct ClapHostMain<'a> {
    plugin: OnceCell<InitializedPluginHandle<'a>>,
    gui: Cell<Option<PluginGui>>,
    params: Cell<Option<PluginParams>>,
    state: Cell<Option<PluginState>>,
    #[allow(dead_code)]
    audio_ports: Cell<Option<PluginAudioPorts>>,
    #[allow(dead_code)]
    note_ports: Cell<Option<PluginNotePorts>>,
    shared: &'a ClapHostShared,
}

impl HostHandlers for ClapHost {
    type Shared<'a> = ClapHostShared;
    type MainThread<'a> = ClapHostMain<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostGui>()
            .register::<HostParams>()
            .register::<HostLog>()
            .register::<HostTimer>()
            .register::<HostPosixFd>();
    }
}

impl SharedHandler<'_> for ClapHostShared {
    fn request_restart(&self) {
        self.state.restart.store(true, Ordering::SeqCst);
    }

    fn request_process(&self) {}

    fn request_callback(&self) {}
}

impl<'a> MainThreadHandler<'a> for ClapHostMain<'a> {
    fn initialized(&self, plugin: InitializedPluginHandle<'a>) {
        self.gui.set(plugin.get_extension::<PluginGui>());
        self.params.set(plugin.get_extension::<PluginParams>());
        self.state.set(plugin.get_extension::<PluginState>());
        self.audio_ports
            .set(plugin.get_extension::<PluginAudioPorts>());
        self.note_ports
            .set(plugin.get_extension::<PluginNotePorts>());
        let _ = self.plugin.set(plugin);
    }
}

impl HostGuiImpl for ClapHostShared {
    fn resize_hints_changed(&self) {}

    fn request_resize(&self, new_size: GuiSize) -> Result<(), clack_host::prelude::HostError> {
        let _ = self
            .state
            .requests
            .send(GuiRequest::Resize(new_size.width, new_size.height));
        Ok(())
    }

    fn request_show(&self) -> Result<(), clack_host::prelude::HostError> {
        let _ = self.state.requests.send(GuiRequest::Show);
        Ok(())
    }

    fn request_hide(&self) -> Result<(), clack_host::prelude::HostError> {
        let _ = self.state.requests.send(GuiRequest::Hide);
        Ok(())
    }

    fn closed(&self, _was_destroyed: bool) {
        let _ = self.state.requests.send(GuiRequest::Closed);
    }
}

impl HostParamsImplShared for ClapHostShared {
    fn request_flush(&self) {}
}

impl HostParamsImplMainThread for ClapHostMain<'_> {
    fn rescan(&self, _flags: ParamRescanFlags) {}

    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostTimerImpl for ClapHostMain<'_> {
    fn register_timer(&self, period_ms: u32) -> Result<TimerId, clack_host::prelude::HostError> {
        let shared = &self.shared.state;
        // Anything faster than 16 ms only burns CPU for a GUI tick.
        let period = Duration::from_millis(u64::from(period_ms.clamp(16, 1000)));
        let id = shared.next_timer_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut timers) = shared.timers.lock() {
            timers.push(RegisteredTimer {
                id,
                period,
                next: Instant::now() + period,
            });
        }
        Ok(TimerId(id))
    }

    fn unregister_timer(&self, timer_id: TimerId) -> Result<(), clack_host::prelude::HostError> {
        if let Ok(mut timers) = self.shared.state.timers.lock() {
            timers.retain(|timer| timer.id != timer_id.0);
        }
        Ok(())
    }
}

impl HostPosixFdImpl for ClapHostMain<'_> {
    fn register_fd(
        &self,
        fd: std::os::fd::RawFd,
        _flags: FdFlags,
    ) -> Result<(), clack_host::prelude::HostError> {
        if let Ok(mut fds) = self.shared.state.fds.lock() {
            if !fds.contains(&fd) {
                fds.push(fd);
            }
        }
        Ok(())
    }

    fn modify_fd(
        &self,
        _fd: std::os::fd::RawFd,
        _flags: FdFlags,
    ) -> Result<(), clack_host::prelude::HostError> {
        Ok(())
    }

    fn unregister_fd(&self, fd: std::os::fd::RawFd) -> Result<(), clack_host::prelude::HostError> {
        if let Ok(mut fds) = self.shared.state.fds.lock() {
            fds.retain(|registered| *registered != fd);
        }
        Ok(())
    }
}

impl HostLogImpl for ClapHostShared {
    fn log(&self, severity: LogSeverity, message: &str) {
        if severity <= LogSeverity::Warning {
            eprintln!("[plugin] {message}");
        }
    }
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct PortLayout {
    pub inputs: Vec<u32>,
    pub outputs: Vec<u32>,
}

impl PortLayout {
    fn total_inputs(&self) -> usize {
        self.inputs.iter().map(|n| *n as usize).sum()
    }

    fn total_outputs(&self) -> usize {
        self.outputs.iter().map(|n| *n as usize).sum()
    }
}

/// The audio side of a CLAP plugin, owned by the worker thread.
pub struct ClapProcessor {
    processor: StartedPluginAudioProcessor<ClapHost>,
    input_ports: AudioPorts,
    output_ports: AudioPorts,
    input_channels: Vec<Vec<f32>>,
    output_channels: Vec<Vec<f32>>,
    events: EventBuffer,
    note_port: u16,
    pending_params: Vec<(u32, f32)>,
    frames: usize,
    counter: u64,
}

impl ClapProcessor {
    fn new(
        processor: StartedPluginAudioProcessor<ClapHost>,
        layout: PortLayout,
        note_port: u16,
        frames: usize,
    ) -> Self {
        let frames = frames.max(1);
        let input_channels = layout
            .inputs
            .iter()
            .map(|channels| vec![0.0; frames * *channels as usize])
            .collect();
        let output_channels = layout
            .outputs
            .iter()
            .map(|channels| vec![0.0; frames * *channels as usize])
            .collect();
        Self {
            processor,
            input_ports: AudioPorts::with_capacity(layout.total_inputs(), layout.inputs.len()),
            output_ports: AudioPorts::with_capacity(layout.total_outputs(), layout.outputs.len()),
            input_channels,
            output_channels,
            events: EventBuffer::with_capacity(64),
            note_port,
            pending_params: Vec::new(),
            frames,
            counter: 0,
        }
    }

    fn build_events(&mut self, block: &replicantsynth_core::engine::PluginBlock) {
        self.events.clear();
        for event in &block.events[..block.event_count] {
            let time = (event.offset as u32).min(block.frames as u32);
            let pckn = Pckn::new(self.note_port, 0u16, event.note as u16, Match::All);
            if event.on {
                self.events.push(
                    &NoteOnEvent::new(time, pckn, event.velocity as f64 / 127.0)
                        .with_flags(EventFlags::IS_LIVE),
                );
            } else {
                self.events
                    .push(&NoteOffEvent::new(time, pckn, 0.0).with_flags(EventFlags::IS_LIVE));
            }
        }
        let pending = std::mem::take(&mut self.pending_params);
        for (id, value) in pending {
            self.events.push(&ParamValueEvent::new(
                0,
                ClapId::new(id),
                Pckn::match_all(),
                value as f64,
            ));
        }
    }

    fn clear_buffers(&mut self, frames: usize) {
        let stride = self.frames.max(1);
        for buffer in &mut self.input_channels {
            let len = (frames * (buffer.len() / stride).max(1)).min(buffer.len());
            buffer[..len].fill(0.0);
        }
        for buffer in &mut self.output_channels {
            let len = (frames * (buffer.len() / stride).max(1)).min(buffer.len());
            buffer[..len].fill(0.0);
        }
    }

    fn write_input(&mut self, block: &replicantsynth_core::engine::PluginBlock, frames: usize) {
        let stride = self.frames.max(1);
        let Some(first_input) = self.input_channels.first_mut() else {
            return;
        };
        let channels = (first_input.len() / stride).max(1);
        if channels >= 2 {
            // The plugin wants planar channels in one contiguous buffer.
            first_input[..frames].copy_from_slice(&block.left[..frames]);
            first_input[frames..frames * 2].copy_from_slice(&block.right[..frames]);
        } else {
            for (target, (left, right)) in first_input
                .iter_mut()
                .zip(block.left.iter().zip(block.right.iter()))
                .take(frames)
            {
                *target = 0.5 * (left + right);
            }
        }
    }

    fn write_output(
        &mut self,
        block: &mut replicantsynth_core::engine::PluginBlock,
        frames: usize,
    ) {
        let stride = self.frames.max(1);
        let Some(first_output) = self.output_channels.first() else {
            return;
        };
        let channels = (first_output.len() / stride).max(1);
        if channels >= 2 {
            block.left[..frames].copy_from_slice(&first_output[..frames]);
            block.right[..frames].copy_from_slice(&first_output[frames..frames * 2]);
        } else if channels == 1 {
            block.left[..frames].copy_from_slice(&first_output[..frames]);
            block.right[..frames].copy_from_slice(&first_output[..frames]);
        }
    }
}

impl PluginProcessor for ClapProcessor {
    fn set_param(&mut self, id: u32, value: f32) {
        self.pending_params.push((id, value));
    }

    fn process(&mut self, block: &mut replicantsynth_core::engine::PluginBlock) {
        let frames = block.frames.min(self.frames);
        if frames == 0 {
            return;
        }
        self.clear_buffers(frames);
        self.write_input(block, frames);
        self.build_events(block);

        let frame_count = self.frames;
        let input_buffers =
            self.input_ports
                .with_input_buffers(self.input_channels.iter_mut().map(|port_buffer| {
                    AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_input_only(
                            port_buffer
                                .chunks_exact_mut(frame_count)
                                .map(|channel| InputChannel {
                                    buffer: &mut channel[..frames],
                                    is_constant: false,
                                }),
                        ),
                    }
                }));
        let mut output_buffers =
            self.output_ports
                .with_output_buffers(self.output_channels.iter_mut().map(|port_buffer| {
                    AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            port_buffer
                                .chunks_exact_mut(frame_count)
                                .map(|channel| &mut channel[..frames]),
                        ),
                    }
                }));

        let input_events = self.events.as_input();
        let mut output_events = OutputEvents::void();
        let result = self.processor.process(
            &input_buffers,
            &mut output_buffers,
            &input_events,
            &mut output_events,
            Some(self.counter),
            None,
        );
        if result.is_ok() {
            self.write_output(block, frames);
        }
        self.counter += frames as u64;
    }
}

// ---------------------------------------------------------------------------
// Main-thread runtime
// ---------------------------------------------------------------------------

/// The UI-thread half of a loaded CLAP plugin.
pub struct ClapRuntime {
    instance: PluginInstance<ClapHost>,
    gui: Option<PluginGui>,
    configuration: Option<GuiConfiguration<'static>>,
    requests: Receiver<GuiRequest>,
    params: Vec<PluginParam>,
    editor: Option<EditorWindow>,
    editor_open: bool,
    /// The plugin's timer and fd extensions, plus the host state that tracks
    /// what it registered. A VSTGUI editor only paints when these are pumped,
    /// which is the same reason the VST3 host has to service its run loop.
    timers: Option<PluginTimer>,
    posix_fd: Option<PluginPosixFd>,
    state: std::sync::Arc<HostState>,
}

impl ClapRuntime {
    /// Instantiate the plugin, activate it and hand back its audio processor.
    pub fn load(
        path: &Path,
        plugin_id: &str,
        sample_rate: f64,
        max_block: usize,
        prefer_floating: bool,
    ) -> Result<(Self, Box<dyn PluginProcessor>), PluginError> {
        let entry = unsafe { PluginEntry::load(path) }
            .map_err(|error| PluginError::Load(error.to_string()))?;
        let id = CString::new(plugin_id)
            .map_err(|error| PluginError::Load(format!("invalid plugin id: {error}")))?;
        let host_info = HostInfo::new(
            "ReplicantSynth",
            "DanimanX",
            "https://github.com/DanimanX/ReplicantSynthDesktop",
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|error| PluginError::Load(error.to_string()))?;

        let (requests_tx, requests_rx) = unbounded();
        let state = std::sync::Arc::new(HostState {
            requests: requests_tx,
            restart: AtomicBool::new(false),
            timers: Mutex::new(Vec::new()),
            next_timer_id: AtomicU32::new(1),
            fds: Mutex::new(Vec::new()),
        });
        let host_state = state.clone();
        let mut instance = PluginInstance::<ClapHost>::new(
            |_| ClapHostShared {
                state: host_state.clone(),
            },
            |shared| ClapHostMain {
                plugin: OnceCell::new(),
                gui: Cell::new(None),
                params: Cell::new(None),
                state: Cell::new(None),
                audio_ports: Cell::new(None),
                note_ports: Cell::new(None),
                shared,
            },
            &entry,
            &id,
            &host_info,
        )
        .map_err(|error| PluginError::Instantiate(error.to_string()))?;

        let (params, layout, note_port) = inspect(&mut instance)?;

        let configuration = instance
            .access_handler(|handler| handler.gui.get())
            .and_then(|gui| negotiate(gui, &mut instance, prefer_floating));

        let processor = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate,
                    min_frames_count: 1,
                    max_frames_count: max_block.max(1) as u32,
                },
            )
            .map_err(|error| PluginError::Audio(error.to_string()))?
            .start_processing()
            .map_err(|error| PluginError::Audio(error.to_string()))?;

        let gui = instance.access_handler(|handler| handler.gui.get());
        let runtime = Self {
            instance,
            gui,
            configuration,
            requests: requests_rx,
            params,
            editor: None,
            editor_open: false,
            timers: None,
            posix_fd: None,
            state,
        };
        let processor = ClapProcessor::new(processor, layout, note_port, max_block);
        Ok((runtime, Box::new(processor) as Box<dyn PluginProcessor>))
    }

    pub fn params(&self) -> &[PluginParam] {
        &self.params
    }

    pub fn is_floating(&self) -> bool {
        self.configuration
            .map(|configuration| configuration.is_floating)
            .unwrap_or(true)
    }

    pub fn open_editor(
        &mut self,
        parent: Option<u64>,
        rect: EditorRect,
    ) -> Result<(), PluginError> {
        if self.editor_open {
            return Ok(());
        }
        let Some(gui) = self.gui else {
            return Err(PluginError::Embed("this plugin has no editor".to_string()));
        };
        let Some(configuration) = self.configuration else {
            return Err(PluginError::Embed(
                "no compatible GUI API for this plugin".to_string(),
            ));
        };

        let handle = self.instance.plugin_handle();
        gui.create(&handle, configuration)
            .map_err(|error| PluginError::Embed(error.to_string()))?;

        let preferred = gui.get_size(&handle).unwrap_or(GuiSize {
            width: rect.width.max(320),
            height: rect.height.max(240),
        });

        if configuration.is_floating {
            gui.suggest_title(&handle, c"ReplicantSynth plugin");
            let _ = gui.show(&handle);
            self.editor_open = true;
            return Ok(());
        }

        let Some(parent) = parent else {
            gui.destroy(&handle);
            return Err(PluginError::EmbedUnsupported);
        };

        let width = preferred.width.min(rect.width.max(160)).max(160);
        let height = preferred.height.min(rect.height.max(120)).max(120);
        let window = EditorWindow::open(parent, "plugin editor", width, height, rect.x, rect.y)?;

        // SAFETY: the window outlives the plugin's GUI; it is destroyed in
        // `close_editor`, after `gui.destroy` has removed the plugin's child.
        let result =
            unsafe { gui.set_parent(&handle, ClapWindow::from_x11_handle(window.handle())) };
        if result.is_err() {
            drop(window);
            gui.destroy(&handle);
            return Err(PluginError::Embed("set_parent was refused".to_string()));
        }
        gui.show(&handle).ok();
        self.editor = Some(window);
        self.editor_open = true;
        Ok(())
    }

    /// The size the plugin's editor currently has.
    ///
    /// CLAP cannot report this without creating the GUI, and creating it twice
    /// (once to probe, once to open) leaves some plugins with a stale window that
    /// never paints. So this reports the live size only, and the host sizes the
    /// editor from it once the editor exists.
    pub fn preferred_size(&mut self) -> Option<(u32, u32)> {
        if !self.editor_open {
            return None;
        }
        let gui = self.gui?;
        let handle = self.instance.plugin_handle();
        gui.get_size(&handle).map(|size| (size.width, size.height))
    }

    /// Keep the plugin's editor in step with the region the host reserved.
    pub fn set_editor_rect(&mut self, rect: EditorRect) {
        if !self.editor_open {
            return;
        }
        let Some(gui) = self.gui else {
            return;
        };
        let handle = self.instance.plugin_handle();
        if let Some(window) = &self.editor {
            window.resize(rect.width, rect.height);
            window.move_to(rect.x, rect.y);
        }
        if gui.can_resize(&handle) {
            gui.set_size(
                &handle,
                GuiSize {
                    width: rect.width.max(80),
                    height: rect.height.max(60),
                },
            )
            .ok();
        }
    }

    pub fn close_editor(&mut self) {
        if !self.editor_open {
            return;
        }
        let handle = self.instance.plugin_handle();
        if let Some(gui) = self.gui {
            gui.hide(&handle).ok();
            gui.destroy(&handle);
        }
        self.editor = None;
        self.editor_open = false;
    }

    pub fn editor_open(&self) -> bool {
        self.editor_open
    }

    /// Tick the plugin's timers and pump its watched file descriptors.
    ///
    /// A VSTGUI-based editor (most CLAP instruments) does not paint until the
    /// host drives its timers and X11 file descriptor. Call this every frame
    /// while the editor is open.
    pub fn service(&mut self) {
        if !self.editor_open {
            return;
        }
        if self.timers.is_none() || self.posix_fd.is_none() {
            let handle = self.instance.plugin_handle();
            self.timers = handle.get_extension::<PluginTimer>();
            self.posix_fd = handle.get_extension::<PluginPosixFd>();
        }

        let due: Vec<TimerId> = {
            let Ok(mut timers) = self.state.timers.lock() else {
                return;
            };
            let now = Instant::now();
            let mut due = Vec::new();
            for timer in timers.iter_mut() {
                if now >= timer.next {
                    timer.next = now + timer.period;
                    due.push(TimerId(timer.id));
                }
            }
            due
        };
        if !due.is_empty() {
            if let Some(timers) = self.timers {
                let handle = self.instance.plugin_handle();
                for id in due {
                    timers.on_timer(&handle, id);
                }
            }
        }

        let fds: Vec<std::os::fd::RawFd> = self
            .state
            .fds
            .lock()
            .map(|fds| fds.clone())
            .unwrap_or_default();
        if fds.is_empty() {
            return;
        }
        if let Some(posix_fd) = self.posix_fd {
            let handle = self.instance.plugin_handle();
            for fd in fds {
                if fd_is_readable(fd) {
                    posix_fd.on_fd(&handle, fd, FdFlags::READ);
                }
            }
        }
    }

    pub fn poll_events(&mut self) -> Vec<EditorEvent> {
        let mut out = Vec::new();
        while let Ok(request) = self.requests.try_recv() {
            match request {
                GuiRequest::Resize(width, height) => {
                    if let Some(window) = &self.editor {
                        window.resize(width, height);
                    }
                    out.push(EditorEvent::Resized(width, height));
                }
                GuiRequest::Show => {
                    if let Some(window) = &self.editor {
                        window.show();
                    }
                }
                GuiRequest::Hide => {
                    if let Some(window) = &self.editor {
                        window.hide();
                    }
                }
                GuiRequest::Closed => {
                    self.close_editor();
                    out.push(EditorEvent::Closed);
                }
            }
        }
        out
    }

    pub fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        let Some(state) = self.instance.access_handler(|handler| handler.state.get()) else {
            return Ok(Vec::new());
        };
        let handle = self.instance.plugin_handle();
        let mut bytes = Vec::new();
        state
            .save(&handle, &mut bytes)
            .map_err(|error| PluginError::State(error.to_string()))?;
        Ok(bytes)
    }

    pub fn restore_state(&mut self, data: &[u8]) -> Result<(), PluginError> {
        if data.is_empty() {
            return Ok(());
        }
        let Some(state) = self.instance.access_handler(|handler| handler.state.get()) else {
            return Ok(());
        };
        let handle = self.instance.plugin_handle();
        let mut cursor = data;
        state
            .load(&handle, &mut cursor)
            .map_err(|error| PluginError::State(error.to_string()))
    }

    pub fn param_value(&mut self, id: u32) -> Option<f64> {
        let params = self
            .instance
            .access_handler(|handler| handler.params.get())?;
        let handle = self.instance.plugin_handle();
        params.get_value(&handle, ClapId::new(id))
    }
}

/// Is this descriptor readable right now? Used to drive the plugin's own
/// event loop without ever blocking the UI thread.
fn fd_is_readable(fd: std::os::fd::RawFd) -> bool {
    let mut poll_fd = libc_pollfd {
        fd,
        events: 1, // POLLIN
        revents: 0,
    };
    // SAFETY: a single well-formed pollfd for zero timeout.
    let ready = unsafe { poll(&mut poll_fd, 1, 0) };
    ready > 0 && poll_fd.revents != 0
}

#[repr(C)]
struct libc_pollfd {
    fd: std::os::fd::RawFd,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut libc_pollfd, nfds: std::ffi::c_ulong, timeout: i32) -> i32;
}

/// Pick the GUI configuration to use.
///
/// Embedding is preferred, but only when the host window can actually parent a
/// child window — on Wayland it cannot, so the caller asks for floating instead.
fn negotiate(
    gui: PluginGui,
    instance: &mut PluginInstance<ClapHost>,
    prefer_floating: bool,
) -> Option<GuiConfiguration<'static>> {
    let api_type = GuiApiType::default_for_current_platform()?;
    let handle = instance.plugin_handle();

    let mut config = GuiConfiguration {
        api_type,
        is_floating: prefer_floating,
    };
    if gui.is_api_supported(&handle, config) {
        return Some(config);
    }
    config.is_floating = !config.is_floating;
    if gui.is_api_supported(&handle, config) {
        return Some(config);
    }
    None
}

fn inspect(
    instance: &mut PluginInstance<ClapHost>,
) -> Result<(Vec<PluginParam>, PortLayout, u16), PluginError> {
    let mut params = Vec::new();
    let handle = instance.plugin_handle();

    if let Some(extension) = handle.get_extension::<PluginParams>() {
        let count = extension.count(&handle).min(2048);
        for index in 0..count {
            let mut buffer = ParamInfoBuffer::new();
            let Some(info) = extension.get_info(&handle, index, &mut buffer) else {
                continue;
            };
            params.push(PluginParam {
                id: info.id.get(),
                name: String::from_utf8_lossy(info.name).trim().to_string(),
                group: String::from_utf8_lossy(info.module).trim().to_string(),
                min: info.min_value,
                max: info.max_value,
                default: info.default_value,
                stepped: info.flags.contains(ParamInfoFlags::IS_STEPPED),
                value: extension
                    .get_value(&handle, info.id)
                    .unwrap_or(info.default_value),
            });
        }
    }

    let mut layout = PortLayout::default();
    if let Some(extension) = handle.get_extension::<PluginAudioPorts>() {
        let mut buffer = AudioPortInfoBuffer::new();
        for is_input in [true, false] {
            let count = extension.count(&handle, is_input).min(16);
            for index in 0..count {
                let Some(info) = extension.get(&handle, index, is_input, &mut buffer) else {
                    continue;
                };
                let channels = info.channel_count.max(1);
                if is_input {
                    layout.inputs.push(channels);
                } else {
                    layout.outputs.push(channels);
                }
            }
        }
    }
    if layout.outputs.is_empty() {
        // A plugin that reports no output ports cannot be heard; assume stereo
        // rather than silence it entirely.
        layout.outputs.push(2);
    }

    let mut note_port = 0u16;
    if let Some(extension) = handle.get_extension::<PluginNotePorts>() {
        let mut buffer = NotePortInfoBuffer::new();
        let count = extension.count(&handle, true).min(8);
        for index in 0..count {
            let Some(info) = extension.get(&handle, index, true, &mut buffer) else {
                continue;
            };
            if info
                .supported_dialects
                .intersects(NoteDialects::CLAP | NoteDialects::MIDI)
            {
                note_port = index as u16;
                break;
            }
        }
    }

    Ok((params, layout, note_port))
}
