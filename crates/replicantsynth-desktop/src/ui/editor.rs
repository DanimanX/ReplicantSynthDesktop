//! Clip editors: the 303 step sequencer, the 909 kit grid and the Wave piano roll.

use super::theme;
use super::widgets::chip;
use crate::app::App;
use egui::{Color32, CornerRadius, FontId, RichText, Sense, Stroke, Ui, Vec2};
use replicantsynth_core::dsp::fx_info;
use replicantsynth_core::model::{ClipKind, Step, WaveParam, DRUM_VOICE_NAMES, WAVETABLE_BANKS};

pub fn draw(ctx: &egui::Context, app: &mut App) {
    let mut open = app.windows.editor;
    let pad = app.selected;
    let kind = app.project.clips[pad].kind;
    let title = format!("CLIP EDITOR · PAD {:02} · {}", pad + 1, kind.label());
    egui::Window::new(title)
        .open(&mut open)
        .default_size([900.0, 560.0])
        .min_width(680.0)
        .resizable(true)
        .show(ctx, |ui| match kind {
            ClipKind::Acid => acid_editor(ui, app, pad),
            ClipKind::Drums => drum_editor(ui, app, pad),
            ClipKind::Wave => wave_editor(ui, app, pad),
            _ => {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(
                        "This pad has no engine yet. Pick 303, WAVE or 909 in the inspector.",
                    )
                    .size(12.0)
                    .color(theme::DIM),
                );
            }
        });
    app.windows.editor = open;
}

fn header(ui: &mut Ui, app: &mut App, pad: usize) {
    let color = theme::kind_color(app.project.clips[pad].kind);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("PAD {:02}", pad + 1))
                .size(13.0)
                .strong()
                .color(color),
        );
        let name = app.project.clips[pad].name.clone();
        ui.label(RichText::new(name).size(12.0).color(theme::TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if chip(ui, "BROWSE", false).clicked() {
                app.windows.presets = true;
            }
            if chip(ui, "CLOSE", false).clicked() {
                app.windows.editor = false;
            }
        });
    });
    ui.add_space(4.0);
}

#[allow(clippy::needless_range_loop)]
fn acid_editor(ui: &mut Ui, app: &mut App, pad: usize) {
    header(ui, app, pad);
    ui.label(
        RichText::new("Click a cell to toggle. Drag NOTE to change pitch; ACCENT and SLIDE follow the hardware.")
            .size(10.0)
            .color(theme::DIM),
    );
    ui.add_space(6.0);
    let color = theme::AMBER;
    let mut changed = false;

    {
        let clip = &mut app.project.clips[pad];
        let mut steps = clip.steps;

        egui::Grid::new(("acid-grid", pad))
            .num_columns(17)
            .spacing(Vec2::new(3.0, 3.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("STEP")
                        .size(9.0)
                        .color(theme::DIM)
                        .monospace(),
                );
                for step in 0..16 {
                    ui.label(
                        RichText::new(format!("{:02}", step + 1))
                            .size(9.0)
                            .monospace()
                            .color(if step % 4 == 0 {
                                theme::TEXT
                            } else {
                                theme::DIM
                            }),
                    );
                }
                ui.end_row();

                ui.label(
                    RichText::new("NOTE")
                        .size(9.0)
                        .color(theme::DIM)
                        .monospace(),
                );
                for step in 0..16 {
                    let mut note = steps[step].note as i32;
                    let response =
                        ui.add(egui::DragValue::new(&mut note).speed(0.3).range(0..=127));
                    if response.changed() {
                        steps[step].note = note as u8;
                        changed = true;
                    }
                }
                ui.end_row();

                ui.label(
                    RichText::new("GATE")
                        .size(9.0)
                        .color(theme::DIM)
                        .monospace(),
                );
                for step in 0..16 {
                    let data = steps[step];
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(
                        rect,
                        CornerRadius::same(3),
                        if data.gate {
                            color
                        } else {
                            Color32::from_rgb(24, 31, 43)
                        },
                    );
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(3),
                        Stroke::new(1.0_f32, theme::LINE),
                        egui::StrokeKind::Inside,
                    );
                    if response.clicked() {
                        steps[step].gate = !data.gate;
                        changed = true;
                    }
                }
                ui.end_row();

                ui.label(RichText::new("ACC").size(9.0).color(theme::DIM).monospace());
                for step in 0..16 {
                    let data = steps[step];
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(
                        rect,
                        CornerRadius::same(3),
                        if data.accent {
                            theme::PINK
                        } else {
                            Color32::from_rgb(24, 31, 43)
                        },
                    );
                    if response.clicked() {
                        steps[step].accent = !data.accent;
                        if steps[step].accent {
                            steps[step].velocity = 118;
                        } else {
                            steps[step].velocity = 92;
                        }
                        changed = true;
                    }
                }
                ui.end_row();

                ui.label(
                    RichText::new("SLIDE")
                        .size(9.0)
                        .color(theme::DIM)
                        .monospace(),
                );
                for step in 0..16 {
                    let data = steps[step];
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(
                        rect,
                        CornerRadius::same(3),
                        if data.slide {
                            theme::CYAN
                        } else {
                            Color32::from_rgb(24, 31, 43)
                        },
                    );
                    if response.clicked() {
                        steps[step].slide = !data.slide;
                        changed = true;
                    }
                }
                ui.end_row();
            });

        if changed {
            clip.steps = steps;
        }
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if chip(ui, "RANDOMIZE", false).clicked() {
            randomize_acid(app, pad);
            changed = true;
        }
        if chip(ui, "CLEAR", false).clicked() {
            app.project.clips[pad].steps = [Step::default(); 16];
            changed = true;
        }
        ui.label(
            RichText::new(format!(
                "root {} · length {}",
                app.project.clips[pad].root, app.project.clips[pad].length
            ))
            .size(10.0)
            .color(theme::DIM),
        );
    });

    if changed {
        app.sync_pad(pad);
    }
}

