//! Plugin loading and lifetime, on the UI thread.
//!
//! Scanning and loading both execute third-party code, so neither happens on the
//! audio thread and neither happens inside a frame: scanning runs on a worker
//! thread and its result arrives as a message.

use crate::ui::theme;
use egui::Color32;
use replicantsynth_core::engine::{EngineHandles, PluginHostCommand};
use replicantsynth_core::model::{PluginFormat, PluginRef};
use replicantsynth_host::{
    default_scan_dirs, scan, EditorEvent, EditorRect, HostedPlugin, PluginDescriptor,
};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

/// What the scanner thread reports back.
pub enum ScanMessage {
    Found(Vec<PluginDescriptor>),
    Failed(String),
}

/// One plugin loaded onto a pad.
pub struct LoadedPlugin {
    pub plugin: HostedPlugin,
    /// The region the editor occupies, in physical pixels relative to the window.
    pub rect: EditorRect,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct PluginManager {
    pub available: Vec<PluginDescriptor>,
    pub scanning: bool,
    scanner: Option<Receiver<ScanMessage>>,
    pub loaded: HashMap<usize, LoadedPlugin>,
    pub scan_error: Option<String>,
    scanned: bool,
    /// Editors the user closed from their own window since the last frame.
    closed_editors: Vec<usize>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a scan on a worker thread, if one is not already running.
    pub fn start_scan(&mut self, force: bool) {
        if self.scanning || (self.scanned && !force) {
            return;
        }
        self.scanning = true;
        self.scan_error = None;
        let (sender, receiver): (Sender<ScanMessage>, Receiver<ScanMessage>) = channel();
        self.scanner = Some(receiver);
        std::thread::Builder::new()
            .name("replicant-scan".to_string())
            .spawn(move || {
                let message = match std::panic::catch_unwind(|| scan(&default_scan_dirs())) {
                    Ok(found) => ScanMessage::Found(found),
                    Err(_) => ScanMessage::Failed(
                        "the plugin scanner crashed while reading a plugin".to_string(),
                    ),
                };
                let _ = sender.send(message);
            })
            .ok();
    }

    /// Collect scan results; returns true when something new arrived.
    pub fn poll_scan(&mut self) -> bool {
        let Some(receiver) = &self.scanner else {
            return false;
        };
        match receiver.try_recv() {
            Ok(ScanMessage::Found(found)) => {
                self.available = found;
                self.scanning = false;
                self.scanned = true;
                self.scanner = None;
                true
            }
            Ok(ScanMessage::Failed(error)) => {
                self.scan_error = Some(error);
                self.scanning = false;
                self.scanned = true;
                self.scanner = None;
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.scanning = false;
                self.scanner = None;
                true
            }
        }
    }

    pub fn is_loaded(&self, pad: usize) -> bool {
        self.loaded.contains_key(&pad)
    }

    pub fn get(&self, pad: usize) -> Option<&LoadedPlugin> {
        self.loaded.get(&pad)
    }

    pub fn get_mut(&mut self, pad: usize) -> Option<&mut LoadedPlugin> {
        self.loaded.get_mut(&pad)
    }

    /// Load `descriptor` onto `pad`, replacing whatever was there.
    ///
    /// The plugin's audio runs on its own worker thread; the engine only keeps
    /// the bridge. Everything that can fail here fails before the engine is told
    /// anything, so a failed load leaves the pad untouched.
    pub fn load(
        &mut self,
        pad: usize,
        descriptor: &PluginDescriptor,
        sample_rate: u32,
        max_block: usize,
        prefer_floating: bool,
        handles: &EngineHandles,
    ) -> Result<PluginRef, String> {
        self.unload(pad, handles);

        let (plugin, bridge) =
            HostedPlugin::load(descriptor, sample_rate as f64, max_block, prefer_floating)
                .map_err(|error| error.to_string())?;

        let sent = handles
            .plugin_sender()
            .send(PluginHostCommand::Install {
                pad: pad as u8,
                bridge: Box::new(bridge),
            })
            .is_ok();
        if !sent {
            return Err("the engine stopped accepting plugins".to_string());
        }

        let reference = PluginRef {
            format: descriptor.format,
            path: descriptor.path.display().to_string(),
            id: descriptor.id.clone(),
            name: descriptor.name.clone(),
            vendor: descriptor.vendor.clone(),
            state: None,
        };

        self.loaded.insert(
            pad,
            LoadedPlugin {
                plugin,
                rect: EditorRect::default(),
                error: None,
            },
        );
        Ok(reference)
    }

    /// Detach the plugin from a pad and tear it down off the audio thread.
    pub fn unload(&mut self, pad: usize, handles: &EngineHandles) {
        let _ = handles
            .plugin_sender()
            .send(PluginHostCommand::Remove { pad: pad as u8 });
        if let Some(mut loaded) = self.loaded.remove(&pad) {
            loaded.plugin.shut_down();
        }
        handles.drain_retired_bridges();
    }

    /// Tear down every loaded plugin, used when the project is replaced.
    pub fn shut_all(&mut self, handles: &EngineHandles) {
        for pad in self.loaded.keys() {
            let _ = handles
                .plugin_sender()
                .send(PluginHostCommand::Remove { pad: *pad as u8 });
        }
        for (_, mut loaded) in self.loaded.drain() {
            loaded.plugin.shut_down();
        }
        handles.drain_retired_bridges();
    }

    /// Per-frame housekeeping: plugin editor requests and teardown backlogs.
    ///
    /// Returns true when a parameter moved inside a plugin's own editor, so the
    /// caller can ask for a repaint: the editor is a window of the plugin, not of
    /// this UI, so dragging a knob there produces no input event here and the
    /// frame would otherwise wait for the idle refresh.
    pub fn poll(&mut self, handles: &EngineHandles) -> bool {
        handles.drain_retired_bridges();
        let mut moved = false;
        let mut closed = Vec::new();
        for (pad, loaded) in self.loaded.iter_mut() {
            loaded.plugin.service();
            if loaded.plugin.sync_params() && loaded.plugin.editor_open() {
                moved = true;
            }
            for event in loaded.plugin.poll_events() {
                match event {
                    EditorEvent::Closed => {
                        // The user closed the editor's own window: the panel that
                        // was showing it has to go away too.
                        loaded.rect = EditorRect::default();
                        closed.push(*pad);
                    }
                    EditorEvent::Resized(width, height) => {
                        loaded.rect.width = width;
                        loaded.rect.height = height;
                    }
                }
            }
        }
        self.closed_editors.extend(closed);
        moved
    }

    /// Editors closed from their own window, once each.
    pub fn take_closed_editors(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.closed_editors)
    }

