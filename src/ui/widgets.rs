//! Drawing primitives for the dashboard: cards, gauges, badges, navigation and icons.
use super::{i18n::tr, theme::Theme};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, pos2, vec2};
use std::f32::consts::{PI, TAU};

pub fn card<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = Theme::of_ui(ui);
    egui::Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(
            1.0_f32,
            if t.dark {
                t.border
            } else {
                Color32::TRANSPARENT
            },
        ))
        .corner_radius(4)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui)
        })
        .inner
}

/// A card with a title row and optional trailing text.
pub fn panel(ui: &mut egui::Ui, title: &str, trailing: &str, contents: impl FnOnce(&mut egui::Ui)) {
    card(ui, |ui| {
        let t = Theme::of_ui(ui);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(title)).size(15.0).strong().color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(tr(trailing)).size(11.5).color(t.muted));
            });
        });
        ui.add_space(4.0);
        contents(ui);
    });
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .corner_radius(99)
        .inner_margin(egui::Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(tr(text)).size(11.5).strong().color(color));
        })
        .response
}

pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    let t = Theme::of_ui(ui);
    ui.label(
        RichText::new(tr(text).to_uppercase())
            .size(10.5)
            .color(t.muted)
            .extra_letter_spacing(0.8),
    );
}

pub fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let t = Theme::of_ui(ui);
    ui.add(
        egui::Button::new(RichText::new(tr(text)).color(Color32::WHITE).strong())
            .fill(t.accent)
            .stroke(Stroke::NONE)
            .corner_radius(3),
    )
}

pub fn secondary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let t = Theme::of_ui(ui);
    ui.add(
        egui::Button::new(RichText::new(tr(text)).color(t.text))
            .fill(t.raised)
            .stroke(Stroke::new(1.0_f32, t.border))
            .corner_radius(7),
    )
}

/// Stat tile: small label, large value and a muted footnote.
pub fn stat(ui: &mut egui::Ui, label: &str, value: &str, note: &str, color: Color32) {
    card(ui, |ui| {
        let t = Theme::of_ui(ui);
        eyebrow(ui, label);
        ui.label(RichText::new(value).size(24.0).color(color));
        ui.label(RichText::new(tr(note)).size(11.5).color(t.muted));
    });
}

