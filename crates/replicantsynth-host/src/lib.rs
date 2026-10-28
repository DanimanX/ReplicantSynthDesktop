//! Plugin hosting for ReplicantSynth Desktop.
//!
//! Two formats, one shape: discover what is installed, load a plugin onto a pad,
//! run its audio on a worker thread, and show its own editor.
//!
//! ```no_run
//! # fn main() -> Result<(), replicantsynth_host::PluginError> {
//! use replicantsynth_host::{EditorRect, HostedPlugin, scan, default_scan_dirs};
//! use replicantsynth_core::engine::bridge_pair;
//!
//! let found = scan(&default_scan_dirs());
//! let Some(descriptor) = found.iter().find(|plugin| plugin.is_instrument) else {
//!     return Ok(());
//! };
//! let (mut plugin, _bridge) = HostedPlugin::load(descriptor, 48_000.0, 512, true)?;
//! plugin.open_editor(None, EditorRect::default())?;
//! # Ok(())
//! # }
//! ```

pub mod clap_backend;
pub mod gui;
pub mod scan;
pub mod vst3_backend;
pub mod worker;

pub use gui::{EditorRect, EditorWindow};
pub use scan::{default_scan_dirs, scan, PluginDescriptor};
pub use worker::{PluginProcessor, PluginWorker};

use replicantsynth_core::engine::{bridge_pair, PluginBridge};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum PluginError {
    Load(String),
    Instantiate(String),
    Audio(String),
    Embed(String),
    EmbedUnsupported,
    State(String),
    Unsupported(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::Load(message) => write!(formatter, "cannot load plugin: {message}"),
            PluginError::Instantiate(message) => {
                write!(formatter, "cannot instantiate plugin: {message}")
            }
            PluginError::Audio(message) => write!(formatter, "audio setup failed: {message}"),
            PluginError::Embed(message) => write!(formatter, "editor failed: {message}"),
            PluginError::EmbedUnsupported => {
                write!(formatter, "this platform cannot embed plugin editors yet")
            }
            PluginError::State(message) => write!(formatter, "plugin state failed: {message}"),
            PluginError::Unsupported(message) => write!(formatter, "unsupported: {message}"),
        }
    }
}

impl std::error::Error for PluginError {}

/// One automatable parameter, as the plugin describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginParam {
    pub id: u32,
    pub name: String,
    pub group: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub stepped: bool,
    pub value: f64,
}

impl PluginParam {
    /// Position of the value between 0 and 1, which is what the UI edits.
    pub fn normalized(&self) -> f64 {
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / span).clamp(0.0, 1.0)
        }
    }

    pub fn from_normalized(&self, normalized: f64) -> f64 {
        self.min + (self.max - self.min) * normalized.clamp(0.0, 1.0)
    }
}

/// Something the plugin's editor asked the host to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    Resized(u32, u32),
    Closed,
}

/// The host-side behaviour every format backend implements.
trait Backend {
    fn params(&self) -> &[PluginParam];
    /// Open the editor. `parent` is the application window's X11 id, or `None`
    /// when the session has none to embed into (Wayland); `title` names the
    /// window when it ends up detached.
    fn open_editor(
        &mut self,
        parent: Option<u64>,
        rect: EditorRect,
        title: &str,
    ) -> Result<(), PluginError>;
    fn close_editor(&mut self);
    fn set_editor_rect(&mut self, rect: EditorRect);
    /// Per-frame work the format needs while an editor is open (VST3's run loop).
    fn service(&mut self) {}
    /// The size the plugin wants its editor to be.
    fn preferred_size(&mut self) -> Option<(u32, u32)>;
    fn editor_open(&self) -> bool;
    fn is_floating(&self) -> bool;
    fn poll_events(&mut self) -> Vec<EditorEvent>;
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError>;
    fn restore_state(&mut self, data: &[u8]) -> Result<(), PluginError>;
    fn param_value(&mut self, id: u32) -> Option<f64>;
}

impl Backend for clap_backend::ClapRuntime {
    fn params(&self) -> &[PluginParam] {
        self.params()
    }
    fn service(&mut self) {
        self.service()
    }
    fn open_editor(
        &mut self,
        parent: Option<u64>,
        rect: EditorRect,
        title: &str,
    ) -> Result<(), PluginError> {
        self.open_editor(parent, rect, title)
    }
    fn close_editor(&mut self) {
        self.close_editor()
    }
    fn set_editor_rect(&mut self, rect: EditorRect) {
        self.set_editor_rect(rect)
    }
    fn preferred_size(&mut self) -> Option<(u32, u32)> {
        self.preferred_size()
    }
    fn editor_open(&self) -> bool {
        self.editor_open()
    }
    fn is_floating(&self) -> bool {
        self.is_floating()
    }
    fn poll_events(&mut self) -> Vec<EditorEvent> {
        self.poll_events()
    }
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        self.save_state()
    }
    fn restore_state(&mut self, data: &[u8]) -> Result<(), PluginError> {
        self.restore_state(data)
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        self.param_value(id)
    }
}

