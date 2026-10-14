//! Draws the overlay into a pixel buffer. Pure: same scene, same pixels.

use fontdue::Font;
use std::collections::VecDeque;
use tiny_skia::{
    FillRule, FilterQuality, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pattern, Pixmap,
    PremultipliedColorU8, SpreadMode, Stroke, Transform,
};

use super::{FlashKind, View};
use crate::spectrogram::{BANDS, HOP, SAMPLE_RATE};
use crate::theme::{Color, Theme};

/// Seconds of history shown.
pub const VISIBLE_SECS: f32 = 3.0;
pub const VISIBLE_COLUMNS: usize = (VISIBLE_SECS * SAMPLE_RATE as f32 / HOP as f32) as usize;

const DOT_X: f32 = 22.0;
const REGION_LEFT: f32 = 40.0;
const REGION_RIGHT_MARGIN: f32 = 14.0;
const REGION_VERTICAL_MARGIN: f32 = 10.0;

pub struct Scene<'a> {
    /// Logical size; the pixmap may be larger by an integer scale.
    pub size: (u32, u32),
    pub radius: f32,
    pub theme: &'a Theme,
    pub view: &'a View,
    pub columns: &'a VecDeque<Vec<f32>>,
    /// Seconds since the overlay appeared, drives the animations.
    pub time: f32,
    pub font: Option<&'a Font>,
}

fn sk(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(sk(c));
    p.anti_alias = true;
    p
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let k = r * 0.552_284_75;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Intensity 0..1 to colour: surface, through the accent, towards the text colour for the peaks.
fn ramp(theme: &Theme, v: f32) -> Color {
    let base = theme.surface.mix(theme.accent, v.powf(0.85));
    if v > 0.8 { base.mix(theme.text, (v - 0.8) * 2.0) } else { base }
}

/// One pixel per column and band, high frequencies on top; scaled up with bilinear filtering when drawn.
fn spectrogram_image(theme: &Theme, columns: &VecDeque<Vec<f32>>, gain: f32) -> Option<Pixmap> {
    let mut image = Pixmap::new(VISIBLE_COLUMNS as u32, BANDS as u32)?;
    let shown = columns.len().min(VISIBLE_COLUMNS);
    let first = columns.len() - shown;
    let missing = VISIBLE_COLUMNS - shown;
    let pixels = image.pixels_mut();
    for x in 0..VISIBLE_COLUMNS {
        let column = x.checked_sub(missing).map(|i| &columns[first + i]);
        for band in 0..BANDS {
            let v = column.and_then(|c| c.get(band)).copied().unwrap_or(0.0) * gain;
            let c = ramp(theme, v);
            pixels[(BANDS - 1 - band) * VISIBLE_COLUMNS + x] = PremultipliedColorU8::from_rgba(c.r, c.g, c.b, 255)?;
        }
    }
    Some(image)
}

pub fn render(pixmap: &mut Pixmap, scene: &Scene) {
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    let scale = pixmap.width() as f32 / scene.size.0 as f32;
    let tf = Transform::from_scale(scale, scale);
    let (w, h) = (scene.size.0 as f32, scene.size.1 as f32);
    let theme = scene.theme;
    let erroring = matches!(scene.view, View::Flash(f) if f.kind == FlashKind::Error);

    if let Some(body) = rounded_rect(0.5, 0.5, w - 1.0, h - 1.0, scene.radius) {
        pixmap.fill_path(&body, &paint(theme.background), FillRule::Winding, tf, None);
        let border = if erroring { theme.error } else { theme.border };
        let stroke = Stroke { width: 1.0, ..Stroke::default() };
        pixmap.stroke_path(&body, &paint(border), &stroke, tf, None);
    }

    let region = (REGION_LEFT, REGION_VERTICAL_MARGIN, w - REGION_LEFT - REGION_RIGHT_MARGIN, h - 2.0 * REGION_VERTICAL_MARGIN);
    draw_dot(pixmap, scene, tf, h / 2.0);
    match scene.view {
        View::Hidden => {}
        View::WarmingUp => draw_flat_line(pixmap, scene, tf, region),
        View::Recording => draw_spectrogram(pixmap, scene, tf, region, 1.0),
        View::Transcribing => {
            draw_spectrogram(pixmap, scene, tf, region, 0.45);
            draw_sweep(pixmap, scene, tf, region);
        }
        View::Flash(flash) => {
            let colour = if flash.kind == FlashKind::Error { theme.error } else { theme.text.with_alpha(0.75) };
            if let Some(font) = scene.font {
                let (cx, cy) = (region.0 + region.2 / 2.0, h / 2.0);
                super::text::draw_centered(pixmap, font, &flash.text, 14.0 * scale, cx * scale, cy * scale, colour);
            }
        }
    }
}

fn draw_dot(pixmap: &mut Pixmap, scene: &Scene, tf: Transform, cy: f32) {
    let theme = scene.theme;
    let pulse = (scene.time * std::f32::consts::TAU / 1.4).sin() * 0.5 + 0.5;
    let (colour, halo) = match scene.view {
        View::Hidden => return,
        View::WarmingUp => (theme.text.with_alpha(0.3 + 0.3 * pulse), None),
        View::Recording => (theme.accent, Some((theme.accent.with_alpha(0.28 * (1.0 - pulse)), 5.0 + 4.0 * pulse))),
        View::Transcribing => (theme.accent.mix(theme.text, 0.5), Some((theme.accent.with_alpha(0.2 * pulse), 5.0 + 3.0 * pulse))),
        View::Flash(f) if f.kind == FlashKind::Error => (theme.error, None),
        View::Flash(_) => (theme.text.with_alpha(0.5), None),
    };
    if let Some((halo_colour, radius)) = halo {
        if let Some(path) = PathBuilder::from_circle(DOT_X, cy, radius) {
            pixmap.fill_path(&path, &paint(halo_colour), FillRule::Winding, tf, None);
        }
    }
    if let Some(path) = PathBuilder::from_circle(DOT_X, cy, 5.0) {
        pixmap.fill_path(&path, &paint(colour), FillRule::Winding, tf, None);
    }
}

type Region = (f32, f32, f32, f32);

fn draw_flat_line(pixmap: &mut Pixmap, scene: &Scene, tf: Transform, (x, y, w, h): Region) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y + h / 2.0);
    pb.line_to(x + w, y + h / 2.0);
    if let Some(path) = pb.finish() {
        let stroke = Stroke { width: 1.5, line_cap: tiny_skia::LineCap::Round, ..Stroke::default() };
        pixmap.stroke_path(&path, &paint(scene.theme.text.with_alpha(0.3)), &stroke, tf, None);
    }
}

