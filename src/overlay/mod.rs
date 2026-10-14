//! The overlay model: what is shown and when. Portable; platform surfaces only present its pixels.

pub mod dots;
pub mod draw;
pub mod text;

use anyhow::Result;
use fontdue::Font;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

use crate::config::OverlayConfig;
use crate::theme::Theme;
use dots::Dots;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashKind {
    Error,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashMessage {
    pub text: String,
    pub kind: FlashKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Hidden,
    WarmingUp,
    Recording,
    Transcribing,
    Flash(FlashMessage),
}

#[derive(Debug, Clone)]
pub enum OverlayCmd {
    Show(View),
    /// RMS levels of new 16 ms audio frames, oldest first.
    Levels(Vec<f32>),
    Theme(Theme),
}

const FLASH_DURATION: Duration = Duration::from_millis(1600);
const FADE_IN: Duration = Duration::from_millis(120);
const FADE_OUT: Duration = Duration::from_millis(150);
/// Size of the overlay when fully transparent, as a fraction of its final size.
const POP_SCALE: f32 = 0.94;

/// Opacity of the overlay moving linearly from `from` to `to`; a reversal starts from wherever it is.
#[derive(Debug, Clone, Copy)]
struct Fade {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Fade {
    fn value(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * t.clamp(0.0, 1.0)
    }

    fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.duration
    }
}

pub struct Overlay {
    config: OverlayConfig,
    theme: Theme,
    view: View,
    dots: Dots,
    shown_at: Instant,
    /// When the view last became `Recording`, i.e. the microphone went live.
    live_at: Option<Instant>,
    fade: Fade,
    /// The view stays on screen, fading out, until the fade ends.
    hiding: bool,
    flash_until: Option<Instant>,
    font: Option<Font>,
}

impl Overlay {
    pub fn new(config: OverlayConfig, theme: Theme, font: Option<Font>) -> Self {
        Self {
            config,
            theme,
            view: View::Hidden,
            dots: Dots::default(),
            shown_at: Instant::now(),
            live_at: None,
            fade: Fade { from: 0.0, to: 0.0, start: Instant::now(), duration: FADE_IN },
            hiding: false,
            flash_until: None,
            font,
        }
    }

    pub fn config(&self) -> &OverlayConfig {
        &self.config
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn is_visible(&self) -> bool {
        self.view != View::Hidden
    }

    pub fn apply(&mut self, cmd: OverlayCmd, now: Instant) {
        match cmd {
            OverlayCmd::Show(View::Hidden) => self.start_hiding(now),
            OverlayCmd::Show(view) => {
                let appearing = !self.is_visible() || self.hiding;
                if !self.is_visible() || view == View::WarmingUp {
                    self.shown_at = now;
                    self.dots.reset();
                }
                if appearing {
                    self.fade = Fade { from: self.opacity(now), to: 1.0, start: now, duration: FADE_IN };
                    self.hiding = false;
                }
                // Going live is the change into recording; further `Recording` commands keep the original moment.
                let was_recording = self.view == View::Recording;
                self.live_at = (view == View::Recording).then(|| self.live_at.filter(|_| was_recording).unwrap_or(now));
                self.flash_until = matches!(view, View::Flash(_)).then(|| now + FLASH_DURATION);
                self.view = view;
            }
            OverlayCmd::Levels(levels) => self.dots.push(&levels),
            OverlayCmd::Theme(theme) => self.theme = theme,
        }
    }

    fn start_hiding(&mut self, now: Instant) {
        self.flash_until = None;
        if self.is_visible() && !self.hiding {
            self.fade = Fade { from: self.opacity(now), to: 0.0, start: now, duration: FADE_OUT };
            self.hiding = true;
        }
    }

    /// 0 when fully transparent, 1 when fully shown.
    fn opacity(&self, now: Instant) -> f32 {
        if self.is_visible() { self.fade.value(now) } else { 0.0 }
    }

    /// Advances animations; a flash starts fading out once its time is up, and a finished fade-out hides the view.
    pub fn tick(&mut self, now: Instant) {
        self.dots.animate(now);
        if self.flash_until.is_some_and(|until| now >= until) {
            self.start_hiding(now);
        }
        if self.hiding && self.fade.done(now) {
            self.hiding = false;
            self.view = View::Hidden;
        }
    }

    pub fn render(&self, pixmap: &mut Pixmap, now: Instant) {
        let scene = draw::Scene {
            size: self.size(),
            radius: self.config.radius,
            theme: &self.theme,
            view: &self.view,
            dots: self.dots.sizes(),
            time: now.saturating_duration_since(self.shown_at).as_secs_f32(),
            live: self.live_at.map(|t| now.saturating_duration_since(t).as_secs_f32()),
            font: self.font.as_ref(),
        };
        let opacity = self.opacity(now);
        if opacity >= 1.0 {
            draw::render(pixmap, &scene);
            return;
        }
        // Fading: draw the finished picture off screen, then place it shrunken and translucent.
        let Some(mut full) = Pixmap::new(pixmap.width(), pixmap.height()) else { return };
        draw::render(&mut full, &scene);
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        let eased = 1.0 - (1.0 - opacity).powi(2);
        let scale = POP_SCALE + (1.0 - POP_SCALE) * eased;
        let (cx, cy) = (pixmap.width() as f32 / 2.0, pixmap.height() as f32 / 2.0);
        let paint =
            tiny_skia::PixmapPaint { opacity, quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
        let tf = tiny_skia::Transform::from_translate(cx, cy).pre_scale(scale, scale).pre_translate(-cx, -cy);
        pixmap.draw_pixmap(0, 0, full.as_ref(), &paint, tf, None);
    }
}

/// Where overlay commands go. The sink may be on another thread.
pub trait OverlaySink: Send + Sync {
    fn send(&self, cmd: OverlayCmd);
}

/// Platform adapter that presents an [`Overlay`] on screen.
pub trait Surface {
    /// Starts presenting on its own thread and returns the handle that feeds it.
    fn spawn(self: Box<Self>, overlay: Overlay) -> Result<Arc<dyn OverlaySink>>;
}

#[cfg(test)]
mod tests;
