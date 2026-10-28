//! Auxiliary windows: preset browser, FX picker, MIDI, takes, project, settings.

use super::theme;
use super::widgets::chip;
use crate::app::{output_devices, App, PresetBank};
use egui::{RichText, ScrollArea, Vec2};
use replicantsynth_core::dsp::{fx_catalog, fx_info, FX_COUNT};
use replicantsynth_core::model::{
    AcidPreset, AcidPresets, ClipKind, DrumPreset, DrumPresets, WavePreset,
};
use std::sync::OnceLock;

fn acid_all() -> &'static [AcidPreset] {
    static CACHE: OnceLock<Vec<AcidPreset>> = OnceLock::new();
    CACHE.get_or_init(AcidPresets::all)
}

fn drum_all() -> &'static [DrumPreset] {
    static CACHE: OnceLock<Vec<DrumPreset>> = OnceLock::new();
    CACHE.get_or_init(DrumPresets::all)
}

fn wave_all() -> &'static [WavePreset] {
    static CACHE: OnceLock<Vec<WavePreset>> = OnceLock::new();
    CACHE.get_or_init(|| replicantsynth_core::model::wave_presets().to_vec())
}

pub fn draw(ctx: &egui::Context, app: &mut App) {
    plugin_browser(ctx, app);
    presets(ctx, app);
    fx_picker(ctx, app);
    midi_sheet(ctx, app);
    recordings(ctx, app);
    project_sheet(ctx, app);
    settings(ctx, app);
    pad_menu(ctx, app);
    diagnostics(ctx, app);
}