fn draw_spectrogram(pixmap: &mut Pixmap, scene: &Scene, tf: Transform, (x, y, w, h): Region, gain: f32) {
    let Some(path) = rounded_rect(x, y, w, h, 6.0) else { return };
    pixmap.fill_path(&path, &paint(scene.theme.surface), FillRule::Winding, tf, None);
    let Some(image) = spectrogram_image(scene.theme, scene.columns, gain) else { return };
    let to_region = Transform::from_row(w / VISIBLE_COLUMNS as f32, 0.0, 0.0, h / BANDS as f32, x, y);
    let mut p = Paint { anti_alias: true, ..Paint::default() };
    p.shader = Pattern::new(image.as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, 1.0, to_region);
    pixmap.fill_path(&path, &p, FillRule::Winding, tf, None);
}

/// A soft highlight running across the frozen spectrogram.
fn draw_sweep(pixmap: &mut Pixmap, scene: &Scene, tf: Transform, (x, y, w, h): Region) {
    let Some(path) = rounded_rect(x, y, w, h, 6.0) else { return };
    let band = 90.0;
    let cycle = (scene.time / 1.3).fract();
    let start = x - band + cycle * (w + band);
    let accent = scene.theme.accent;
    let stops = [
        GradientStop::new(0.0, sk(accent.with_alpha(0.0))),
        GradientStop::new(0.6, sk(accent.with_alpha(0.35))),
        GradientStop::new(1.0, sk(accent.with_alpha(0.0))),
    ];
    let shader = LinearGradient::new(
        tiny_skia::Point::from_xy(start, 0.0),
        tiny_skia::Point::from_xy(start + band, 0.0),
        stops.to_vec(),
        SpreadMode::Pad,
        Transform::identity(),
    );
    if let Some(shader) = shader {
        let p = Paint { shader, anti_alias: true, ..Paint::default() };
        pixmap.fill_path(&path, &p, FillRule::Winding, tf, None);
    }
}
