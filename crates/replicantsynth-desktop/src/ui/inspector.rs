//! The right-hand inspector: SOUND / FX / PATTERN / MIX for the selected pad.

use super::theme;
use super::widgets::{self, chip, knob, section_label, Unit};
use crate::app::{App, InspectorTab};
use egui::{Color32, RichText, Stroke, Ui, Vec2};
use replicantsynth_core::dsp::{fx_info, FX_COUNT};
use replicantsynth_core::model::{
    AcidParam, ClipKind, DrumParam, FxParam, SampleParam, WaveParam, CHORD_MODES, DISTORTION_TYPES,
    DRUM_VOICE_NAMES, FILTER_MODES, WAVETABLE_BANKS,
};
use replicantsynth_core::ClipState;

pub fn draw(ui: &mut Ui, app: &mut App) {
    let pad = app.selected;
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(11))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            header(ui, app, pad);
            ui.add_space(6.0);
            tabs(ui, app);
            ui.add_space(4.0);
            let height = ui.available_height() - 4.0;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(height)
                .show(ui, |ui| match app.tab {
                    InspectorTab::Sound => sound(ui, app, pad),
                    InspectorTab::Fx => fx(ui, app, pad),
                    InspectorTab::Pattern => pattern(ui, app, pad),
                    InspectorTab::Mix => mix(ui, app, pad),
                });
        });
}

fn header(ui: &mut Ui, app: &mut App, pad: usize) {
    let clip = &app.project.clips[pad];
    let color = theme::kind_color(clip.kind);
    let telemetry = app.snapshot.pads[pad];

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(format!("PAD {:02}", pad + 1))
                    .size(10.0)
                    .color(theme::DIM),
            );
            ui.label(RichText::new(clip.kind.label()).size(15.0).strong().color(
                if clip.kind == ClipKind::Empty {
                    theme::DIM
                } else {
                    color
                },
            ));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let state = telemetry.state;
            let (label, state_color) = match state {
                ClipState::Playing => ("PLAYING", color),
                ClipState::QueuedStart => ("QUEUED", theme::AMBER),
                ClipState::QueuedStop => ("STOPPING", theme::RED),
                ClipState::Stopped => ("STOPPED", theme::DIM),
            };
            ui.label(RichText::new(label).size(10.0).color(state_color));
            if clip.solo {
                ui.label(RichText::new("SOLO").size(9.0).color(theme::AMBER));
            }
            if clip.mute {
                ui.label(RichText::new("MUTE").size(9.0).color(theme::RED));
            }
        });
    });

    ui.add_space(4.0);
    {
        let clip = &mut app.project.clips[pad];
        let response = ui.add(
            egui::TextEdit::singleline(&mut clip.name)
                .hint_text("clip name")
                .desired_width(ui.available_width())
                .font(egui::TextStyle::Body),
        );
        if response.changed() {
            app.dirty = true;
        }
    }

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for kind in ClipKind::ALL {
            let active = app.project.clips[pad].kind == kind;
            if chip(ui, kind.short(), active).clicked() && !active {
                app.set_kind(pad, kind);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if chip(ui, "BROWSE", false).clicked() {
                app.windows.presets = true;
            }
        });
    });
}

fn tabs(ui: &mut Ui, app: &mut App) {
    ui.horizontal(|ui| {
        for tab in InspectorTab::ALL {
            if chip(ui, tab.label(), app.tab == tab).clicked() {
                app.tab = tab;
            }
        }
    });
}

