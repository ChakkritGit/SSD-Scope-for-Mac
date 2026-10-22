//! Original app icon rasterized from signed-distance geometry: a red tile with a 2.5-inch
//! drive under a magnifying glass whose lens shows a health ring. Shared by the app, Dock
//! icon and menu bar template.
use std::f32::consts::{PI, TAU};

fn rounded_box(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hx - r);
    let qy = (py - cy).abs() - (hy - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

fn circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    (px - cx).hypot(py - cy) - r
}

/// Distance to a segment with round ends of radius `r`.
fn capsule(px: f32, py: f32, a: (f32, f32), b: (f32, f32), r: f32) -> f32 {
    let (bx, by) = (b.0 - a.0, b.1 - a.1);
    let (qx, qy) = (px - a.0, py - a.1);
    let h = ((qx * bx + qy * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    (qx - bx * h).hypot(qy - by * h) - r
}

/// Distance to a stroked arc around `c` with round caps, from `start` sweeping `sweep` radians.
fn arc(px: f32, py: f32, c: (f32, f32), radius: f32, width: f32, start: f32, sweep: f32) -> f32 {
    let (dx, dy) = (px - c.0, py - c.1);
    let relative = (dy.atan2(dx) - start).rem_euclid(TAU);
    if relative <= sweep {
        (dx.hypot(dy) - radius).abs() - width / 2.0
    } else {
        [start, start + sweep]
            .iter()
            .map(|a| (px - c.0 - a.cos() * radius).hypot(py - c.1 - a.sin() * radius))
            .fold(f32::MAX, f32::min)
            - width / 2.0
    }
}

pub fn rgba(size: u32, template: bool) -> Vec<u8> {
    let n = size as usize;
    let mut data = vec![0; n * n * 4];
    let scale = size as f32;
    let coverage = |d: f32| (0.5 - d * scale).clamp(0.0, 1.0);
    // Geometry shared by the colour icon and the template.
    let lens = (0.605, 0.585);
    let lens_r = 0.15;
    let ring_w = 0.042;
    for y in 0..n {
        for x in 0..n {
            let px = (x as f32 + 0.5) / scale;
            let py = (y as f32 + 0.5) / scale;
            // Premultiplied RGBA accumulation, painted back to front.
            let mut out = [0.0f32; 4];
            let mut over = |rgb: [f32; 3], alpha: f32| {
                for i in 0..3 {
                    out[i] = rgb[i] * alpha + out[i] * (1.0 - alpha);
                }
                out[3] = alpha + out[3] * (1.0 - alpha);
            };
            let drive = coverage(rounded_box(px, py, 0.425, 0.42, 0.245, 0.175, 0.035));
            let label = coverage(rounded_box(px, py, 0.365, 0.42, 0.13, 0.11, 0.018));
            let mut lines = 0.0f32;
            for (i, length) in [0.17f32, 0.13, 0.15].iter().enumerate() {
                let ly = 0.385 + i as f32 * 0.04;
                lines = lines.max(coverage(capsule(
                    px,
                    py,
                    (0.275, ly),
                    (0.275 + length, ly),
                    0.0085,
                )));
            }
            let mut screws = 0.0f32;
            for (sx, sy) in [(0.21, 0.275), (0.64, 0.275), (0.21, 0.565), (0.64, 0.565)] {
                screws = screws.max(coverage(circle(px, py, sx, sy, 0.013)));
            }
            let glass = coverage(circle(px, py, lens.0, lens.1, lens_r));
            let halo = coverage(circle(px, py, lens.0, lens.1, lens_r + ring_w * 0.5 + 0.03));
            let ring = coverage((circle(px, py, lens.0, lens.1, lens_r)).abs() - ring_w / 2.0);
            let handle = coverage(capsule(px, py, (0.725, 0.705), (0.8, 0.78), 0.038));
            let gauge_track = coverage(arc(px, py, lens, 0.085, 0.034, 0.0, TAU));
            let gauge = coverage(arc(px, py, lens, 0.085, 0.034, -PI / 2.0, 1.45 * PI));
            if template {
                over([0.0; 3], drive * (1.0 - label * 0.75) * (1.0 - halo));
                over([0.0; 3], ring.max(handle));
                over([0.0; 3], gauge);
            } else {
                let tile = rounded_box(px, py, 0.5, 0.5, 0.4025, 0.4025, 0.18);
                let shadow = rounded_box(px, py, 0.5, 0.515, 0.4025, 0.4025, 0.18);
                over([0.0; 3], 0.22 * (1.0 - (shadow / 0.03).clamp(0.0, 1.0)));
                let g = ((py - 0.1) / 0.8).clamp(0.0, 1.0);
                let mix = |a: f32, b: f32| a + (b - a) * g;
                let mut rgb = [mix(0.93, 0.66), mix(0.17, 0.05), mix(0.2, 0.08)];
                let gloss = ((0.5 - py) / 0.4).clamp(0.0, 1.0) * 0.12;
                for c in &mut rgb {
                    *c += (1.0 - *c) * gloss;
                }
                over(rgb, coverage(tile));
                // Soft drop shadow under the drive and the glass.
                let drive_shadow = rounded_box(px, py, 0.425, 0.435, 0.245, 0.175, 0.035);
                over(
                    [0.0; 3],
                    0.18 * (1.0 - (drive_shadow / 0.025).clamp(0.0, 1.0)),
                );
                let body = 0.97 - (py - 0.25) * 0.12;
                over([body, body, body], drive);
                over([0.82, 0.83, 0.85], label);
                over([0.6, 0.62, 0.66], lines * label);
                over([0.7, 0.71, 0.74], screws);
                over([0.0; 3], 0.2 * halo * (1.0 - glass));
                over([0.11, 0.12, 0.14], glass);
                over([0.25, 0.27, 0.3], gauge_track);
                over([0.25, 0.85, 0.42], gauge);
                over([1.0; 3], ring.max(handle));
            }
            let alpha = out[3];
            let offset = (y * n + x) * 4;
            for i in 0..3 {
                let value = if alpha > 0.0 { out[i] / alpha } else { 0.0 };
                data[offset + i] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            data[offset + 3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    data
}
pub fn write_png(path: &std::path::Path, size: u32) -> anyhow::Result<()> {
    image::save_buffer_with_format(
        path,
        &rgba(size, false),
        size,
        size,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    Ok(())
}

pub fn write_icns(folder: &std::path::Path) -> anyhow::Result<()> {
    let mut chunks = Vec::new();
    for (kind, size) in [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024),
    ] {
        let png = std::fs::read(folder.join(format!("icon-{size}.png")))?;
        chunks.extend_from_slice(kind);
        chunks.extend_from_slice(&((png.len() + 8) as u32).to_be_bytes());
        chunks.extend(png);
    }
    let mut result = b"icns".to_vec();
    result.extend_from_slice(&((chunks.len() + 8) as u32).to_be_bytes());
    result.extend(chunks);
    std::fs::write(folder.join("Scope.icns"), result)?;
    Ok(())
}