/// Browse the VST3 / CLAP plugins installed on this machine.
fn plugin_browser(ctx: &egui::Context, app: &mut App) {
    if !app.windows.plugin_browser {
        return;
    }
    let mut open = true;
    let mut chosen: Option<usize> = None;
    egui::Window::new("PLUGINS · VST3 / CLAP")
        .open(&mut open)
        .default_size([620.0, 540.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.plugin_filter)
                        .hint_text("search a plugin…")
                        .desired_width(240.0),
                );
                if chip(ui, "RESCAN", app.plugins.scanning).clicked() {
                    app.plugins.start_scan(true);
                }
                if app.plugins.scanning {
                    ui.spinner();
                    ui.label(RichText::new("scanning…").size(10.0).color(theme::DIM));
                }
            });

            if let Some(error) = &app.plugins.scan_error {
                ui.label(RichText::new(error).size(10.0).color(theme::RED));
            }

            let needle = app.plugin_filter.to_lowercase();
            let total = app.plugins.available.len();
            let instruments: Vec<usize> = app
                .plugins
                .available
                .iter()
                .enumerate()
                .filter(|(_, plugin)| plugin.is_instrument)
                .filter(|(_, plugin)| {
                    needle.is_empty()
                        || plugin.name.to_lowercase().contains(&needle)
                        || plugin.vendor.to_lowercase().contains(&needle)
                })
                .map(|(index, _)| index)
                .collect();
            let effects: Vec<usize> = app
                .plugins
                .available
                .iter()
                .enumerate()
                .filter(|(_, plugin)| !plugin.is_instrument)
                .filter(|(_, plugin)| {
                    needle.is_empty()
                        || plugin.name.to_lowercase().contains(&needle)
                        || plugin.vendor.to_lowercase().contains(&needle)
                })
                .map(|(index, _)| index)
                .collect();

            if total == 0 && !app.plugins.scanning {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "No plugins found. Looked in the CLAP and VST3 folders for this platform; \
                         set REPLICANTSYNTH_PLUGIN_PATH to add a directory.",
                    )
                    .size(10.5)
                    .color(theme::DIM),
                );
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(400.0)
                .show(ui, |ui| {
                    for (title, list) in [("INSTRUMENTS", &instruments), ("EFFECTS", &effects)] {
                        if list.is_empty() {
                            continue;
                        }
                        ui.add_space(4.0);
                        ui.label(RichText::new(title).size(9.5).color(theme::DIM).strong());
                        for index in list {
                            let plugin = &app.plugins.available[*index];
                            let label = format!(
                                "{:<34} {:<5} {}",
                                truncate(&plugin.name, 34),
                                plugin.format.label(),
                                truncate(&plugin.vendor, 20)
                            );
                            if ui
                                .selectable_label(false, label)
                                .on_hover_text(plugin.path.display().to_string())
                                .clicked()
                            {
                                chosen = Some(*index);
                            }
                        }
                    }
                });

            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "{total} plugin(s) · pad {:02} · the editor opens after loading",
                    app.selected + 1
                ))
                .size(9.5)
                .color(theme::DIM),
            );
        });

    if let Some(index) = chosen {
        let descriptor = app.plugins.available[index].clone();
        let pad = app.selected;
        app.load_plugin(pad, &descriptor);
        open = false;
    }
    app.windows.plugin_browser = open;
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        text.chars()
            .take(limit.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

fn presets(ctx: &egui::Context, app: &mut App) {
    if !app.windows.presets {
        return;
    }
    let mut open = true;
    egui::Window::new("FACTORY PRESETS")
        .open(&mut open)
        .default_size([560.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("BANK").size(9.5).color(theme::DIM));
                let mut bank = app.preset_bank;
                for (kind, label) in [
                    (PresetBank::Acid, "303"),
                    (PresetBank::Drums, "909"),
                    (PresetBank::Wave, "WAVE"),
                ] {
                    if chip(ui, label, bank == kind).clicked() {
                        bank = kind;
                    }
                }
                app.preset_bank = bank;
                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(&mut app.preset_filter)
                        .hint_text("filter…")
                        .desired_width(150.0),
                );
            });
            ui.add_space(4.0);
            let needle = app.preset_filter.to_lowercase();
            let pad = app.selected;
            let kind = app.project.clips[pad].kind;

            if kind == ClipKind::Empty {
                ui.label(
                    RichText::new("Select a pad with an engine to apply a preset.")
                        .size(11.0)
                        .color(theme::DIM),
                );
            }

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(400.0)
                .show(ui, |ui| match app.preset_bank {
                    PresetBank::Acid => {
                        let mut apply: Option<usize> = None;
                        for (index, preset) in acid_all().iter().enumerate() {
                            let name = preset.display_name();
                            if !needle.is_empty() && !name.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if ui
                                .selectable_label(
                                    false,
                                    format!("{name:<34}  {} steps", preset.length),
                                )
                                .clicked()
                            {
                                apply = Some(index);
                            }
                        }
                        if let Some(index) = apply {
                            let preset = acid_all()[index].clone();
                            app.apply_acid_preset(&preset);
                        }
                    }
                    PresetBank::Drums => {
                        let mut apply: Option<usize> = None;
                        for (index, preset) in drum_all().iter().enumerate() {
                            let label = format!("{:<24}  {}", preset.name, preset.category);
                            if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if ui.selectable_label(false, label).clicked() {
                                apply = Some(index);
                            }
                        }
                        if let Some(index) = apply {
                            let preset = drum_all()[index].clone();
                            app.apply_drum_preset(&preset);
                        }
                    }
                    PresetBank::Wave => {
                        let mut apply: Option<usize> = None;
                        for (index, preset) in wave_all().iter().enumerate() {
                            let label = format!("{:<28}  {}", preset.name, preset.category);
                            if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if ui.selectable_label(false, label).clicked() {
                                apply = Some(index);
                            }
                        }
                        if let Some(index) = apply {
                            let preset = wave_all()[index].clone();
                            app.apply_wave_preset(&preset);
                        }
                    }
                });
            ui.add_space(4.0);
            ui.label(
                RichText::new(match app.preset_bank {
                    PresetBank::Acid => format!("{} acid lines · 8 banks x 64", AcidPresets::COUNT),
                    PresetBank::Drums => format!("{} classic 909 patterns", drum_all().len()),
                    PresetBank::Wave => format!("{} ReplicantWave sounds", wave_all().len()),
                })
                .size(9.5)
                .color(theme::DIM),
            );
        });
    app.windows.presets = open;
}