#[allow(clippy::needless_range_loop)]
fn sound(ui: &mut Ui, app: &mut App, pad: usize) {
    let kind = app.project.clips[pad].kind;
    let color = theme::kind_color(kind);
    let mut changed = false;

    match kind {
        ClipKind::Empty => {
            ui.add_space(12.0);
            ui.label(
                RichText::new("Empty slot. Pick an engine above, or double-click a pad to edit.")
                    .size(11.0)
                    .color(theme::DIM),
            );
        }
        ClipKind::Acid => {
            section_label(ui, "ACID 303");
            let clip = &mut app.project.clips[pad];
            ui.horizontal_wrapped(|ui| {
                let saw = clip.params[AcidParam::WAVE] < 0.5;
                if chip(ui, "SAW", saw).clicked() {
                    clip.params[AcidParam::WAVE] = 0.0;
                    changed = true;
                }
                if chip(ui, "SQUARE", !saw).clicked() {
                    clip.params[AcidParam::WAVE] = 1.0;
                    changed = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                changed |= knob(
                    ui,
                    "CUTOFF",
                    &mut clip.params[AcidParam::CUTOFF],
                    25.0,
                    2600.0,
                    Unit::Hertz,
                    Some(420.0),
                    color,
                );
                changed |= knob(
                    ui,
                    "RESONANCE",
                    &mut clip.params[AcidParam::RESONANCE],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.72),
                    color,
                );
                changed |= knob(
                    ui,
                    "ENV MOD",
                    &mut clip.params[AcidParam::ENV_MOD],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.68),
                    color,
                );
                changed |= knob(
                    ui,
                    "DECAY",
                    &mut clip.params[AcidParam::DECAY],
                    0.15,
                    2.8,
                    Unit::Seconds,
                    Some(0.55),
                    color,
                );
                changed |= knob(
                    ui,
                    "ACCENT",
                    &mut clip.params[AcidParam::ACCENT],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.82),
                    color,
                );
                changed |= knob(
                    ui,
                    "VOLUME",
                    &mut clip.params[AcidParam::VOLUME],
                    0.0,
                    1.2,
                    Unit::Percent,
                    Some(0.9),
                    color,
                );
            });

            section_label(ui, "DISTORTION");
            ui.horizontal_wrapped(|ui| {
                let index = clip.params[AcidParam::DIST_TYPE] as usize;
                egui::ComboBox::from_id_salt(("dist", pad))
                    .selected_text(DISTORTION_TYPES[index.min(4)])
                    .width(118.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in DISTORTION_TYPES.iter().enumerate() {
                            if ui.selectable_label(index == i, *name).clicked() {
                                clip.params[AcidParam::DIST_TYPE] = i as f32;
                                changed = true;
                            }
                        }
                    });
                changed |= knob(
                    ui,
                    "DRIVE",
                    &mut clip.params[AcidParam::DIST_DRIVE],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.55),
                    color,
                );
                changed |= knob(
                    ui,
                    "MIX",
                    &mut clip.params[AcidParam::DIST_MIX],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.55),
                    color,
                );
                changed |= knob(
                    ui,
                    "TONE",
                    &mut clip.params[AcidParam::DIST_TONE],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.62),
                    color,
                );
            });
        }
        ClipKind::Wave => {
            section_label(ui, "REPLICANT WAVE");
            let clip = &mut app.project.clips[pad];
            let bank = (clip.params[WaveParam::BANK] as usize).min(7);
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("bank", pad))
                    .selected_text(WAVETABLE_BANKS[bank])
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in WAVETABLE_BANKS.iter().enumerate() {
                            if ui
                                .selectable_label(bank == i, format!("{i}  {name}"))
                                .clicked()
                            {
                                clip.params[WaveParam::BANK] = i as f32;
                                changed = true;
                            }
                        }
                    });
                let mode = (clip.params[WaveParam::FILTER_MODE] as usize).min(3);
                egui::ComboBox::from_id_salt(("filter", pad))
                    .selected_text(FILTER_MODES[mode])
                    .width(84.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in FILTER_MODES.iter().enumerate() {
                            if ui.selectable_label(mode == i, *name).clicked() {
                                clip.params[WaveParam::FILTER_MODE] = i as f32;
                                changed = true;
                            }
                        }
                    });
                let chord = (clip.params[WaveParam::CHORD] as usize).min(4);
                egui::ComboBox::from_id_salt(("chord", pad))
                    .selected_text(CHORD_MODES[chord])
                    .width(88.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in CHORD_MODES.iter().enumerate() {
                            if ui.selectable_label(chord == i, *name).clicked() {
                                clip.params[WaveParam::CHORD] = i as f32;
                                changed = true;
                            }
                        }
                    });
            });

            ui.horizontal_wrapped(|ui| {
                changed |= knob(
                    ui,
                    "POSITION",
                    &mut clip.params[WaveParam::POSITION],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.3),
                    color,
                );
                changed |= knob(
                    ui,
                    "UNISON",
                    &mut clip.params[WaveParam::UNISON],
                    1.0,
                    8.0,
                    Unit::Raw,
                    Some(3.0),
                    color,
                );
                changed |= knob(
                    ui,
                    "DETUNE",
                    &mut clip.params[WaveParam::DETUNE],
                    0.0,
                    80.0,
                    Unit::Cents,
                    Some(16.0),
                    color,
                );
                changed |= knob(
                    ui,
                    "SPREAD",
                    &mut clip.params[WaveParam::SPREAD],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.5),
                    color,
                );
                changed |= knob(
                    ui,
                    "CUTOFF",
                    &mut clip.params[WaveParam::CUTOFF],
                    40.0,
                    16000.0,
                    Unit::Hertz,
                    Some(2200.0),
                    color,
                );
                changed |= knob(
                    ui,
                    "RESO",
                    &mut clip.params[WaveParam::RESONANCE],
                    0.5,
                    18.0,
                    Unit::Raw,
                    Some(0.7),
                    color,
                );
                changed |= knob(
                    ui,
                    "DRIVE",
                    &mut clip.params[WaveParam::DRIVE],
                    1.0,
                    6.0,
                    Unit::Raw,
                    Some(1.2),
                    color,
                );
                changed |= knob(
                    ui,
                    "LEVEL",
                    &mut clip.params[WaveParam::LEVEL],
                    0.0,
                    1.2,
                    Unit::Percent,
                    Some(0.8),
                    color,
                );
            });

            section_label(ui, "AMP ENVELOPE");
            ui.horizontal_wrapped(|ui| {
                changed |= knob(
                    ui,
                    "ATTACK",
                    &mut clip.params[WaveParam::ATTACK],
                    0.001,
                    4.0,
                    Unit::Seconds,
                    Some(0.01),
                    color,
                );
                changed |= knob(
                    ui,
                    "DECAY",
                    &mut clip.params[WaveParam::DECAY],
                    0.005,
                    4.0,
                    Unit::Seconds,
                    Some(0.4),
                    color,
                );
                changed |= knob(
                    ui,
                    "SUSTAIN",
                    &mut clip.params[WaveParam::SUSTAIN],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.6),
                    color,
                );
                changed |= knob(
                    ui,
                    "RELEASE",
                    &mut clip.params[WaveParam::RELEASE],
                    0.005,
                    4.0,
                    Unit::Seconds,
                    Some(0.5),
                    color,
                );
                changed |= knob(
                    ui,
                    "SUB",
                    &mut clip.params[WaveParam::SUB],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.2),
                    color,
                );
                changed |= knob(
                    ui,
                    "NOISE",
                    &mut clip.params[WaveParam::NOISE],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(0.0),
                    color,
                );
            });
        }
        ClipKind::Drums => {
            section_label(ui, "REPLICANT 909");
            let clip = &mut app.project.clips[pad];
            ui.horizontal_wrapped(|ui| {
                for (label, slot, min, max, default) in [
                    ("TUNE", DrumParam::TUNE, 0.0, 1.0, 0.5),
                    ("DECAY", DrumParam::DECAY, 0.0, 1.0, 0.5),
                    ("SNAPPY", DrumParam::SNAPPY, 0.0, 1.0, 0.5),
                    ("TONE", DrumParam::TONE, 0.0, 1.0, 0.5),
                    ("ACCENT", DrumParam::ACCENT, 0.0, 1.0, 0.7),
                    ("LEVEL", DrumParam::LEVEL, 0.0, 1.2, 0.9),
                ] {
                    changed |= knob(
                        ui,
                        label,
                        &mut clip.params[slot],
                        min,
                        max,
                        Unit::Percent,
                        Some(default),
                        color,
                    );
                }
                changed |= knob(
                    ui,
                    "FILTER",
                    &mut clip.params[DrumParam::CUTOFF],
                    200.0,
                    20000.0,
                    Unit::Hertz,
                    Some(18000.0),
                    color,
                );
                changed |= knob(
                    ui,
                    "RESO",
                    &mut clip.params[DrumParam::RESONANCE],
                    0.5,
                    12.0,
                    Unit::Raw,
                    Some(0.6),
                    color,
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("BD · SD · LT · MT · HT · RS · CP · CH · OH · CC · RC")
                    .size(9.5)
                    .color(theme::DIM),
            );
            ui.horizontal_wrapped(|ui| {
                for (voice, name) in DRUM_VOICE_NAMES.iter().enumerate() {
                    if chip(ui, name, false)
                        .on_hover_text(format!("Audition {name}"))
                        .clicked()
                    {
                        let note = replicantsynth_core::dsp::drums::instrument_default_note(voice);
                        app.handles.preview_note(pad, true, note, 110);
                        app.handles.preview_note(pad, false, note, 0);
                    }
                }
            });
        }
        ClipKind::Sample => {
            section_label(ui, "SAMPLE");
            let mut import = false;
            let mut stem = false;
            let clip = &mut app.project.clips[pad];
            ui.label(
                RichText::new(
                    clip.sample_name
                        .clone()
                        .unwrap_or_else(|| "no sample loaded".to_string()),
                )
                .size(11.0)
                .color(if clip.sample_missing {
                    theme::RED
                } else {
                    theme::TEXT
                }),
            );
            ui.horizontal(|ui| {
                if chip(ui, "IMPORT…", false).clicked() {
                    import = true;
                }
                let mut looping = clip.sample_loop;
                if chip(ui, "LOOP", looping).clicked() {
                    looping = !looping;
                    clip.sample_loop = looping;
                    changed = true;
                }
                if chip(ui, "STEM REC", false).clicked() {
                    stem = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                changed |= knob(
                    ui,
                    "GAIN",
                    &mut clip.params[SampleParam::GAIN],
                    0.0,
                    2.0,
                    Unit::Percent,
                    Some(1.0),
                    color,
                );
                changed |= knob(
                    ui,
                    "PITCH",
                    &mut clip.params[SampleParam::PITCH],
                    -24.0,
                    24.0,
                    Unit::Semitones,
                    Some(0.0),
                    color,
                );
            });
            if import {
                app.import_sample(pad);
            }
            if stem {
                app.start_recording(Some(pad));
            }
        }
    }

    if changed {
        app.params_only(pad);
    }
}

fn fx(ui: &mut Ui, app: &mut App, pad: usize) {
    let mut changed = false;
    {
        let clip = &mut app.project.clips[pad];
        let info = fx_info(clip.fx_type);
        section_label(ui, "REPLICANT FX INSERT");
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(info.name).size(13.0).strong());
                ui.label(
                    RichText::new(format!("{} · SLOT {:02}", info.code, clip.fx_type))
                        .size(9.5)
                        .color(theme::DIM),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if chip(ui, "CHANGE", false).clicked() {
                    app.windows.fx_picker = true;
                }
            });
        });
        ui.horizontal(|ui| {
            let mut enabled = clip.fx_enabled;
            if chip(ui, "ENABLED", enabled).clicked() {
                enabled = !enabled;
                clip.fx_enabled = enabled;
                clip.params[FxParam::ENABLED] = if enabled { 1.0 } else { 0.0 };
                changed = true;
            }
            if chip(ui, "RESET", false).clicked() {
                let slot = clip.fx_type;
                clip.apply_fx_defaults(slot);
                changed = true;
            }
        });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for i in 0..6 {
                let label = info.labels[i];
                let default = info.defaults[i];
                let slot = FxParam::BASE + i;
                changed |= knob(
                    ui,
                    label,
                    &mut clip.params[slot],
                    0.0,
                    1.0,
                    Unit::Percent,
                    Some(default),
                    theme::PINK,
                );
            }
        });

        if clip.fx_type == 0 {
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Bypass is selected. Pick any of the 48 SP-404MKII slots to insert it per pad.",
                )
                .size(10.5)
                .color(theme::DIM),
            );
        }
        let code = info.code;
        let slot_count = FX_COUNT;
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{slot_count} slots · current {code}"))
                .size(9.5)
                .color(theme::DIM),
        );
    }
    if changed {
        app.params_only(pad);
    }
}

