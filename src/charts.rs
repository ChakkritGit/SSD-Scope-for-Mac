use eframe::egui::{self, Color32, RichText};
use egui_plot::{Line, Plot, PlotPoints};
use ssd_scope_mac::{disks, telemetry::History};

pub const READ: Color32 = Color32::from_rgb(36, 139, 112);
pub const WRITE: Color32 = Color32::from_rgb(108, 134, 205);
pub fn activity(ui: &mut egui::Ui, history: Option<&History>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Live activity").size(19.0).strong());
        ui.label(
            RichText::new("Last 2 minutes · MB/s")
                .small()
                .color(Color32::GRAY),
        );
    });
    let points = history.map(|h| &h.points);
    if points.is_none_or(|p| p.is_empty()) {
        ui.label("Collecting disk activity… Graphs use real IORegistry counters.");
    }
    let end = points
        .and_then(|p| p.back())
        .map(|p| p.elapsed)
        .unwrap_or(0.0);
    let reads: PlotPoints = points
        .into_iter()
        .flatten()
        .map(|p| [p.elapsed - end, p.read_mbps])
        .collect();
    let writes: PlotPoints = points
        .into_iter()
        .flatten()
        .map(|p| [p.elapsed - end, p.write_mbps])
        .collect();
    Plot::new("live-io")
        .height(185.0)
        .include_x(-120.0)
        .include_x(0.0)
        .include_y(0.0)
        .include_y(1.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .show_axes([false, true])
        .show_background(false)
        .show_grid([false, true])
        .grid_spacing(egui::Rangef::new(55.0, 100.0))
        .show(ui, |plot| {
            plot.line(Line::new("Read", reads).color(READ).width(2.0_f32));
            plot.line(Line::new("Write", writes).color(WRITE).width(2.0_f32));
        });
    let (read, write) = points
        .and_then(|p| p.back())
        .map(|p| (p.read_mbps, p.write_mbps))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.colored_label(READ, format!("Read {read:.2} MB/s"));
        ui.colored_label(WRITE, format!("Write {write:.2} MB/s"));
    });
}
pub fn capacity(ui: &mut egui::Ui, capacity: u64, free: u64) {
    let used = capacity.saturating_sub(free);
    let fraction = if capacity > 0 {
        used as f32 / capacity as f32
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(160.0, 160.0), egui::Sense::hover());
    let center = rect.center();
    let radius = 62.0;
    ui.painter().circle_stroke(
        center,
        radius,
        egui::Stroke::new(12.0_f32, Color32::from_rgb(231, 237, 234)),
    );
    let count = (fraction * 100.0).ceil() as usize;
    if count > 0 {
        let path = (0..=count)
            .map(|i| {
                let angle = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * fraction * i as f32 / count as f32;
                center + egui::vec2(angle.cos(), angle.sin()) * radius
            })
            .collect();
        ui.painter()
            .add(egui::Shape::line(path, egui::Stroke::new(12.0_f32, READ)));
    }
    ui.painter().text(
        center - egui::vec2(0.0, 7.0),
        egui::Align2::CENTER_CENTER,
        format!("{:.0}%", fraction * 100.0),
        egui::FontId::proportional(27.0),
        Color32::from_rgb(30, 52, 43),
    );
    ui.painter().text(
        center + egui::vec2(0.0, 20.0),
        egui::Align2::CENTER_CENTER,
        "used",
        egui::FontId::proportional(13.0),
        Color32::GRAY,
    );
    ui.label(format!("{} used", disks::bytes(used)));
    ui.label(format!("{} free", disks::bytes(free)));
}
pub fn glyph(ui: &mut egui::Ui, kind: usize, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
    glyph_at(ui.painter(), rect, kind, color);
}

pub fn navigation(ui: &mut egui::Ui, kind: usize, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 42.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    let active = response.hovered() || response.has_focus();
    let color = if selected {
        READ
    } else {
        Color32::from_rgb(91, 108, 99)
    };
    let fill = if selected {
        Color32::from_rgb(220, 237, 228)
    } else if active {
        Color32::from_rgb(235, 241, 237)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 9, fill);
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            9,
            egui::Stroke::new(1.0_f32, READ),
            egui::StrokeKind::Inside,
        );
    }
    let icon = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 23.0, rect.center().y),
        egui::vec2(20.0, 20.0),
    );
    glyph_at(ui.painter(), icon, kind, color);
    ui.painter().text(
        egui::pos2(rect.left() + 46.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn glyph_at(p: &egui::Painter, rect: egui::Rect, kind: usize, color: Color32) {
    let r = rect.shrink(3.0);
    let stroke = egui::Stroke::new(1.5_f32, color);
    match kind {
        0 => {
            p.rect_stroke(r, 3, stroke, egui::StrokeKind::Inside);
            p.line_segment([r.left_center(), r.right_center()], stroke);
        }
        1 => {
            p.circle_stroke(r.center(), r.width() / 2.0, stroke);
            p.line_segment(
                [
                    r.center() - egui::vec2(0.0, 4.0),
                    r.center() + egui::vec2(0.0, 3.0),
                ],
                stroke,
            );
            p.circle_filled(r.center() + egui::vec2(0.0, 5.0), 1.0, color);
        }
        2 => {
            p.line_segment([r.left_bottom(), r.right_top()], stroke);
            p.circle_stroke(r.left_bottom(), 3.0, stroke);
            p.circle_stroke(r.right_top(), 3.0, stroke);
        }
        3 => {
            p.rect_stroke(r, 2, stroke, egui::StrokeKind::Inside);
            p.line_segment([r.center_top(), r.center_bottom()], stroke);
            p.line_segment(
                [
                    r.center_bottom(),
                    r.center_bottom() + egui::vec2(-4.0, -4.0),
                ],
                stroke,
            );
        }
        _ => {
            p.circle_stroke(r.center(), 4.0, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = egui::vec2(a.cos(), a.sin());
                p.line_segment([r.center() + d * 5.0, r.center() + d * 7.0], stroke);
            }
        }
    }
}