fn fx_picker(ctx: &egui::Context, app: &mut App) {
    if !app.windows.fx_picker {
        return;
    }
    let mut open = true;
    egui::Window::new("REPLICANT FX · 48 SLOTS")
        .open(&mut open)
        .default_size([520.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.fx_filter)
                        .hint_text("search a slot…")
                        .desired_width(200.0),
                );
                let current = app.project.clips[app.selected].fx_type;
                ui.label(
                    RichText::new(format!("current: {}", fx_info(current).name))
                        .size(10.0)
                        .color(theme::PINK),
                );
            });
            ui.add_space(4.0);
            let needle = app.fx_filter.to_lowercase();
            let pad = app.selected;
            let mut selected: Option<usize> = None;
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(400.0)
                .show(ui, |ui| {
                    for (index, info) in fx_catalog().iter().enumerate() {
                        let label = format!("{:02}  {:<3}  {}", index, info.code, info.name);
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let active = app.project.clips[pad].fx_type == index;
                        if ui.selectable_label(active, label).clicked() {
                            selected = Some(index);
                        }
                    }
                });
            if let Some(index) = selected {
                app.set_fx(pad, index);
                app.set_status(format!("FX slot {index}: {}", fx_info(index).name));
            }
            ui.label(
                RichText::new(format!("{FX_COUNT} slots · SP-404MKII v5.52 catalog"))
                    .size(9.5)
                    .color(theme::DIM),
            );
        });
    app.windows.fx_picker = open;
}

