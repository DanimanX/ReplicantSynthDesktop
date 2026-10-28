//! Plugin discovery: find the CLAP and VST3 plugins installed on this machine.
//!
//! VST3 bundles are read without executing any plugin code — the bundle's
//! `moduleinfo.json` already lists every class, its name, vendor and
//! sub-categories. CLAP has no such manifest, so listing a `.clap` library means
//! loading it and reading its descriptor, which is what every CLAP host does.
//! That loading happens on the scanner thread, never on the audio thread.

use replicantsynth_core::model::PluginFormat;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginDescriptor {
    pub format: PluginFormat,
    /// The `.clap` file, or the `.vst3` bundle directory.
    pub path: PathBuf,
    /// The CLAP plugin id, or the VST3 class id as uppercase hex.
    pub id: String,
    pub name: String,
    pub vendor: String,
    /// `Instrument`, `Effect`, `Note effect` … as reported by the format.
    pub category: String,
    pub is_instrument: bool,
}

impl PluginDescriptor {
    pub fn key(&self) -> String {
        format!("{}:{}", self.format.label(), self.id)
    }

    pub fn label(&self) -> String {
        if self.vendor.trim().is_empty() {
            self.name.clone()
        } else {
            format!("{} · {}", self.name, self.vendor)
        }
    }
}

/// The directories searched on this platform, most specific first.
pub fn default_scan_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(extra) = std::env::var("REPLICANTSYNTH_PLUGIN_PATH") {
        for part in extra.split(if cfg!(windows) { ';' } else { ':' }) {
            if !part.trim().is_empty() {
                dirs.push(PathBuf::from(part));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/lib/clap"));
        dirs.push(PathBuf::from("/usr/local/lib/clap"));
        dirs.push(PathBuf::from("/usr/lib/vst3"));
        dirs.push(PathBuf::from("/usr/local/lib/vst3"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(Path::new(&home).join(".clap"));
            dirs.push(Path::new(&home).join(".vst3"));
        }
        // Flatpak and the common "system wide user" locations.
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/lib/clap"));
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/lib/vst3"));
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        dirs.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(Path::new(&home).join("Library/Audio/Plug-Ins/CLAP"));
            dirs.push(Path::new(&home).join("Library/Audio/Plug-Ins/VST3"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        dirs.push(PathBuf::from("C:/Program Files/Common Files/CLAP"));
        dirs.push(PathBuf::from("C:/Program Files/Common Files/VST3"));
        if let Ok(program_files) = std::env::var("PROGRAMFILES") {
            dirs.push(Path::new(&program_files).join("Common Files/CLAP"));
            dirs.push(Path::new(&program_files).join("Common Files/VST3"));
        }
    }

    dirs.retain(|dir| dir.exists());
    dirs
}

/// Directories for a single format, for a scanner that wants to run per-format.
pub fn dirs_for(format: PluginFormat) -> Vec<PathBuf> {
    default_scan_dirs()
        .into_iter()
        .filter(|dir| match format {
            PluginFormat::Clap => dir.to_string_lossy().to_ascii_lowercase().contains("clap"),
            PluginFormat::Vst3 => dir.to_string_lossy().to_ascii_lowercase().contains("vst3"),
        })
        .collect()
}

/// Every plugin found under `dirs`, sorted by name.
///
/// A bundle that fails to load is skipped; scanning never fails the whole run.
pub fn scan(dirs: &[PathBuf]) -> Vec<PluginDescriptor> {
    let mut found: Vec<PluginDescriptor> = Vec::new();

    let clap_paths = collect(dirs, |path| {
        path.extension().and_then(|ext| ext.to_str()) == Some("clap")
    });
    for path in clap_paths {
        match crate::clap_backend::list(&path) {
            Ok(mut descriptors) => found.append(&mut descriptors),
            Err(error) => eprintln!("clap scan: {}: {error}", path.display()),
        }
    }

    let vst3_paths = collect(dirs, |path| {
        path.extension().and_then(|ext| ext.to_str()) == Some("vst3") && path.is_dir()
    });
    for path in vst3_paths {
        match crate::vst3_backend::list(&path) {
            Ok(descriptors) => {
                // A bundle with no readable manifest cannot be described without
                // loading it, and loading a yabridge bundle starts Wine. Offer
                // only what we could inspect from the filesystem; such a plugin
                // is still loadable when the user points at it directly.
                if descriptors.iter().all(|entry| entry.id.is_empty()) {
                    eprintln!(
                        "vst3 scan: {} has no moduleinfo.json, not listing it",
                        path.display()
                    );
                    continue;
                }
                found.extend(descriptors.into_iter().filter(|entry| !entry.id.is_empty()));
            }
            Err(error) => eprintln!("vst3 scan: {}: {error}", path.display()),
        }
    }

    found.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.format.label().cmp(b.format.label()))
    });
    found.dedup_by(|a, b| a.path == b.path && a.id == b.id);
    found
}

/// Depth-limited walk that returns every path matching `predicate`.
fn collect(dirs: &[PathBuf], predicate: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 3;
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = dirs.iter().map(|dir| (dir.clone(), 0)).collect();

    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if predicate(&path) {
                out.push(path);
                continue;
            }
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            // `.vst3` bundles are directories we already matched above; don't
            // descend into them looking for more plugins.
            let is_bundle = path.extension().and_then(|ext| ext.to_str()) == Some("vst3");
            if is_dir && !is_bundle && depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dirs_only_lists_existing_paths() {
        for dir in default_scan_dirs() {
            assert!(dir.exists(), "{} does not exist", dir.display());
        }
    }

    #[test]
    fn scanning_the_system_finds_plugins() {
        let dirs = default_scan_dirs();
        if dirs.is_empty() {
            println!("no plugin directories on this machine; skipping");
            return;
        }
        let found = scan(&dirs);
        for descriptor in &found {
            assert!(!descriptor.name.is_empty(), "{descriptor:?}");
            assert!(!descriptor.id.is_empty(), "{descriptor:?}");
            assert!(descriptor.path.exists(), "{descriptor:?}");
        }
        println!("found {} plugin(s)", found.len());
        for descriptor in found.iter().take(12) {
            println!(
                "  {:<5} {:<28} {}",
                descriptor.format.label(),
                descriptor.name,
                descriptor.category
            );
        }
    }

    #[test]
    fn a_missing_directory_is_not_an_error() {
        let found = scan(&[PathBuf::from("/nonexistent/plugins")]);
        assert!(found.is_empty());
    }
}