    /// The size the plugin's editor is (or wants to be), in physical pixels.
    pub fn preferred_size(&mut self, pad: usize) -> Option<(u32, u32)> {
        self.loaded.get_mut(&pad)?.plugin.preferred_editor_size()
    }

    pub fn open_editor(&mut self, pad: usize, parent: Option<u64>, rect: EditorRect) -> bool {
        let Some(loaded) = self.loaded.get_mut(&pad) else {
            return false;
        };
        loaded.rect = rect;
        match loaded.plugin.open_editor(parent, rect) {
            Ok(()) => {
                loaded.error = None;
                true
            }
            Err(error) => {
                loaded.error = Some(error.to_string());
                false
            }
        }
    }

    /// Keep an open editor aligned with the region the UI reserved for it.
    pub fn set_editor_rect(&mut self, pad: usize, rect: EditorRect) {
        if let Some(loaded) = self.loaded.get_mut(&pad) {
            if loaded.rect == rect {
                return;
            }
            loaded.rect = rect;
            loaded.plugin.set_editor_rect(rect);
        }
    }

    pub fn close_editor(&mut self, pad: usize) {
        if let Some(loaded) = self.loaded.get_mut(&pad) {
            loaded.plugin.close_editor();
        }
    }

    /// Save every loaded plugin's state into its `PluginRef`.
    pub fn capture_states(&mut self, clips: &mut [replicantsynth_core::model::Clip]) {
        for (pad, loaded) in self.loaded.iter_mut() {
            let Ok(state) = loaded.plugin.save_state() else {
                continue;
            };
            if state.is_empty() {
                continue;
            }
            if let Some(clip) = clips.get_mut(*pad) {
                if let Some(reference) = clip.plugin.as_mut() {
                    reference.state = Some(base64_encode(&state));
                }
            }
        }
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        for (_, mut loaded) in self.loaded.drain() {
            loaded.plugin.shut_down();
        }
    }
}

/// Colour used for plugin pads, shared with the pad grid and inspector.
pub fn plugin_accent() -> Color32 {
    theme::PLUGIN
}

/// Rebuild a descriptor from a saved project reference.
pub fn descriptor_from_reference(reference: &PluginRef) -> PluginDescriptor {
    PluginDescriptor {
        format: reference.format,
        path: std::path::PathBuf::from(&reference.path),
        id: reference.id.clone(),
        name: reference.name.clone(),
        vendor: reference.vendor.clone(),
        category: if reference.format == PluginFormat::Vst3 {
            "Plugin".to_string()
        } else {
            "Instrument".to_string()
        },
        is_instrument: true,
    }
}

// ---------------------------------------------------------------------------
// base64, so the plugin's opaque state survives a JSON project file
// ---------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[(triple >> 18) as usize & 0x3F] as char);
        out.push(B64[(triple >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            B64[(triple >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[triple as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut lookup = [255u8; 256];
    for (index, byte) in B64.iter().enumerate() {
        lookup[*byte as usize] = index as u8;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for character in text.bytes() {
        let value = lookup[character as usize];
        if value == 255 {
            continue;
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_length() {
        for length in 0..64 {
            let source: Vec<u8> = (0..length).map(|i| (i * 37 % 251) as u8).collect();
            let encoded = base64_encode(&source);
            let decoded = base64_decode(&encoded).expect("decode");
            assert_eq!(decoded, source, "length {length}");
        }
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
