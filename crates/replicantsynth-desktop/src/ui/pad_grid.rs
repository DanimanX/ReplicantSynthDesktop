//! The 8 x 8 clip grid and its scene rail.

use super::theme;
use super::widgets::panel;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, Ui, Vec2};
use replicantsynth_core::engine::PadTelemetry;
use replicantsynth_core::model::{Clip, ProjectState, COLS, ROWS};
use replicantsynth_core::ClipState;

#[derive(Clone)]
pub struct Snapshot {
    pub pads: [PadTelemetry; 64],
    pub meters: [f32; 64],
    pub master_meter: f32,
    pub playhead: i32,
    pub absolute_step: i32,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            pads: [PadTelemetry::default(); 64],
            meters: [0.0; 64],
            master_meter: 0.0,
            playhead: 0,
            absolute_step: 0,
        }
    }
}

#[derive(Default)]
pub struct GridResponse {
    pub launch: Option<usize>,
    pub select: Option<usize>,
    pub editor: Option<usize>,
    pub context: Option<usize>,
    pub plugin_editor: Option<usize>,
    pub scene_launch: Option<usize>,
    pub scene_stop: Option<usize>,
}

fn pad_label(clip: &Clip) -> String {
    if !clip.name.trim().is_empty() {
        clip.name.clone()
    } else {
        replicantsynth_core::model::default_name(clip.kind, clip.index)
    }
}