fn randomize_acid(app: &mut App, pad: usize) {
    // Deterministic-but-fresh variation seeded from the pad and time.
    let mut state = ((pad as u32 + 1) * 2654435761)
        ^ std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(1);
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let root = app.project.clips[pad].root.clamp(24, 48);
    let mut steps = [Step::default(); 16];
    for (i, step) in steps.iter_mut().enumerate() {
        let gate = i == 0 || next() % 100 < 62;
        let scale = [0, 3, 5, 7, 10, 12];
        let note = root + scale[(next() % 6) as usize] + if next() % 5 == 0 { 12 } else { 0 };
        *step = Step {
            gate,
            note: note.clamp(0, 127) as u8,
            velocity: if next() % 4 == 0 { 118 } else { 92 },
            accent: gate && next() % 4 == 0,
            slide: gate && next() % 100 < 18,
        };
    }
    app.project.clips[pad].steps = steps;
}

#[allow(clippy::needless_range_loop)]
fn drum_editor(ui: &mut Ui, app: &mut App, pad: usize) {
    header(ui, app, pad);
    ui.label(
        RichText::new(
            "Click: toggle a hit. Shift-click: accent (velocity 120). Right-click: clear the row.",
        )
        .size(10.0)
        .color(theme::DIM),
    );
    ui.add_space(6.0);
    let color = theme::PINK;
    let mut changed = false;
    let shift = ui.input(|i| i.modifiers.shift);

    {
        let clip = &mut app.project.clips[pad];
        egui::Grid::new(("drum-grid", pad))
            .num_columns(18)
            .spacing(Vec2::new(3.0, 3.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("VOICE")
                        .size(9.0)
                        .color(theme::DIM)
                        .monospace(),
                );
                for step in 0..16 {
                    ui.label(
                        RichText::new(format!("{:02}", step + 1))
                            .size(9.0)
                            .monospace()
                            .color(if step % 4 == 0 {
                                theme::TEXT
                            } else {
                                theme::DIM
                            }),
                    );
                }
                ui.end_row();

                for voice in 0..11 {
                    ui.label(
                        RichText::new(DRUM_VOICE_NAMES[voice])
                            .size(10.0)
                            .monospace()
                            .color(theme::TEXT),
                    );
                    for step in 0..16 {
                        let on = clip.drum_on(voice, step);
                        let velocity = clip.drum_velocity(voice, step);
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(24.0, 22.0), Sense::click());
                        let painter = ui.painter();
                        let fill = if on {
                            if velocity >= 110 {
                                color
                            } else {
                                theme::dim(color, 0.55)
                            }
                        } else {
                            Color32::from_rgb(24, 31, 43)
                        };
                        painter.rect_filled(rect, CornerRadius::same(3), fill);
                        painter.rect_stroke(
                            rect,
                            CornerRadius::same(3),
                            Stroke::new(1.0_f32, theme::LINE),
                            egui::StrokeKind::Inside,
                        );
                        if on {
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", velocity / 16),
                                FontId::monospace(8.0),
                                Color32::BLACK,
                            );
                        }
                        if response.clicked() {
                            clip.set_drum_step(voice, step, !on);
                            clip.set_drum_velocity(voice, step, if shift { 120 } else { 100 });
                            changed = true;
                        }
                        if response.secondary_clicked() {
                            for step in 0..16 {
                                clip.set_drum_step(voice, step, false);
                            }
                            changed = true;
                        }
                    }
                    ui.end_row();
                }
            });
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if chip(ui, "CLEAR KIT", false).clicked() {
            app.project.clips[pad].drum_mask = [0; 11];
            changed = true;
        }
        if chip(ui, "FOUR ON THE FLOOR", false).clicked() {
            let clip = &mut app.project.clips[pad];
            for step in [0usize, 4, 8, 12] {
                clip.set_drum_step(0, step, true);
                clip.set_drum_velocity(0, step, 122);
            }
            changed = true;
        }
    });

    if changed {
        app.sync_pad(pad);
    }
}

