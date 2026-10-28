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
    /// `(id, value)` pairs the plugin changed from its own editor since the last
    /// call, in each parameter's own range. Formats that notify the host return
    /// only what moved; a format that does not poll instead.
    fn poll_param_changes(&mut self) -> Vec<(u32, f64)> {
        Vec::new()
    }
    /// Every current parameter value, for when the plugin's values changed
    /// wholesale — loading a state blob, most of all — and no edit was reported.
    fn read_all_params(&mut self) -> Vec<(u32, f64)> {
        Vec::new()
    }
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
    fn poll_param_changes(&mut self) -> Vec<(u32, f64)> {
        self.poll_param_changes()
    }
    fn read_all_params(&mut self) -> Vec<(u32, f64)> {
        self.read_all_params()
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
    fn poll_param_changes(&mut self) -> Vec<(u32, f64)> {
        self.poll_param_changes()
    }
    fn read_all_params(&mut self) -> Vec<(u32, f64)> {
        self.read_all_params()
    }
}

/// A loaded plugin: descriptor, main-thread runtime, worker and parameter list.
pub struct HostedPlugin {
    pub descriptor: PluginDescriptor,
    backend: Box<dyn Backend>,
    worker: PluginWorker,
    params: Vec<PluginParam>,
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
        self.worker.set_param(id, normalized as f32);
    }

    /// Mirror parameter values the plugin changed from its own editor into the
    /// cached list the UI displays.
    ///
    /// Turning a knob in the plugin's own window is the plugin telling the host
    /// that a parameter moved. Nothing else in this application reads the
    /// parameter list between frames, so without this poll the inspector kept
    /// showing the values that were current when the plugin was loaded.
    ///
    /// Returns true when a value actually moved, so the caller can ask for a
    /// fresh frame instead of repainting on a plugin that reported nothing.
    pub fn sync_params(&mut self) -> bool {
        let changes = self.backend.poll_param_changes();
        apply_param_values(&mut self.params, &changes)
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
        self.backend.restore_state(data)?;
        // A state blob moves every parameter at once and reports none of them,
        // so the cached list has to be re-read or the panel keeps showing the
        // values the plugin had before the project was reopened.
        let values = self.backend.read_all_params();
        apply_param_values(&mut self.params, &values);
        Ok(())
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

/// Fold `(id, value)` pairs the plugin reported into the cached parameter list.
///
/// Values arrive in each parameter's own range, ids the list does not know are
/// ignored, and a value that did not really move is not a change — a format
/// without change notification (CLAP) hands back every value it has, and a float
/// re-read unchanged must not turn into a repaint on every frame.
///
/// Returns true when at least one cached value moved.
fn apply_param_values(params: &mut [PluginParam], changes: &[(u32, f64)]) -> bool {
    if changes.is_empty() {
        return false;
    }
    // A big instrument has thousands of parameters and CLAP reports all of them,
    // so scan for the small batches (VST3 sends only what was edited) and index
    // once when the batch stops being small enough for a scan per change.
    let index: Option<std::collections::HashMap<u32, usize>> = if changes.len() > 8 {
        Some(
            params
                .iter()
                .enumerate()
                .map(|(position, param)| (param.id, position))
                .collect(),
        )
    } else {
        None
    };

    let mut moved = false;
    for &(id, value) in changes {
        let position = match &index {
            Some(index) => index.get(&id).copied(),
            None => params.iter().position(|param| param.id == id),
        };
        let Some(position) = position else {
            continue;
        };
        let param = &mut params[position];
        let span = (param.max - param.min).abs();
        if (param.value - value).abs() <= (span * 1e-6).max(1e-9) {
            continue;
        }
        param.value = value;
        moved = true;
    }
    moved
}

fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Test-only coordination for the tests that instantiate real plugins.
///
/// Instrument plugins are heavy global state: two instances created from two
/// test threads at once can corrupt each other's heap (u-he aborts with
/// `malloc(): invalid size`). Every test that loads a plugin takes this lock for
/// its whole body, so the suite stays parallel for everything else.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    pub fn one_plugin_at_a_time() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(id: u32, value: f64) -> PluginParam {
        PluginParam {
            id,
            name: format!("param {id}"),
            group: String::new(),
            min: 0.0,
            max: 1.0,
            default: 0.5,
            stepped: false,
            value,
        }
    }

    /// A backend whose parameter reports the test scripts, and a processor that
    /// renders nothing: together they stand in for a plugin's own editor moving
    /// a knob, without needing a plugin or a display.
    struct ScriptedBackend {
        params: Vec<PluginParam>,
        reports: std::collections::VecDeque<Vec<(u32, f64)>>,
        /// What a state restore leaves behind, read back in full.
        after_restore: Vec<(u32, f64)>,
    }

    impl Backend for ScriptedBackend {
        fn params(&self) -> &[PluginParam] {
            &self.params
        }
        fn open_editor(
            &mut self,
            _parent: Option<u64>,
            _rect: EditorRect,
            _title: &str,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn close_editor(&mut self) {}
        fn set_editor_rect(&mut self, _rect: EditorRect) {}
        fn preferred_size(&mut self) -> Option<(u32, u32)> {
            None
        }
        fn editor_open(&self) -> bool {
            true
        }
        fn is_floating(&self) -> bool {
            false
        }
        fn poll_events(&mut self) -> Vec<EditorEvent> {
            Vec::new()
        }
        fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
            Ok(Vec::new())
        }
        fn restore_state(&mut self, _data: &[u8]) -> Result<(), PluginError> {
            Ok(())
        }
        fn poll_param_changes(&mut self) -> Vec<(u32, f64)> {
            self.reports.pop_front().unwrap_or_default()
        }
        fn read_all_params(&mut self) -> Vec<(u32, f64)> {
            self.after_restore.clone()
        }
    }

    struct SilentProcessor;

    impl crate::worker::PluginProcessor for SilentProcessor {
        fn process(&mut self, _block: &mut replicantsynth_core::engine::PluginBlock) {}
        fn set_param(&mut self, _id: u32, _value: f32) {}
    }

    /// A `HostedPlugin` whose plugin is the script above.
    fn scripted(reports: Vec<Vec<(u32, f64)>>) -> HostedPlugin {
        let params = vec![param(1, 0.0), param(2, 0.5)];
        let (bridge, blocks_in, blocks_out) = bridge_pair(next_id());
        drop(bridge);
        HostedPlugin {
            descriptor: PluginDescriptor {
                format: replicantsynth_core::model::PluginFormat::Clap,
                path: std::path::PathBuf::from("/dev/null"),
                id: "test".to_string(),
                name: "scripted".to_string(),
                vendor: String::new(),
                category: "Instrument".to_string(),
                is_instrument: true,
            },
            backend: Box::new(ScriptedBackend {
                params: params.clone(),
                reports: reports.into(),
                after_restore: Vec::new(),
            }),
            worker: crate::worker::PluginWorker::spawn(
                Box::new(SilentProcessor),
                blocks_in,
                blocks_out,
            ),
            params,
        }
    }

    #[test]
    fn a_knob_turned_in_the_plugins_own_editor_reaches_the_ui_list() {
        // What `params()` returns is what the inspector draws, so a value the
        // plugin reported has to appear there or the tab never moves.
        let mut plugin = scripted(vec![vec![(2, 0.9)]]);
        assert!(plugin.sync_params());
        assert_eq!(plugin.params()[1].value, 0.9);
        assert_eq!(plugin.params()[0].value, 0.0);
    }

    #[test]
    fn restoring_state_refreshes_the_whole_list() {
        // A state blob moves every parameter and reports none of them, so the
        // panel would keep the values the plugin had when it was loaded.
        let mut plugin = scripted(vec![Vec::new()]);
        plugin.backend = Box::new(ScriptedBackend {
            params: plugin.params.clone(),
            reports: Default::default(),
            after_restore: vec![(1, 0.75), (2, 0.1)],
        });
        plugin.restore_state(&[1, 2, 3]).expect("restore");
        assert_eq!(plugin.params()[0].value, 0.75);
        assert_eq!(plugin.params()[1].value, 0.1);
    }

    #[test]
    fn a_quiet_frame_reports_no_change() {
        let mut plugin = scripted(vec![Vec::new()]);
        assert!(!plugin.sync_params());
    }

    #[test]
    fn a_change_the_plugin_reported_reaches_the_cached_list() {
        let mut params = vec![param(1, 0.25), param(2, 0.5)];
        assert!(apply_param_values(&mut params, &[(2, 0.75)]));
        assert_eq!(params[0].value, 0.25);
        assert_eq!(params[1].value, 0.75);
    }

    #[test]
    fn a_value_re_read_unchanged_is_not_a_change() {
        let mut params = vec![param(1, 0.25)];
        assert!(!apply_param_values(&mut params, &[(1, 0.25)]));
        // Re-reading across a float round-trip must not count either.
        assert!(!apply_param_values(&mut params, &[(1, 0.25 + 1e-12)]));
    }

    #[test]
    fn ids_the_list_does_not_know_are_ignored() {
        let mut params = vec![param(1, 0.25)];
        assert!(!apply_param_values(&mut params, &[(99, 1.0)]));
        assert_eq!(params[0].value, 0.25);
    }

    #[test]
    fn a_big_batch_still_lands_on_the_right_parameter() {
        // Past the scan threshold the fold indexes by id instead of scanning, so
        // this pins the indexed path: 2000 parameters, every value moved.
        let mut params: Vec<PluginParam> = (0..2000).map(|id| param(id * 3 + 7, 0.0)).collect();
        let changes: Vec<(u32, f64)> = params.iter().map(|param| (param.id, 0.5)).collect();
        assert!(apply_param_values(&mut params, &changes));
        assert!(params.iter().all(|param| param.value == 0.5));
        // Twice in a row is a no-op: the display settles instead of repainting.
        assert!(!apply_param_values(&mut params, &changes));
    }
}
