//! Minimal text rendering for the overlay's one-line messages.

use fontdue::{Font, FontSettings};
use tiny_skia::Pixmap;

use crate::theme::Color;

/// The system sans-serif font through fontconfig, with a few well-known paths as a fallback.
pub fn load_system_font() -> Option<Font> {
    let from_fontconfig = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "sans-serif:weight=medium"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok());
    let fallbacks = [
        "/run/current-system/sw/share/X11/fonts/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ];
    from_fontconfig
        .into_iter()
        .chain(fallbacks.map(String::from))
        .filter_map(|path| std::fs::read(path).ok())
        .find_map(|bytes| Font::from_bytes(bytes, FontSettings::default()).ok())
}

pub fn text_width(font: &Font, text: &str, px: f32) -> f32 {
    text.chars().map(|c| font.metrics(c, px).advance_width).sum()
}

/// Draws `text` centred on (`cx`, `cy`). Coordinates and `px` are in pixmap pixels.
pub fn draw_centered(pixmap: &mut Pixmap, font: &Font, text: &str, px: f32, cx: f32, cy: f32, color: Color) {
    let Some(line) = font.horizontal_line_metrics(px) else { return };
    let baseline = cy + (line.ascent + line.descent) / 2.0;
    let mut pen = cx - text_width(font, text, px) / 2.0;
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let data = pixmap.data_mut();
    for c in text.chars() {
        let (metrics, coverage) = font.rasterize(c, px);
        let left = (pen + metrics.xmin as f32).round() as i32;
        let top = (baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let (x, y) = (left + col as i32, top + row as i32);
                if x < 0 || y < 0 || x >= width || y >= height {
                    continue;
                }
                let alpha = coverage[row * metrics.width + col] as f32 / 255.0 * color.a as f32 / 255.0;
                let pixel = &mut data[((y * width + x) * 4) as usize..][..4];
                for (dst, src) in pixel.iter_mut().zip([color.r, color.g, color.b, 255]) {
                    // Premultiplied source-over.
                    *dst = (src as f32 * alpha + *dst as f32 * (1.0 - alpha)).round() as u8;
                }
            }
        }
        pen += metrics.advance_width;
    }
}