fn lighten(color: Color32, amount: f32) -> Color32 {
    let mix = |c: u8| (c as f32 + (255.0 - c as f32) * amount).round() as u8;
    Color32::from_rgb(mix(color.r()), mix(color.g()), mix(color.b()))
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn track_color(t: &Theme) -> Color32 {
    if t.dark {
        Color32::from_white_alpha(22)
    } else {
        Color32::from_black_alpha(16)
    }
}

/// Activity-style ring: thin neutral track, gradient progress with round caps.
pub fn arc_gauge(
    ui: &mut egui::Ui,
    fraction: Option<f32>,
    value: &str,
    caption: &str,
    color: Color32,
    size: f32,
) {
    let t = Theme::of_ui(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    let center = rect.center();
    let width = size * 0.085;
    let radius = size / 2.0 - width / 2.0 - 2.0;
    let painter = ui.painter();
    painter.circle_stroke(center, radius, Stroke::new(width, track_color(&t)));
    let fraction = fraction.map(|f| f.clamp(0.0, 1.0));
    if let Some(fraction) = fraction.filter(|f| *f > 0.0) {
        let start = -PI / 2.0;
        let sweep = TAU * fraction;
        let from = lighten(color, 0.45);
        let steps = (fraction * 180.0).ceil().max(2.0) as usize;
        let point = |i: usize| {
            let a = start + sweep * i as f32 / steps as f32;
            center + vec2(a.cos(), a.sin()) * radius
        };
        // Short overlapping round-capped segments give a smooth angular gradient.
        for i in 0..steps {
            let shade = lerp_color(from, color, (i as f32 + 0.5) / steps as f32);
            painter.line_segment([point(i), point(i + 1)], Stroke::new(width, shade));
            painter.circle_filled(point(i + 1), width / 2.0, shade);
        }
        painter.circle_filled(point(0), width / 2.0, from);
        let tip = point(steps);
        painter.circle_filled(
            tip + vec2(0.0, 1.0),
            width / 2.0 + 1.0,
            Color32::from_black_alpha(if t.dark { 60 } else { 25 }),
        );
        painter.circle_filled(tip, width / 2.0, color);
    }
    let value_color = if fraction.is_some() { t.text } else { t.muted };
    painter.text(
        center - vec2(0.0, if caption.is_empty() { 0.0 } else { size * 0.05 }),
        egui::Align2::CENTER_CENTER,
        value,
        egui::FontId::proportional(size * 0.21),
        value_color,
    );
    if !caption.is_empty() {
        painter.text(
            center + vec2(0.0, size * 0.12),
            egui::Align2::CENTER_CENTER,
            tr(caption),
            egui::FontId::proportional(size * 0.075),
            t.muted,
        );
    }
}

/// Slim capsule bar with a gradient fill and an optional marker (e.g. 100% of rated endurance).
pub fn bar(ui: &mut egui::Ui, fraction: f32, color: Color32, marker: Option<f32>, height: f32) {
    let t = Theme::of_ui(ui);
    let height = height.min(8.0);
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
    let radius = height / 2.0;
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius as u8), track_color(&t));
    let fill_width = rect.width() * fraction.clamp(0.0, 1.0);
    if fill_width > 0.5 {
        // Capsule = end caps plus a horizontal gradient body.
        let from = lighten(color, 0.4);
        let left = rect.left() + radius;
        let right = (rect.left() + fill_width - radius).max(left);
        let mut mesh = egui::Mesh::default();
        let span = (right - left).max(1.0);
        let steps = 24;
        for i in 0..=steps {
            let x = left + span * i as f32 / steps as f32;
            let shade = lerp_color(from, color, (x - left) / span);
            mesh.colored_vertex(pos2(x, rect.top()), shade);
            mesh.colored_vertex(pos2(x, rect.bottom()), shade);
        }
        for i in 0..steps as u32 {
            let b = i * 2;
            mesh.add_triangle(b, b + 1, b + 2);
            mesh.add_triangle(b + 1, b + 2, b + 3);
        }
        if right > left {
            ui.painter().add(egui::Shape::mesh(mesh));
        }
        ui.painter()
            .circle_filled(pos2(left, rect.center().y), radius, from);
        ui.painter()
            .circle_filled(pos2(right, rect.center().y), radius, color);
    }
    if let Some(marker) = marker {
        let x = rect.left() + rect.width() * marker.clamp(0.0, 1.0);
        ui.painter().line_segment(
            [pos2(x, rect.top() - 5.0), pos2(x, rect.bottom() + 5.0)],
            Stroke::new(2.0_f32, t.muted),
        );
    }
}

pub fn advisory(ui: &mut egui::Ui, color: Color32, title: &str, body: &str) {
    let t = Theme::of_ui(ui);
    egui::Frame::new()
        .fill(color.gamma_multiply(if t.dark { 0.12 } else { 0.07 }))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.25)))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 11))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 8.0, color);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "!",
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(RichText::new(tr(title)).strong().color(color));
                    ui.label(RichText::new(tr(body)).color(t.text));
                });
            });
        });
}

/// Segmented control used for page tabs.
pub fn segmented(ui: &mut egui::Ui, selected: &mut usize, labels: &[&str]) {
    let t = Theme::of_ui(ui);
    egui::Frame::new()
        .fill(if t.dark {
            t.raised
        } else {
            t.border.gamma_multiply(0.7)
        })
        .corner_radius(8)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (i, label) in labels.iter().enumerate() {
                    let active = *selected == i;
                    let text = RichText::new(tr(label)).size(13.0).color(if active {
                        t.text
                    } else {
                        t.muted
                    });
                    let button = egui::Button::new(text)
                        .fill(if active {
                            t.surface
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::NONE)
                        .corner_radius(6)
                        .min_size(vec2(88.0, 26.0));
                    if ui.add(button).clicked() {
                        *selected = i;
                    }
                }
            });
        });
}