fn midi_sheet(ctx: &egui::Context, app: &mut App) {
    if !app.windows.midi {
        return;
    }
    let mut open = true;
    egui::Window::new("MIDI · APC64")
        .open(&mut open)
        .default_size([460.0, 420.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if chip(ui, "CONNECT APC64", false).clicked() {
                    match app.midi.connect(Some("APC64")) {
                        Ok(count) => app.set_status(format!("APC64: {count} ports open")),
                        Err(err) => app.set_status(format!("APC64: {err}")),
                    }
                }
                if chip(ui, "CONNECT ALL", false).clicked() {
                    match app.midi.connect(None) {
                        Ok(count) => app.set_status(format!("MIDI: {count} ports open")),
                        Err(err) => app.set_status(format!("MIDI: {err}")),
                    }
                }
                if chip(ui, "DISCONNECT", false).clicked() {
                    app.midi.disconnect();
                    app.set_status("MIDI disconnected");
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("status: {}", app.midi.status))
                    .size(10.5)
                    .color(if app.midi.is_connected() {
                        theme::GREEN
                    } else {
                        theme::DIM
                    }),
            );
            if let Some(error) = &app.midi.error {
                ui.label(RichText::new(error).size(10.0).color(theme::RED));
            }
            ui.separator();
            ui.label(RichText::new("PORTS").size(9.5).color(theme::DIM));
            for (index, port) in app.midi.ports.clone().iter().enumerate() {
                let role = crate::midi::port_role(port, index);
                ui.label(
                    RichText::new(format!("{index:02}  {port:<40} {:?}", role))
                        .size(10.0)
                        .monospace()
                        .color(theme::TEXT),
                );
            }
            if app.midi.ports.is_empty() {
                ui.label(
                    RichText::new("no MIDI inputs found — plug the APC64 in over USB")
                        .size(10.0)
                        .color(theme::DIM),
                );
            }
            ui.separator();
            ui.label(RichText::new("APC64").size(9.5).color(theme::DIM));
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("mode {}", app.midi.apc.mode.label()))
                        .size(10.5)
                        .color(theme::CYAN),
                );
                let firmware = app
                    .midi
                    .apc
                    .firmware
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                let supported =
                    crate::midi::firmware_supported(app.midi.apc.firmware.as_deref());
                ui.label(
                    RichText::new(format!(
                        "firmware {firmware} · validated {}",
                        crate::midi::VALIDATED_FIRMWARE
                    ))
                    .size(10.5)
                    .color(if supported { theme::GREEN } else { theme::AMBER }),
                );
                if let Some(strip) = app.midi.touched_strip {
                    ui.label(
                        RichText::new(format!("strip {}", strip + 1))
                            .size(10.0)
                            .color(theme::CYAN),
                    );
                }
            });
            ui.horizontal(|ui| {
                let base_switch = format!("BASE {}", crate::midi::PAD_BASE_CUSTOM);
                let mut base = app.midi.apc.base_note;
                if chip(ui, "BASE 0", base == 0).clicked() {
                    base = 0;
                }
                if chip(ui, &base_switch, base == crate::midi::PAD_BASE_CUSTOM).clicked() {
                    base = crate::midi::PAD_BASE_CUSTOM;
                }
                app.midi.apc.base_note = base;
                let mut flip = app.midi.apc.flip_rows;
                if chip(ui, "FLIP ROWS", flip).clicked() {
                    flip = !flip;
                    app.midi.apc.flip_rows = flip;
                }
            });
            ui.horizontal(|ui| {
                if chip(ui, "HANDSHAKE", false).clicked() {
                    let mut sent = 0;
                    for message in crate::midi::handshake() {
                        if app.midi.send(&message).is_ok() {
                            sent += 1;
                        }
                    }
                    app.set_status(format!("APC64 handshake: {sent} messages"));
                }
                if chip(ui, "IDENTIFY", false).clicked() {
                    let _ = app.midi.send(&crate::midi::identity_request());
                }
                if chip(ui, "CLEAR LEDS", false).clicked() {
                    app.midi.clear_pads();
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Pads launch clips, scene buttons fire rows, transport/record work, the encoder nudges BPM and the touch strips ride master / pad gain. LED and OLED feedback are pushed at 10 Hz.",
                )
                .size(10.0)
                .color(theme::DIM),
            );
            ui.separator();
            ui.label(RichText::new("MONITOR").size(9.5).color(theme::DIM));
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(96.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if app.midi.last_events.is_empty() {
                        ui.label(
                            RichText::new("no messages yet")
                                .size(10.0)
                                .color(theme::DIM),
                        );
                    }
                    for line in &app.midi.last_events {
                        ui.label(
                            RichText::new(line)
                                .size(9.5)
                                .monospace()
                                .color(theme::TEXT),
                        );
                    }
                });
        });
    app.windows.midi = open;
}

fn recordings(ctx: &egui::Context, app: &mut App) {
    if !app.windows.recordings {
        return;
    }
    let mut open = true;
    egui::Window::new("TAKES")
        .open(&mut open)
        .default_size([520.0, 420.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if chip(ui, "REFRESH", false).clicked() {
                    app.refresh_takes();
                }
                if chip(ui, "● MASTER REC", app.is_recording()).clicked() {
                    app.start_recording(None);
                }
                if chip(ui, "STEM → SELECTED", false).clicked() {
                    let pad = app.selected;
                    app.start_recording(Some(pad));
                }
            });
            ui.label(
                RichText::new(app.recordings_dir.display().to_string())
                    .size(9.5)
                    .color(theme::DIM),
            );
            ui.separator();
            let mut load: Option<std::path::PathBuf> = None;
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(320.0)
                .show(ui, |ui| {
                    if app.takes.is_empty() {
                        ui.label(
                            RichText::new("no takes yet — press REC or capture a stem")
                                .size(10.5)
                                .color(theme::DIM),
                        );
                    }
                    for take in &app.takes {
                        ui.horizontal(|ui| {
                            let seconds = take.frames as f32 / app.audio.sample_rate.max(1) as f32;
                            ui.label(
                                RichText::new(format!("{:<32} {:>6.1}s", take.name, seconds))
                                    .size(10.5)
                                    .monospace(),
                            );
                            if chip(ui, "LOAD → PAD", false).clicked() {
                                load = Some(take.path.clone());
                            }
                        });
                    }
                });
            if let Some(path) = load {
                let pad = app.selected;
                app.load_sample_file(pad, &path);
            }
            ui.label(
                RichText::new("24-bit stereo WAV, written by a dedicated thread.")
                    .size(9.5)
                    .color(theme::DIM),
            );
        });
    app.windows.recordings = open;
}

