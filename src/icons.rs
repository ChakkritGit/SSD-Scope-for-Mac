//! Original vector-style disk icon rasterized from geometry, shared by app and menu bar.
pub fn rgba(size: u32, template: bool) -> Vec<u8> {
    let mut data = vec![0; size as usize * size as usize * 4];
    for y in 0..size {
        for x in 0..size {
            let px = (x as f32 + 0.5) / size as f32;
            let py = (y as f32 + 0.5) / size as f32;
            let round = |cx: f32, cy: f32, w: f32, h: f32, r: f32| {
                let dx = (px - cx).abs() - (w / 2.0 - r);
                let dy = (py - cy).abs() - (h / 2.0 - r);
                dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) <= r
            };
            let outer = round(0.5, 0.5, 0.68, 0.65, 0.13);
            let inner = round(0.5, 0.48, 0.53, 0.40, 0.07);
            let dot = (px - 0.67).hypot(py - 0.70) < 0.035;
            let line = (0.30..0.49).contains(&px) && (0.67..0.72).contains(&py);
            let color = if template {
                if (outer && !inner) || dot || line {
                    [0, 0, 0, 255]
                } else {
                    [0, 0, 0, 0]
                }
            } else if dot || line {
                [187, 243, 208, 255]
            } else if inner {
                [28, 92, 72, 255]
            } else if outer {
                [41, 139, 101, 255]
            } else if round(0.5, 0.5, 0.95, 0.95, 0.23) {
                [236, 246, 240, 255]
            } else {
                [0, 0, 0, 0]
            };
            let offset = (y as usize * size as usize + x as usize) * 4;
            data[offset..offset + 4].copy_from_slice(&color);
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