fn wave_editor(ui: &mut Ui, app: &mut App, pad: usize) {
    header(ui, app, pad);
    let bank = (app.project.clips[pad].params[WaveParam::BANK] as usize).min(7);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("BANK {}", WAVETABLE_BANKS[bank]))
                .size(10.0)
                .color(theme::CYAN),
        );
        ui.label(
            RichText::new("click: add a note · right-click: remove · click a note: remove")
                .size(10.0)
                .color(theme::DIM),
        );
    });
    ui.add_space(6.0);

    let root = app.project.clips[pad].root.clamp(12, 108);
    let low = (root - 12).max(0);
    let high = (low + 24).min(127);
    let rows = (high - low + 1) as usize;
    let mut changed = false;
    let mut pending: Option<(i32, i32, bool)> = None;
    let note_length = 2i32;

    {
        let clip = &mut app.project.clips[pad];
        let notes = clip.midi_notes.clone();
        egui::Grid::new(("piano-roll", pad))
            .num_columns(17)
            .spacing(Vec2::new(2.0, 1.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("PITCH")
                        .size(9.0)
                        .color(theme::DIM)
                        .monospace(),
                );
                for step in 0..16 {
                    ui.label(
                        RichText::new(format!("{:02}", step + 1))
                            .size(9.0)
                            .monospace()
                            .color(if step % 4 == 0 {
                                theme::TEXT
                            } else {
                                theme::DIM
                            }),
                    );
                }
                ui.end_row();

                for row in (0..rows).rev() {
                    let pitch = low + row as i32;
                    let is_black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
                    ui.label(
                        RichText::new(format!("{pitch:>3}"))
                            .size(9.0)
                            .monospace()
                            .color(if is_black { theme::DIM } else { theme::TEXT }),
                    );
                    for step in 0..16 {
                        let covered = notes.iter().find(|note| {
                            note.note as i32 == pitch
                                && step as i32 >= note.start as i32
                                && (step as i32) < note.end()
                        });
                        let is_start = notes
                            .iter()
                            .any(|note| note.note as i32 == pitch && note.start as usize == step);
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(40.0, 13.0), Sense::click());
                        let painter = ui.painter();
                        let bg = if is_black {
                            Color32::from_rgb(15, 19, 27)
                        } else {
                            Color32::from_rgb(20, 26, 36)
                        };
                        painter.rect_filled(rect, CornerRadius::same(2), bg);
                        if covered.is_some() {
                            painter.rect_filled(
                                rect.shrink(1.0),
                                CornerRadius::same(2),
                                if is_start {
                                    theme::CYAN
                                } else {
                                    theme::dim(theme::CYAN, 0.45)
                                },
                            );
                        }
                        if response.clicked() {
                            if covered.is_some() {
                                pending = Some((pitch, step as i32, false));
                            } else {
                                pending = Some((pitch, step as i32, true));
                            }
                        }
                        if response.secondary_clicked() {
                            pending = Some((pitch, step as i32, false));
                        }
                    }
                    ui.end_row();
                }
            });
    }

    if let Some((pitch, step, add)) = pending {
        let clip = &mut app.project.clips[pad];
        if add {
            clip.add_midi_note(pitch, step, note_length, 100);
        } else {
            if let Some(note) = clip
                .midi_notes
                .iter()
                .find(|note| {
                    note.note as i32 == pitch && step >= note.start as i32 && step < note.end()
                })
                .copied()
            {
                clip.remove_midi_note(note.note as i32, note.start as i32);
            } else {
                clip.remove_midi_note(pitch, step);
            }
        }
        changed = true;
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if chip(ui, "CLEAR NOTES", false).clicked() {
            app.project.clips[pad].midi_notes.clear();
            changed = true;
        }
        if chip(ui, "LOAD PRESET", false).clicked() {
            app.windows.presets = true;
        }
        ui.label(
            RichText::new(format!(
                "{} notes · fx {}",
                app.project.clips[pad].midi_notes.len(),
                fx_info(app.project.clips[pad].fx_type).name
            ))
            .size(10.0)
            .color(theme::DIM),
        );
    });

    if changed {
        app.sync_pad(pad);
    }
}