pub enum Glyph {
    Dashboard,
    Health,
    Performance,
    Backup,
    Settings,
    Drive,
    Internal,
}

pub fn nav_item(ui: &mut egui::Ui, glyph: Glyph, label: &str, selected: bool) -> egui::Response {
    let t = Theme::of_ui(ui);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 32.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    let fill = if selected {
        t.sidebar_selected
    } else if response.hovered() {
        t.sidebar_selected.gamma_multiply(0.5)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 7, fill);
    let icon =
        egui::Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(17.0, 17.0));
    draw_glyph(ui.painter(), icon, &glyph, t.accent);
    ui.painter().text(
        pos2(rect.left() + 36.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        tr(label),
        egui::FontId::proportional(13.5),
        if selected {
            t.sidebar_selected_text
        } else {
            t.sidebar_text
        },
    );
    response
}

/// Sidebar drive entry with a status dot.
pub fn drive_item(
    ui: &mut egui::Ui,
    title: &str,
    detail: &str,
    internal: bool,
    dot: Option<Color32>,
    selected: bool,
) -> egui::Response {
    let t = Theme::of_ui(ui);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 50.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, title)
    });
    let fill = if selected {
        t.sidebar_selected
    } else if response.hovered() {
        t.sidebar_selected.gamma_multiply(0.5)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 7, fill);
    let icon =
        egui::Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), vec2(28.0, 28.0));
    let tone = if internal { t.accent_alt } else { t.accent };
    ui.painter().rect_filled(icon, 7, tone.gamma_multiply(0.14));
    draw_glyph(
        ui.painter(),
        icon.shrink(6.0),
        &if internal {
            Glyph::Internal
        } else {
            Glyph::Drive
        },
        tone,
    );
    let mut job = egui::text::LayoutJob::simple_singleline(
        title.into(),
        egui::FontId::proportional(13.0),
        t.sidebar_text,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 66.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(rect.left() + 44.0, rect.top() + 9.0),
        galley,
        t.sidebar_text,
    );
    ui.painter().text(
        pos2(rect.left() + 44.0, rect.bottom() - 13.0),
        egui::Align2::LEFT_CENTER,
        detail,
        egui::FontId::proportional(11.0),
        t.sidebar_muted,
    );
    if let Some(dot) = dot {
        ui.painter()
            .circle_filled(pos2(rect.right() - 14.0, rect.center().y), 4.0, dot);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn draw_glyph(p: &egui::Painter, r: egui::Rect, glyph: &Glyph, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    let c = r.center();
    let w = r.width();
    match glyph {
        Glyph::Dashboard => {
            let h = w * 0.22;
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                p.rect_stroke(
                    egui::Rect::from_center_size(c + vec2(dx * h, dy * h), vec2(h * 1.7, h * 1.7)),
                    2,
                    s,
                    egui::StrokeKind::Middle,
                );
            }
        }
        Glyph::Health => {
            let pts = [
                c + vec2(-w * 0.48, 0.0),
                c + vec2(-w * 0.2, 0.0),
                c + vec2(-w * 0.08, -w * 0.32),
                c + vec2(w * 0.08, w * 0.32),
                c + vec2(w * 0.2, 0.0),
                c + vec2(w * 0.48, 0.0),
            ];
            p.add(egui::Shape::line(pts.to_vec(), s));
        }
        Glyph::Performance => {
            let radius = w * 0.45;
            let pts: Vec<_> = (0..=24)
                .map(|i| {
                    let a = PI + PI * i as f32 / 24.0;
                    c + vec2(0.0, w * 0.15) + vec2(a.cos(), a.sin()) * radius
                })
                .collect();
            p.add(egui::Shape::line(pts, s));
            let a = PI * 1.68;
            p.line_segment(
                [
                    c + vec2(0.0, w * 0.15),
                    c + vec2(0.0, w * 0.15) + vec2(a.cos(), a.sin()) * radius * 0.8,
                ],
                s,
            );
        }
        Glyph::Backup => {
            p.circle_stroke(c, w * 0.42, s);
            p.line_segment([c + vec2(0.0, w * 0.2), c - vec2(0.0, w * 0.2)], s);
            p.line_segment([c - vec2(0.0, w * 0.2), c + vec2(-w * 0.14, -w * 0.06)], s);
            p.line_segment([c - vec2(0.0, w * 0.2), c + vec2(w * 0.14, -w * 0.06)], s);
        }
        Glyph::Settings => {
            p.circle_stroke(c, w * 0.18, s);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * w * 0.3, c + d * w * 0.46], s);
            }
        }
        Glyph::Drive | Glyph::Internal => {
            let body = egui::Rect::from_center_size(c, vec2(w * 0.95, w * 0.62));
            p.rect_stroke(body, 3, s, egui::StrokeKind::Middle);
            p.circle_filled(body.right_center() - vec2(w * 0.17, 0.0), w * 0.06, color);
            if matches!(glyph, Glyph::Drive) {
                p.line_segment(
                    [
                        body.left_center() + vec2(w * 0.14, 0.0),
                        body.center() + vec2(w * 0.02, 0.0),
                    ],
                    s,
                );
            } else {
                p.line_segment(
                    [
                        body.left_bottom() + vec2(0.0, w * 0.18),
                        body.right_bottom() + vec2(0.0, w * 0.18),
                    ],
                    s,
                );
            }
        }
    }
}