fn pattern(ui: &mut Ui, app: &mut App, pad: usize) {
    let kind = app.project.clips[pad].kind;
    let color = theme::kind_color(kind);
    ui.horizontal(|ui| {
        if chip(ui, "OPEN EDITOR", false).clicked() {
            app.windows.editor = true;
        }
        ui.label(
            RichText::new(match kind {
                ClipKind::Wave => "16-step piano roll",
                ClipKind::Drums => "11 x 16 kit grid",
                ClipKind::Acid => "16-step acid line",
                _ => "no pattern",
            })
            .size(10.0)
            .color(theme::DIM),
        );
    });
    ui.add_space(4.0);
    let mut changed = false;
    {
        let clip = &mut app.project.clips[pad];
        ui.horizontal(|ui| {
            ui.label(RichText::new("ROOT").size(9.5).color(theme::DIM));
            let mut root = clip.root;
            if ui
                .add(egui::DragValue::new(&mut root).speed(0.5).range(0..=127))
                .changed()
            {
                clip.root = root;
                changed = true;
            }
            ui.label(RichText::new("LENGTH").size(9.5).color(theme::DIM));
            let mut length = clip.length;
            if ui
                .add(egui::DragValue::new(&mut length).speed(0.2).range(1..=16))
                .changed()
            {
                clip.length = length;
                changed = true;
            }
        });
    }

    match kind {
        ClipKind::Acid => {
            ui.add_space(6.0);
            section_label(ui, "GATES");
            let clip = &mut app.project.clips[pad];
            ui.horizontal(|ui| {
                for step in 0..16 {
                    let mut data = clip.steps[step];
                    let label = if data.gate {
                        format!("{}", data.note)
                    } else {
                        "·".to_string()
                    };
                    if chip(ui, &label, data.gate).clicked() {
                        data.gate = !data.gate;
                        clip.steps[step] = data;
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                for step in 0..16 {
                    let data = clip.steps[step];
                    let mut accent = data.accent;
                    if chip(ui, "ACC", accent).clicked() {
                        accent = !accent;
                        let mut data = clip.steps[step];
                        data.accent = accent;
                        clip.steps[step] = data;
                        changed = true;
                    }
                }
            });
        }
        ClipKind::Drums => {
            ui.add_space(6.0);
            section_label(ui, "KIT");
            let clip = &mut app.project.clips[pad];
            #[allow(clippy::needless_range_loop)]
            for voice in 0..DRUM_VOICE_NAMES.len() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(DRUM_VOICE_NAMES[voice])
                            .size(9.5)
                            .monospace()
                            .color(theme::DIM),
                    );
                    for step in 0..16 {
                        let on = clip.drum_on(voice, step);
                        let response =
                            ui.allocate_response(Vec2::new(13.0, 13.0), egui::Sense::click());
                        let rect = response.rect;
                        let painter = ui.painter();
                        painter.rect_filled(
                            rect,
                            egui::CornerRadius::same(2),
                            if on {
                                color
                            } else {
                                Color32::from_rgb(24, 31, 43)
                            },
                        );
                        if response.clicked() {
                            clip.set_drum_step(voice, step, !on);
                            if !on {
                                clip.set_drum_velocity(voice, step, 110);
                            }
                            changed = true;
                        }
                    }
                });
            }
        }
        ClipKind::Wave => {
            ui.add_space(6.0);
            section_label(ui, "NOTES");
            let clip = &mut app.project.clips[pad];
            let notes = clip.midi_notes.len();
            ui.label(
                RichText::new(format!("{notes} MIDI note(s) in the piano roll"))
                    .size(10.5)
                    .color(theme::DIM),
            );
            let mut remove: Option<(i32, i32)> = None;
            ui.horizontal_wrapped(|ui| {
                for note in clip.midi_notes.iter().take(24) {
                    let label = format!("{}@{}", note.note, note.start + 1);
                    if chip(ui, &label, true).clicked() {
                        remove = Some((note.note as i32, note.start as i32));
                    }
                }
            });
            if let Some((note, start)) = remove {
                clip.remove_midi_note(note, start);
                changed = true;
            }
        }
        _ => {}
    }

    if changed {
        app.sync_pad(pad);
    }
}