fn project_sheet(ctx: &egui::Context, app: &mut App) {
    if !app.windows.project {
        return;
    }
    let mut open = true;
    egui::Window::new("PROJECT")
        .open(&mut open)
        .default_size([420.0, 320.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("NAME").size(9.5).color(theme::DIM));
                if ui
                    .add(egui::TextEdit::singleline(&mut app.project.name).desired_width(220.0))
                    .changed()
                {
                    app.dirty = true;
                }
            });
            ui.label(
                RichText::new(app.project_path.display().to_string())
                    .size(9.5)
                    .color(theme::DIM),
            );
            ui.separator();
            ui.horizontal(|ui| {
                if chip(ui, "SAVE", false).clicked() {
                    app.save_project(false);
                }
                if chip(ui, "SAVE AS…", false).clicked() {
                    app.save_project(true);
                }
                if chip(ui, "OPEN…", false).clicked() {
                    app.open_project();
                }
            });
            ui.horizontal(|ui| {
                if chip(ui, "NEW", false).clicked() {
                    app.new_project();
                }
                if chip(ui, "LOAD DEMO", false).clicked() {
                    app.load_demo();
                }
            });
            ui.separator();
            ui.label(RichText::new("STATS").size(9.5).color(theme::DIM));
            let filled = app
                .project
                .clips
                .iter()
                .filter(|clip| clip.kind != ClipKind::Empty)
                .count();
            ui.label(
                RichText::new(format!(
                    "{filled} / 64 pads · {:.1} BPM · quantize {} · swing {:.0}%",
                    app.project.bpm,
                    replicantsynth_core::model::quantize_label(app.project.quantize),
                    app.project.swing * 100.0
                ))
                .size(10.5)
                .color(theme::TEXT),
            );
            ui.label(
                RichText::new(if app.dirty {
                    "unsaved changes"
                } else {
                    "saved"
                })
                .size(10.0)
                .color(if app.dirty {
                    theme::AMBER
                } else {
                    theme::GREEN
                }),
            );
        });
    app.windows.project = open;
}

fn settings(ctx: &egui::Context, app: &mut App) {
    if !app.windows.settings {
        return;
    }
    let mut open = true;
    let mut restart = false;
    egui::Window::new("SETTINGS")
        .open(&mut open)
        .default_size([480.0, 460.0])
        .show(ctx, |ui| {
            ui.label(RichText::new("AUDIO OUTPUT").size(9.5).color(theme::DIM));
            let devices = output_devices();
            let current = app
                .device_choice
                .clone()
                .or_else(|| Some(app.audio.device_name.clone()))
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("device")
                .selected_text(current.clone())
                .width(300.0)
                .show_ui(ui, |ui| {
                    for device in &devices {
                        if ui.selectable_label(current == *device, device).clicked() {
                            app.device_choice = Some(device.clone());
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.label(RichText::new("RATE").size(9.5).color(theme::DIM));
                for rate in [44_100u32, 48_000, 96_000] {
                    let active = app.device_rate.unwrap_or(app.audio.sample_rate) == rate;
                    if chip(ui, &format!("{}", rate / 1000), active).clicked() {
                        app.device_rate = Some(rate);
                    }
                }
            });
            if chip(ui, "RESTART AUDIO", false).clicked() {
                restart = true;
            }
            ui.label(
                RichText::new(format!(
                    "{} · {} ch · {:.1} kHz · {}",
                    app.audio.device_name,
                    app.audio.channels,
                    app.audio.sample_rate as f32 / 1000.0,
                    app.audio.status
                ))
                .size(10.0)
                .color(theme::DIM),
            );
            ui.separator();
            ui.label(RichText::new("SHORTCUTS").size(9.5).color(theme::DIM));
            for line in [
                "Space — play / stop",
                "R — capture the master",
                "Arrows — move the pad selection",
                "Delete — clear the selected pad",
                "Ctrl/Cmd+S — save · Ctrl/Cmd+O — open · Ctrl/Cmd+N — new",
                "Double-click a pad — clip editor",
                "Right-click a pad — pad menu",
            ] {
                ui.label(RichText::new(line).size(10.0).color(theme::TEXT));
            }
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "ReplicantSynth Desktop v{} · MIT · engine {}",
                    env!("CARGO_PKG_VERSION"),
                    replicantsynth_core::VERSION
                ))
                .size(10.0)
                .color(theme::DIM),
            );
        });
    app.windows.settings = open;
    if restart {
        app.restart_audio();
    }
}