/// A neutral 2.5-inch SSD illustration for the drive information card.
pub fn drive_picture(ui: &mut egui::Ui, size: egui::Vec2) {
    let t = Theme::of_ui(ui);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let body = egui::Rect::from_center_size(rect.center(), vec2(size.x * 0.92, size.y * 0.82));
    painter.rect_filled(
        body.translate(vec2(3.0, 4.0)),
        6,
        Color32::from_black_alpha(if t.dark { 90 } else { 30 }),
    );
    let top = if t.dark { 0x5a } else { 0xe4 };
    let bottom = if t.dark { 0x3c } else { 0xbd };
    let mut mesh = egui::Mesh::default();
    for (pos, shade) in [
        (body.left_top(), top),
        (body.right_top(), top),
        (body.right_bottom(), bottom),
        (body.left_bottom(), bottom),
    ] {
        mesh.colored_vertex(pos, Color32::from_gray(shade));
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(
        body,
        6,
        Stroke::new(
            1.0_f32,
            Color32::from_gray(if t.dark { 0x70 } else { 0xa8 }),
        ),
        egui::StrokeKind::Inside,
    );
    for corner in [
        body.left_top() + vec2(8.0, 8.0),
        body.right_top() + vec2(-8.0, 8.0),
        body.left_bottom() + vec2(8.0, -8.0),
        body.right_bottom() + vec2(-8.0, -8.0),
    ] {
        painter.circle_filled(
            corner,
            3.0,
            Color32::from_gray(if t.dark { 0x30 } else { 0x9a }),
        );
    }
    let label = egui::Rect::from_min_size(
        body.left_top() + vec2(body.width() * 0.12, body.height() * 0.22),
        vec2(body.width() * 0.5, body.height() * 0.56),
    );
    painter.rect_filled(
        label,
        3,
        Color32::from_gray(if t.dark { 0x2a } else { 0xf8 }),
    );
    painter.text(
        label.left_top() + vec2(8.0, 10.0),
        egui::Align2::LEFT_CENTER,
        "SSD",
        egui::FontId::proportional(13.0),
        if t.dark {
            Color32::WHITE
        } else {
            Color32::from_gray(0x33)
        },
    );
    for i in 0..3 {
        let y = label.top() + 24.0 + i as f32 * 8.0;
        painter.line_segment(
            [
                pos2(label.left() + 8.0, y),
                pos2(label.right() - 10.0 - i as f32 * 12.0, y),
            ],
            Stroke::new(
                2.0_f32,
                Color32::from_gray(if t.dark { 0x55 } else { 0xcc }),
            ),
        );
    }
    painter.text(
        pos2(body.right() - body.width() * 0.14, body.center().y),
        egui::Align2::CENTER_CENTER,
        "2.5\"",
        egui::FontId::proportional(18.0),
        Color32::from_gray(if t.dark { 0x88 } else { 0x99 }),
    );
}