fn mix(ui: &mut Ui, app: &mut App, pad: usize) {
    let mut changed = false;
    let color = theme::kind_color(app.project.clips[pad].kind);
    let meter_value = app.snapshot.meters[pad];
    {
        let clip = &mut app.project.clips[pad];
        section_label(ui, "CHANNEL STRIP");
        ui.horizontal(|ui| {
            ui.label(RichText::new("LEVEL").size(9.5).color(theme::DIM));
            widgets::meter(ui, meter_value, 180.0, 7.0);
            ui.label(
                RichText::new(format!("{:.0}%", meter_value * 100.0))
                    .size(9.5)
                    .monospace()
                    .color(theme::DIM),
            );
        });
        ui.horizontal_wrapped(|ui| {
            changed |= knob(
                ui,
                "GAIN",
                &mut clip.gain,
                0.0,
                1.5,
                Unit::Percent,
                Some(0.8),
                color,
            );
            changed |= knob(
                ui,
                "PAN",
                &mut clip.pan,
                -1.0,
                1.0,
                Unit::Raw,
                Some(0.0),
                color,
            );
        });
        ui.horizontal(|ui| {
            let mut mute = clip.mute;
            if chip(ui, "MUTE", mute).clicked() {
                mute = !mute;
                clip.mute = mute;
                changed = true;
            }
            let mut solo = clip.solo;
            if chip(ui, "SOLO", solo).clicked() {
                solo = !solo;
                clip.solo = solo;
                changed = true;
            }
        });

        widgets::divider(ui);
        section_label(ui, "PAD");
        ui.label(
            RichText::new(format!("{} · pad {:02}", clip.kind.label(), clip.index + 1))
                .size(10.5)
                .color(theme::DIM),
        );
        ui.label(
            RichText::new(format!("insert: {}", fx_info(clip.fx_type).name))
                .size(10.0)
                .color(theme::DIM),
        );
    }

    if changed {
        app.params_only(pad);
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if chip(ui, "STEM REC", false).clicked() {
            app.start_recording(Some(pad));
        }
        if chip(ui, "LAUNCH", false).clicked() {
            app.launch(pad);
        }
        if chip(ui, "CLEAR PAD", false).clicked() {
            app.handles.clear_clip(pad);
            app.set_kind(pad, ClipKind::Empty);
        }
    });
}