pub fn draw(
    ui: &mut Ui,
    project: &ProjectState,
    snapshot: &Snapshot,
    selected: usize,
) -> GridResponse {
    let mut response = GridResponse::default();
    let available = ui.available_rect_before_wrap();
    let rail_width = 46.0;
    let gap = 4.0;
    let grid_width = available.width() - rail_width - 10.0;
    let cell_w = ((grid_width - gap * (COLS as f32 - 1.0)) / COLS as f32).max(24.0);
    let cell_h = ((available.height() - gap * (ROWS as f32 - 1.0)) / ROWS as f32).max(24.0);
    let cell_w = cell_w.min(cell_h * 1.35);
    let cell_h = (cell_h * 0.98).max(24.0);

    let grid_rect = Rect::from_min_size(
        available.min,
        Vec2::new(
            cell_w * COLS as f32 + gap * (COLS as f32 - 1.0),
            cell_h * ROWS as f32 + gap * (ROWS as f32 - 1.0),
        ),
    );
    panel(ui, grid_rect.expand(5.0), theme::LINE);

    let time = ui.input(|i| i.time) as f32;

    for row in 0..ROWS {
        for col in 0..COLS {
            let index = row * COLS + col;
            let clip = &project.clips[index];
            let telemetry = &snapshot.pads[index];
            let meter_value = snapshot.meters[index];
            let origin =
                grid_rect.min + Vec2::new(col as f32 * (cell_w + gap), row as f32 * (cell_h + gap));
            let rect = Rect::from_min_size(origin, Vec2::new(cell_w, cell_h));
            let id = ui.id().with(("pad", index));
            let pad_response = ui.interact(rect, id, Sense::click());

            let kind = clip.kind;
            let color = theme::kind_color(kind);
            let playing = telemetry.state == ClipState::Playing;
            let queued_start = telemetry.state == ClipState::QueuedStart;
            let queued_stop = telemetry.state == ClipState::QueuedStop;

            let pulse = 0.5 + 0.5 * (time * 4.0).sin();
            let base = if kind == replicantsynth_core::ClipKind::Empty {
                Color32::from_rgb(16, 21, 30)
            } else if playing {
                theme::blend(Color32::from_rgb(18, 24, 34), color, 0.28 + 0.10 * pulse)
            } else if queued_start || queued_stop {
                theme::blend(Color32::from_rgb(18, 24, 34), color, 0.12 + 0.10 * pulse)
            } else {
                theme::blend(Color32::from_rgb(18, 24, 34), color, 0.10)
            };

            let painter = ui.painter();
            let rounding = CornerRadius::same(5);
            painter.rect_filled(rect, rounding, base);

            // Kind stripe on the left edge.
            let stripe = Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height()));
            painter.rect_filled(
                stripe,
                CornerRadius {
                    nw: 5,
                    sw: 5,
                    ne: 0,
                    se: 0,
                },
                color,
            );

            let border = if index == selected {
                theme::WHITE
            } else if playing {
                color
            } else if queued_start || queued_stop {
                theme::blend(color, theme::PANEL, 0.35)
            } else {
                theme::LINE
            };
            painter.rect_stroke(
                rect,
                rounding,
                Stroke::new(if index == selected { 1.6_f32 } else { 1.0_f32 }, border),
                egui::StrokeKind::Inside,
            );

            if kind != replicantsynth_core::ClipKind::Empty {
                painter.text(
                    rect.left_top() + Vec2::new(8.0, 5.0),
                    Align2::LEFT_TOP,
                    format!("{:02}", index + 1),
                    FontId::monospace(9.0),
                    theme::dim(color, 0.85),
                );

                let title = pad_label(clip);
                let label = if title.chars().count() > 12 {
                    format!("{}…", title.chars().take(11).collect::<String>())
                } else {
                    title
                };
                painter.text(
                    rect.center() + Vec2::new(0.0, 2.0),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(if cell_w > 90.0 { 11.0 } else { 9.5 }),
                    if playing {
                        theme::TEXT
                    } else {
                        theme::blend(theme::TEXT, theme::DIM, 0.35)
                    },
                );

                painter.text(
                    rect.right_bottom() + Vec2::new(-6.0, -12.0),
                    Align2::RIGHT_BOTTOM,
                    kind.short(),
                    FontId::monospace(8.5),
                    theme::dim(color, 0.85),
                );

                if clip.fx_enabled && clip.fx_type != 0 {
                    painter.circle_filled(
                        rect.right_bottom() + Vec2::new(-46.0, -17.0),
                        2.6,
                        theme::PINK,
                    );
                }

                // Step phase: a 16-segment tick bar at the bottom.
                if playing {
                    let ticks = 16usize;
                    let width = (rect.width() - 12.0) / ticks as f32;
                    for step in 0..ticks {
                        let active = step == (telemetry.phase as usize % 16);
                        let tick = Rect::from_min_size(
                            rect.left_bottom() + Vec2::new(6.0 + step as f32 * width, -8.0),
                            Vec2::new((width - 1.0).max(1.0), 2.0),
                        );
                        painter.rect_filled(
                            tick,
                            CornerRadius::same(1),
                            if active {
                                color
                            } else {
                                theme::dim(color, 0.28)
                            },
                        );
                    }
                }
            } else {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    format!("{:02}", index + 1),
                    FontId::monospace(10.0),
                    theme::DIM,
                );
            }

            // Meter at the very bottom.
            let meter_rect = Rect::from_min_max(
                rect.left_bottom() + Vec2::new(6.0, -4.0),
                rect.right_bottom() + Vec2::new(-6.0, -2.0),
            );
            let painter = ui.painter();
            painter.rect_filled(
                meter_rect,
                CornerRadius::same(1),
                Color32::from_rgb(10, 13, 19),
            );
            if meter_value > 0.001 {
                let mut filled = meter_rect;
                filled.set_width(meter_rect.width() * meter_value.clamp(0.0, 1.0));
                let meter_color = if meter_value > 0.92 {
                    theme::RED
                } else if meter_value > 0.75 {
                    theme::AMBER
                } else {
                    color
                };
                painter.rect_filled(filled, CornerRadius::same(1), meter_color);
            }

            if pad_response.double_clicked() {
                response.select = Some(index);
                if clip.kind == replicantsynth_core::ClipKind::Plugin {
                    response.plugin_editor = Some(index);
                } else {
                    response.editor = Some(index);
                }
            } else if pad_response.clicked() {
                response.select = Some(index);
                if clip.kind != replicantsynth_core::ClipKind::Empty {
                    response.launch = Some(index);
                }
            }
            if pad_response.secondary_clicked() {
                response.select = Some(index);
                response.context = Some(index);
            }
            if pad_response.hovered() {
                let name = pad_label(clip);
                let _ = pad_response.on_hover_text(format!(
                    "{:02}  {}  ·  {}\nclick: launch / stop  ·  double-click: editor  ·  right-click: menu",
                    index + 1,
                    name,
                    telemetry.state.label()
                ));
            }
        }
    }

    // Scene rail.
    let rail_x = grid_rect.right() + 8.0;
    for row in 0..ROWS {
        let cell_h = (grid_rect.height() - gap * (ROWS as f32 - 1.0)) / ROWS as f32;
        let rect = Rect::from_min_size(
            egui::pos2(rail_x, grid_rect.top() + row as f32 * (cell_h + gap)),
            Vec2::new(rail_width - 8.0, cell_h),
        );
        let id = ui.id().with(("scene", row));
        let rail_response = ui.interact(rect, id, Sense::click());
        let active =
            (0..COLS).any(|col| snapshot.pads[row * COLS + col].state == ClipState::Playing);
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::same(4),
            if active {
                theme::dim(theme::CYAN, 0.22)
            } else {
                Color32::from_rgb(16, 21, 30)
            },
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0_f32, if active { theme::CYAN } else { theme::LINE }),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center() + Vec2::new(0.0, -5.0),
            Align2::CENTER_CENTER,
            format!("S{}", row + 1),
            FontId::monospace(9.5),
            if active { theme::CYAN } else { theme::DIM },
        );
        let triangle = vec![
            rect.center() + Vec2::new(-3.0, 7.0),
            rect.center() + Vec2::new(4.0, 10.0),
            rect.center() + Vec2::new(-3.0, 13.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            triangle,
            if active { theme::CYAN } else { theme::DIM },
            Stroke::NONE,
        ));

        if rail_response.clicked() {
            response.scene_launch = Some(row);
        }
        if rail_response.secondary_clicked() {
            response.scene_stop = Some(row);
        }
        let _ = rail_response.on_hover_text(format!(
            "Scene {}\nclick: launch row  ·  right-click: stop row",
            row + 1
        ));
    }

    response
}

/// A compact horizontal meter used by the transport bar.
pub fn master_meter(ui: &mut Ui, value: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(84.0, 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(10, 13, 19));
    let level = value.clamp(0.0, 1.0);
    let mut filled = rect;
    filled.set_width(rect.width() * level);
    painter.rect_filled(
        filled,
        CornerRadius::same(2),
        if level > 0.92 {
            theme::RED
        } else if level > 0.75 {
            theme::AMBER
        } else {
            theme::GREEN
        },
    );
}
