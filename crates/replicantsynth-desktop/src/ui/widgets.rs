//! Small custom widgets: neon knobs, chips, meters and section labels.

use super::theme;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Raw,
    Percent,
    Hertz,
    Seconds,
    Cents,
    Semitones,
    /// Reserved for gain-staging displays; kept so the formatting table is whole.
    #[allow(dead_code)]
    Decibels,
}

pub fn format_value(value: f32, unit: Unit, min: f32, max: f32) -> String {
    match unit {
        Unit::Percent => format!("{:.0}%", value * 100.0),
        Unit::Hertz => {
            if value >= 1000.0 {
                format!("{:.2}k", value / 1000.0)
            } else if value >= 100.0 {
                format!("{value:.0}")
            } else {
                format!("{value:.1}")
            }
        }
        Unit::Seconds => {
            if value < 0.1 {
                format!("{:.0}ms", value * 1000.0)
            } else {
                format!("{value:.2}s")
            }
        }
        Unit::Cents => format!("{value:.0}c"),
        Unit::Semitones => format!("{value:+.0}st"),
        Unit::Decibels => format!("{value:+.1}dB"),
        Unit::Raw => {
            let span = max - min;
            if span >= 100.0 {
                format!("{value:.0}")
            } else if span >= 10.0 {
                format!("{value:.1}")
            } else {
                format!("{value:.2}")
            }
        }
    }
}

/// A draggable rotary control. Returns true when the value changed.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    unit: Unit,
    default: Option<f32>,
    color: Color32,
) -> bool {
    let size = Vec2::new(58.0, 70.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let mut changed = false;

    if response.dragged() {
        let delta = response.drag_delta();
        let speed = if ui.input(|i| i.modifiers.shift) {
            600.0
        } else {
            160.0
        };
        let step = (max - min) / speed;
        *value = (*value - delta.y * step).clamp(min, max);
        if delta.y != 0.0 {
            changed = true;
        }
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let step = (max - min) / 60.0;
            *value = (*value + scroll.signum() * step).clamp(min, max);
            changed = true;
        }
    }
    if response.double_clicked() {
        if let Some(default) = default {
            *value = default;
            changed = true;
        }
    }
    if response.drag_stopped() || response.clicked() {
        // Keep keyboard focus cheap: nothing to do, but the response is used so
        // egui keeps the widget interactive.
    }
    if let Some(default) = default {
        if ui.rect_contains_pointer(rect) && ui.input(|i| i.pointer.secondary_clicked()) {
            *value = default;
            changed = true;
        }
    }

    let painter = ui.painter();
    let hovered = response.hovered();
    let radius = 17.0;
    let center = rect.center_top() + Vec2::new(0.0, 21.0);
    let t = ((*value - min) / (max - min).max(1.0e-9)).clamp(0.0, 1.0);

    painter.circle_filled(center, radius + 2.0, Color32::from_rgb(12, 16, 24));
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0_f32, if hovered { color } else { theme::LINE }),
    );

    // Value arc from 135° to 405°.
    let start = 135.0f32.to_radians();
    let sweep = 270.0f32.to_radians();
    let steps = 28;
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let frac = i as f32 / steps as f32;
        let angle = start + sweep * frac * t;
        points.push(center + Vec2::new(angle.cos(), angle.sin()) * (radius - 3.0));
    }
    if points.len() > 1 {
        painter.add(egui::Shape::line(points, Stroke::new(2.6_f32, color)));
    }

    let indicator = start + sweep * t;
    let inner = center + Vec2::new(indicator.cos(), indicator.sin()) * 5.0;
    let outer = center + Vec2::new(indicator.cos(), indicator.sin()) * (radius - 4.0);
    painter.line_segment([inner, outer], Stroke::new(2.0_f32, theme::TEXT));

    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 20.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(9.5),
        theme::DIM,
    );
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 6.0),
        Align2::CENTER_CENTER,
        format_value(*value, unit, min, max),
        FontId::monospace(10.0),
        if hovered { color } else { theme::TEXT },
    );

    changed
}

/// A pill-shaped toggle.
pub fn chip(ui: &mut Ui, label: &str, active: bool) -> Response {
    let font = FontId::proportional(11.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), theme::TEXT);
    let size = Vec2::new(galley.size().x + 18.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let bg = if active {
        theme::dim(theme::CYAN, 0.28)
    } else if response.hovered() {
        theme::PANEL_HI
    } else {
        Color32::from_rgb(19, 25, 35)
    };
    painter.rect_filled(rect, CornerRadius::same(10), bg);
    painter.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0_f32, if active { theme::CYAN } else { theme::LINE }),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        if active { theme::CYAN } else { theme::TEXT },
    );
    response
}

/// A thin horizontal meter; `value` is a linear peak 0..1.
pub fn meter(ui: &mut Ui, value: f32, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(10, 13, 19));
    let level = value.clamp(0.0, 1.0);
    if level > 0.0005 {
        let mut filled = rect;
        filled.set_width(rect.width() * level);
        let color = if level > 0.92 {
            theme::RED
        } else if level > 0.75 {
            theme::AMBER
        } else {
            theme::GREEN
        };
        painter.rect_filled(filled, CornerRadius::same(2), color);
    }
}

/// A small uppercase section heading with a rule underneath it.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    let response = ui.label(
        egui::RichText::new(text)
            .size(9.5)
            .color(theme::DIM)
            .strong(),
    );
    let rect = response.rect;
    let y = rect.bottom() + 3.0;
    let available = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(available.right(), y)],
        Stroke::new(1.0_f32, theme::LINE),
    );
    ui.add_space(7.0);
}

pub fn divider(ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [
            egui::pos2(rect.left(), rect.top()),
            egui::pos2(rect.right(), rect.top()),
        ],
        Stroke::new(1.0_f32, theme::LINE),
    );
    ui.add_space(5.0);
}

/// A small status LED with a label.
pub fn status_dot(ui: &mut Ui, on: bool, color: Color32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(11.0, 11.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 4.0, if on { color } else { theme::LINE });
    ui.label(egui::RichText::new(label).size(11.0).color(theme::DIM));
}

/// Draw the neon frame used by the pad grid and inspector.
pub fn panel(ui: &mut Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), theme::PANEL);
    painter.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0_f32, color),
        egui::StrokeKind::Inside,
    );
}
