//! Draws the overlay into a pixel buffer. Pure: same scene, same pixels.

use fontdue::Font;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

use super::dots::HALF;
use super::{FlashKind, View};
use crate::theme::{Color, Theme};

const DOT_SPACING: f32 = 17.0;
const DOT_MIN_RADIUS: f32 = 2.4;
const DOT_MAX_RADIUS: f32 = 7.2;
/// Wave period while transcribing.
const WAVE_SECS: f32 = 1.6;
const FLASH_DOT_X: f32 = 22.0;
const TEXT_LEFT: f32 = 40.0;
const TEXT_RIGHT_MARGIN: f32 = 14.0;

pub struct Scene<'a> {
    /// Logical size; the pixmap is larger by the output scale (possibly fractional).
    pub size: (u32, u32),
    pub radius: f32,
    pub theme: &'a Theme,
    pub view: &'a View,
    /// Dot sizes 0..1, centre first.
    pub dots: &'a [f32; HALF],
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
    let k = r * 0.552_284_8;
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

pub fn render(pixmap: &mut Pixmap, scene: &Scene) {
    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    let scale = pixmap.width() as f32 / scene.size.0 as f32;
    let tf = Transform::from_scale(scale, scale);
    let (w, h) = (scene.size.0 as f32, scene.size.1 as f32);
    let theme = scene.theme;

    let flash_kind = if let View::Flash(f) = scene.view { Some(f.kind) } else { None };
    // The body is drawn in device pixels so the border is a whole number of them and stays crisp.
    let border_width = scale.round().max(1.0);
    let (dw, dh) = (pixmap.width() as f32, pixmap.height() as f32);
    let inset = border_width / 2.0;
    if let Some(body) = rounded_rect(inset, inset, dw - border_width, dh - border_width, scene.radius * scale) {
        pixmap.fill_path(&body, &paint(theme.background), FillRule::Winding, Transform::identity(), None);
        let border = if flash_kind == Some(FlashKind::Error) { theme.error } else { theme.border };
        let stroke = Stroke { width: border_width, ..Stroke::default() };
        pixmap.stroke_path(&body, &paint(border), &stroke, Transform::identity(), None);
    }

    match scene.view {
        View::Hidden => {}
        View::WarmingUp => draw_row(pixmap, tf, w, h, |_| (0.0, theme.text.with_alpha(0.28))),
        View::Recording => draw_row(pixmap, tf, w, h, |i| {
            let v = scene.dots[i.abs_diff(HALF - 1)];
            let colour = theme.accent.mix(theme.text, ((v - 0.7) / 0.3).clamp(0.0, 1.0) * 0.6);
            (v, colour.with_alpha(0.3 + 0.7 * v.sqrt()))
        }),
        View::Transcribing => draw_row(pixmap, tf, w, h, |i| {
            // A pulse travelling from left to right through the row.
            let phase = scene.time / WAVE_SECS - i as f32 / (2 * HALF - 1) as f32;
            let wave = (0.5 + 0.5 * (std::f32::consts::TAU * phase).sin()).powi(3);
            let v = 0.55 * wave;
            (v, theme.accent.with_alpha(0.3 + 0.55 * wave))
        }),
        View::Flash(flash) => {
            let error = flash.kind == FlashKind::Error;
            let colour = if error { theme.error } else { theme.text.with_alpha(0.75) };
            let dot = if error { theme.error } else { theme.text.with_alpha(0.5) };
            if let Some(path) = PathBuilder::from_circle(FLASH_DOT_X, h / 2.0, 5.0) {
                pixmap.fill_path(&path, &paint(dot), FillRule::Winding, tf, None);
            }
            if let Some(font) = scene.font {
                let cx = TEXT_LEFT + (w - TEXT_LEFT - TEXT_RIGHT_MARGIN) / 2.0;
                super::text::draw_centered(
                    pixmap,
                    font,
                    &flash.text,
                    14.0 * scale,
                    cx * scale,
                    h / 2.0 * scale,
                    colour,
                );
            }
        }
    }
}

/// The row of dots; `style` gives the size 0..1 and colour of dot `i` counted from the left.
fn draw_row(pixmap: &mut Pixmap, tf: Transform, w: f32, h: f32, style: impl Fn(usize) -> (f32, Color)) {
    for i in 0..2 * HALF - 1 {
        let (size, colour) = style(i);
        let x = w / 2.0 + (i as f32 - (HALF - 1) as f32) * DOT_SPACING;
        let radius = DOT_MIN_RADIUS + (DOT_MAX_RADIUS - DOT_MIN_RADIUS) * size.clamp(0.0, 1.0).powf(0.8);
        if size > 0.35
            && let Some(glow) = PathBuilder::from_circle(x, h / 2.0, radius + 1.2)
        {
            pixmap.fill_path(&glow, &paint(colour.with_alpha(0.16 * size)), FillRule::Winding, tf, None);
        }
        if let Some(dot) = PathBuilder::from_circle(x, h / 2.0, radius) {
            pixmap.fill_path(&dot, &paint(colour), FillRule::Winding, tf, None);
        }
    }
}
