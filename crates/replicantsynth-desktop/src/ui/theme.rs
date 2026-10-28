//! NIGHT CITY palette and egui styling.

use egui::{Color32, CornerRadius, Stroke, Visuals};
use replicantsynth_core::ClipKind;

pub const BG: Color32 = Color32::from_rgb(7, 9, 14);
pub const PANEL: Color32 = Color32::from_rgb(13, 17, 25);
pub const PANEL_HI: Color32 = Color32::from_rgb(22, 29, 41);
pub const LINE: Color32 = Color32::from_rgb(34, 45, 62);
pub const TEXT: Color32 = Color32::from_rgb(214, 226, 240);
pub const DIM: Color32 = Color32::from_rgb(118, 136, 160);
pub const CYAN: Color32 = Color32::from_rgb(47, 230, 208);
pub const PINK: Color32 = Color32::from_rgb(255, 46, 136);
pub const AMBER: Color32 = Color32::from_rgb(255, 158, 61);
pub const VIOLET: Color32 = Color32::from_rgb(179, 136, 255);
pub const GREEN: Color32 = Color32::from_rgb(118, 230, 138);
pub const RED: Color32 = Color32::from_rgb(255, 84, 84);
pub const WHITE: Color32 = Color32::from_rgb(236, 244, 255);

pub const RADIUS: u8 = 6;

pub fn kind_color(kind: ClipKind) -> Color32 {
    match kind {
        ClipKind::Empty => Color32::from_rgb(44, 54, 70),
        ClipKind::Acid => AMBER,
        ClipKind::Wave => CYAN,
        ClipKind::Drums => PINK,
        ClipKind::Sample => VIOLET,
    }
}

pub fn dim(color: Color32, factor: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (color.a() as f32 * factor.clamp(0.0, 1.0)) as u8,
    )
}

pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
    )
}

pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(9, 12, 18);
    visuals.faint_bg_color = PANEL_HI;
    visuals.override_text_color = Some(TEXT);
    visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    visuals.selection.bg_fill = dim(CYAN, 0.35);
    visuals.selection.stroke = Stroke::new(1.0_f32, CYAN);
    visuals.hyperlink_color = CYAN;
    visuals.widgets.noninteractive.bg_fill = PANEL_HI;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, DIM);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(RADIUS);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(26, 34, 47);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(22, 29, 41);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(RADIUS);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 48, 64);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(32, 43, 58);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, CYAN);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(RADIUS);
    visuals.widgets.active.bg_fill = dim(CYAN, 0.30);
    visuals.widgets.active.weak_bg_fill = dim(CYAN, 0.22);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, CYAN);
    visuals.widgets.active.corner_radius = CornerRadius::same(RADIUS);
    visuals.widgets.open.bg_fill = PANEL_HI;
    visuals.widgets.open.corner_radius = CornerRadius::same(RADIUS);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(7.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(12);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(10.5));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(17.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(11.5));
    ctx.set_style(style);
}