impl Backend for vst3_backend::Vst3Runtime {
    fn params(&self) -> &[PluginParam] {
        self.params()
    }
    fn service(&mut self) {
        self.service()
    }
    fn open_editor(
        &mut self,
        parent: Option<u64>,
        rect: EditorRect,
        title: &str,
    ) -> Result<(), PluginError> {
        self.open_editor(parent, rect, title)
    }
    fn close_editor(&mut self) {
        self.close_editor()
    }
    fn set_editor_rect(&mut self, rect: EditorRect) {
        self.set_editor_rect(rect)
    }
    fn preferred_size(&mut self) -> Option<(u32, u32)> {
        self.preferred_size()
    }
    fn editor_open(&self) -> bool {
        self.editor_open()
    }
    fn is_floating(&self) -> bool {
        self.is_floating()
    }
    fn poll_events(&mut self) -> Vec<EditorEvent> {
        self.poll_events()
    }
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        self.save_state()
    }
    fn restore_state(&mut self, data: &[u8]) -> Result<(), PluginError> {
        self.restore_state(data)
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        self.param_value(id)
    }
}

/// A loaded plugin: descriptor, main-thread runtime, worker and parameter list.
pub struct HostedPlugin {
    pub descriptor: PluginDescriptor,
    backend: Box<dyn Backend>,
    worker: PluginWorker,
    params: Vec<PluginParam>,
    /// Latest value each parameter was set to, for the UI's own display.
    pending: Vec<(u32, f32)>,
}

impl HostedPlugin {
    /// Load `descriptor` and start its worker.
    ///
    /// The returned [`PluginBridge`] is what the engine keeps; the
    /// [`HostedPlugin`] stays on the UI thread.
    /// `prefer_floating` asks for a detached plugin window instead of an
    /// embedded editor, which is what a Wayland session needs.
    pub fn load(
        descriptor: &PluginDescriptor,
        sample_rate: f64,
        max_block: usize,
        prefer_floating: bool,
    ) -> Result<(Self, PluginBridge), PluginError> {
        let (bridge, blocks_in, blocks_out) = bridge_pair(next_id());

        let (backend, processor): (Box<dyn Backend>, Box<dyn PluginProcessor>) =
            match descriptor.format {
                replicantsynth_core::model::PluginFormat::Clap => {
                    let (runtime, processor) = clap_backend::ClapRuntime::load(
                        &descriptor.path,
                        &descriptor.id,
                        sample_rate,
                        max_block,
                        prefer_floating,
                    )?;
                    (Box::new(runtime), processor)
                }
                replicantsynth_core::model::PluginFormat::Vst3 => {
                    let (runtime, processor) = vst3_backend::Vst3Runtime::load(
                        &descriptor.path,
                        &descriptor.id,
                        sample_rate,
                        max_block,
                        prefer_floating,
                    )?;
                    (Box::new(runtime), processor)
                }
            };

        let params = backend.params().to_vec();
        let worker = PluginWorker::spawn(processor, blocks_in, blocks_out);

        Ok((
            Self {
                descriptor: descriptor.clone(),
                backend,
                worker,
                params,
                pending: Vec::new(),
            },
            bridge,
        ))
    }

    pub fn params(&self) -> &[PluginParam] {
        &self.params
    }

    /// Set a parameter from its normalized 0..1 value.
    pub fn set_param_normalized(&mut self, id: u32, normalized: f64) {
        let Some(param) = self.params.iter().find(|param| param.id == id) else {
            return;
        };
        let value = param.from_normalized(normalized);
        if let Some(param) = self.params.iter_mut().find(|param| param.id == id) {
            param.value = value;
        }
        self.pending.push((id, normalized as f32));
        self.worker.set_param(id, normalized as f32);
    }

    /// Ask the plugin for its current value of one parameter.
    pub fn refresh_param(&mut self, id: u32) -> Option<f64> {
        let value = self.backend.param_value(id)?;
        if let Some(param) = self.params.iter_mut().find(|param| param.id == id) {
            param.value = value;
        }
        Some(value)
    }

    pub fn open_editor(
        &mut self,
        parent: Option<u64>,
        rect: EditorRect,
    ) -> Result<(), PluginError> {
        // A detached editor is a window of its own, so it needs a name the user
        // can pick out of their window list.
        let title = format!("{} — ReplicantSynth", self.descriptor.name);
        self.backend.open_editor(parent, rect, &title)
    }

    pub fn close_editor(&mut self) {
        self.backend.close_editor()
    }

    /// Reposition the editor to follow the host layout.
    pub fn set_editor_rect(&mut self, rect: EditorRect) {
        self.backend.set_editor_rect(rect)
    }

    /// Per-frame housekeeping the format needs while an editor is open.
    pub fn service(&mut self) {
        self.backend.service()
    }

    /// The size the plugin wants its editor to be, in physical pixels.
    pub fn preferred_editor_size(&mut self) -> Option<(u32, u32)> {
        self.backend.preferred_size()
    }

    pub fn editor_open(&self) -> bool {
        self.backend.editor_open()
    }

    /// True when the plugin draws its own top-level window instead of embedding.
    pub fn is_floating(&self) -> bool {
        self.backend.is_floating()
    }

    pub fn poll_events(&mut self) -> Vec<EditorEvent> {
        self.backend.poll_events()
    }

    pub fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        self.backend.save_state()
    }

    pub fn restore_state(&mut self, data: &[u8]) -> Result<(), PluginError> {
        self.backend.restore_state(data)
    }

    pub fn blocks_processed(&self) -> u64 {
        self.worker.blocks_processed()
    }

    pub fn path(&self) -> &Path {
        &self.descriptor.path
    }

    /// Stop the worker (dropping the plugin there) and tear down the editor.
    pub fn shut_down(&mut self) {
        self.backend.close_editor();
        self.worker.stop();
    }
}

impl Drop for HostedPlugin {
    fn drop(&mut self) {
        self.shut_down();
    }
}

impl fmt::Debug for HostedPlugin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostedPlugin")
            .field("descriptor", &self.descriptor)
            .field("params", &self.params.len())
            .finish()
    }
}

fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