fn pad_menu(ctx: &egui::Context, app: &mut App) {
    let Some(pad) = app.windows.pad_menu else {
        return;
    };
    let mut open = true;
    let mut close = false;
    egui::Window::new(format!("PAD {:02}", pad + 1))
        .open(&mut open)
        .default_size([240.0, 260.0])
        .show(ctx, |ui| {
            let name = app.project.clips[pad].name.clone();
            ui.label(RichText::new(name).size(11.0).color(theme::TEXT));
            ui.add_space(4.0);
            ui.label(RichText::new("SET ENGINE").size(9.5).color(theme::DIM));
            for kind in ClipKind::ALL {
                if chip(ui, kind.label(), app.project.clips[pad].kind == kind).clicked() {
                    app.set_kind(pad, kind);
                    close = true;
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if chip(ui, "LAUNCH", false).clicked() {
                    app.launch(pad);
                    close = true;
                }
                if chip(ui, "STOP", false).clicked() {
                    app.stop(pad);
                    close = true;
                }
            });
            ui.horizontal(|ui| {
                if chip(ui, "EDIT", false).clicked() {
                    app.selected = pad;
                    app.windows.editor = true;
                    close = true;
                }
                if chip(ui, "CLEAR", false).clicked() {
                    app.handles.clear_clip(pad);
                    app.set_kind(pad, ClipKind::Empty);
                    close = true;
                }
            });
        });
    if !open || close {
        app.windows.pad_menu = None;
    }
}

fn diagnostics(ctx: &egui::Context, app: &mut App) {
    if !app.windows.diagnostics {
        return;
    }
    let mut open = true;
    egui::Window::new("DIAGNOSTICS")
        .open(&mut open)
        .default_size([420.0, 300.0])
        .show(ctx, |ui| {
            let telemetry = app.handles.telemetry();
            ui.label(
                RichText::new(format!(
                    "master frames {} · stem frames {} · dropped {}",
                    telemetry
                        .master_frames
                        .load(std::sync::atomic::Ordering::Relaxed),
                    telemetry
                        .pad_frames
                        .load(std::sync::atomic::Ordering::Relaxed),
                    telemetry
                        .dropped_frames
                        .load(std::sync::atomic::Ordering::Relaxed)
                ))
                .size(10.0)
                .color(theme::TEXT),
            );
            ui.label(
                RichText::new(format!(
                    "step {} · master peak {:.3}",
                    app.snapshot.absolute_step, app.snapshot.master_meter
                ))
                .size(10.0)
                .color(theme::DIM),
            );
            let mut scope = [0.0f32; 256];
            telemetry.read_scope(&mut scope);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(380.0, 110.0), egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, egui::CornerRadius::same(4), theme::BG);
            let mut points = Vec::with_capacity(scope.len());
            for (i, value) in scope.iter().enumerate() {
                let x = rect.left() + rect.width() * i as f32 / scope.len() as f32;
                let y = rect.center().y - value.clamp(-1.0, 1.0) * rect.height() * 0.45;
                points.push(egui::pos2(x, y));
            }
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(1.2_f32, theme::CYAN),
            ));
        });
    app.windows.diagnostics = open;
}
