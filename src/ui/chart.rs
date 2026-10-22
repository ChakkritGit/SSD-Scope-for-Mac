//! Live read/write area chart drawn directly with the painter, with a hover crosshair and tooltip.
use super::{
    i18n::{tr, trf},
    theme::Theme,
};
use eframe::egui::{self, RichText, Stroke, pos2, vec2};
use ssd_scope_mac::telemetry::{History, Sample};

const WINDOW: f64 = 120.0;

fn nice_ceiling(value: f64) -> f64 {
    let value = value.max(1.0);
    let magnitude = 10f64.powf(value.log10().floor());
    for step in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if value <= step * magnitude {
            return step * magnitude;
        }
    }
    10.0 * magnitude
}

pub fn rate(mbps: f64) -> String {
    if mbps >= 100.0 {
        format!("{mbps:.0} MB/s")
    } else if mbps >= 1.0 {
        format!("{mbps:.1} MB/s")
    } else {
        format!("{:.0} KB/s", mbps * 1000.0)
    }
}

pub fn io(ui: &mut egui::Ui, history: Option<&History>, height: f32) {
    let t = Theme::of_ui(ui);
    let samples: Vec<&Sample> = history
        .map(|h| h.points.iter().collect())
        .unwrap_or_default();
    let now = samples.last().map(|s| s.elapsed).unwrap_or(0.0);
    let peak = samples
        .iter()
        .map(|s| s.read_mbps.max(s.write_mbps))
        .fold(0.0, f64::max);
    let top = nice_ceiling(peak * 1.15);
    let (outer, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), egui::Sense::hover());
    let plot = egui::Rect::from_min_max(outer.min + vec2(52.0, 6.0), outer.max - vec2(6.0, 22.0));
    let painter = ui.painter_at(outer);
    let x_of =
        |elapsed: f64| plot.left() + plot.width() * (1.0 - ((now - elapsed) / WINDOW) as f32);
    let y_of = |value: f64| plot.bottom() - plot.height() * (value / top).clamp(0.0, 1.0) as f32;
    for i in 0..=4 {
        let value = top * i as f64 / 4.0;
        let y = y_of(value);
        painter.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(
                1.0_f32,
                t.border.gamma_multiply(if i == 0 { 1.0 } else { 0.6 }),
            ),
        );
        painter.text(
            pos2(plot.left() - 8.0, y),
            egui::Align2::RIGHT_CENTER,
            rate(value),
            egui::FontId::proportional(10.5),
            t.faint,
        );
    }
    for (seconds, label) in [(120.0, "2 min ago"), (60.0, "1 min ago"), (0.0, "now")] {
        let x = plot.left() + plot.width() * (1.0 - seconds as f32 / WINDOW as f32);
        painter.text(
            pos2(x, plot.bottom() + 12.0),
            if seconds == 120.0 {
                egui::Align2::LEFT_CENTER
            } else if seconds == 0.0 {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::CENTER_CENTER
            },
            tr(label),
            egui::FontId::proportional(10.5),
            t.faint,
        );
    }
    if samples.len() < 2 {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            tr("Collecting disk activity…"),
            egui::FontId::proportional(13.0),
            t.muted,
        );
        return;
    }
    let visible: Vec<&Sample> = samples
        .iter()
        .copied()
        .filter(|s| now - s.elapsed <= WINDOW)
        .collect();
    for (pick, color) in [
        (
            &(|s: &Sample| s.write_mbps) as &dyn Fn(&Sample) -> f64,
            t.write,
        ),
        (
            &(|s: &Sample| s.read_mbps) as &dyn Fn(&Sample) -> f64,
            t.read,
        ),
    ] {
        let mut mesh = egui::Mesh::default();
        let top_color = color.gamma_multiply(if t.dark { 0.32 } else { 0.22 });
        let bottom_color = color.gamma_multiply(0.0);
        for s in &visible {
            let x = x_of(s.elapsed);
            mesh.colored_vertex(pos2(x, y_of(pick(s))), top_color);
            mesh.colored_vertex(pos2(x, plot.bottom()), bottom_color);
        }
        for i in 0..visible.len().saturating_sub(1) as u32 {
            let base = i * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 2, base + 3);
        }
        painter.add(egui::Shape::mesh(mesh));
        let line: Vec<_> = visible
            .iter()
            .map(|s| pos2(x_of(s.elapsed), y_of(pick(s))))
            .collect();
        painter.add(egui::Shape::line(line, Stroke::new(2.0_f32, color)));
    }
    let Some(pointer) = response
        .hover_pos()
        .filter(|p| plot.expand(4.0).contains(*p))
    else {
        return;
    };
    let nearest = visible
        .iter()
        .min_by(|a, b| {
            (x_of(a.elapsed) - pointer.x)
                .abs()
                .total_cmp(&(x_of(b.elapsed) - pointer.x).abs())
        })
        .copied();
    let Some(sample) = nearest else { return };
    let x = x_of(sample.elapsed);
    painter.line_segment(
        [pos2(x, plot.top()), pos2(x, plot.bottom())],
        Stroke::new(1.0_f32, t.muted.gamma_multiply(0.7)),
    );
    for (value, color) in [(sample.write_mbps, t.write), (sample.read_mbps, t.read)] {
        let point = pos2(x, y_of(value));
        painter.circle_filled(point, 5.0, t.surface);
        painter.circle_stroke(point, 5.0, Stroke::new(2.0_f32, color));
    }
    let ago = now - sample.elapsed;
    response.on_hover_ui_at_pointer(|ui| {
        ui.label(
            RichText::new(if ago < 1.0 {
                tr("Now")
            } else {
                trf("{} s ago", &[format!("{ago:.0}")])
            })
            .size(11.5)
            .color(t.muted),
        );
        for (name, value, color) in [
            ("Read", sample.read_mbps, t.read),
            ("Write", sample.write_mbps, t.write),
        ] {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, color);
                ui.label(RichText::new(tr(name)).color(t.muted));
                ui.label(RichText::new(rate(value)).strong().color(t.text));
            });
        }
    });
}

pub fn legend(ui: &mut egui::Ui, history: Option<&History>) {
    let t = Theme::of_ui(ui);
    let last = history.and_then(|h| h.points.back());
    let (read, write) = last
        .map(|s| (s.read_mbps, s.write_mbps))
        .unwrap_or_default();
    for (name, value, color) in [("Read", read, t.read), ("Write", write, t.write)] {
        let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, color);
        ui.label(RichText::new(tr(name)).color(t.muted));
        ui.label(RichText::new(rate(value)).strong().color(t.text));
        ui.add_space(8.0);
    }
}

pub fn peak(history: Option<&History>) -> (f64, f64) {
    history
        .map(|h| {
            h.points.iter().fold((0.0, 0.0), |(r, w), s: &Sample| {
                (f64::max(r, s.read_mbps), f64::max(w, s.write_mbps))
            })
        })
        .unwrap_or_default()
}
