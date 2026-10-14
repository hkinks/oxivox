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

pub struct Overlay {
    config: OverlayConfig,
    theme: Theme,
    view: View,
    dots: Dots,
    shown_at: Instant,
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
            OverlayCmd::Show(view) => {
                if !self.is_visible() || view == View::WarmingUp {
                    self.shown_at = now;
                    self.dots.reset();
                }
                self.flash_until = matches!(view, View::Flash(_)).then(|| now + FLASH_DURATION);
                self.view = view;
            }
            OverlayCmd::Levels(levels) => self.dots.push(&levels),
            OverlayCmd::Theme(theme) => self.theme = theme,
        }
    }

    /// Advances animations and hides a flash once its time is up.
    pub fn tick(&mut self, now: Instant) {
        self.dots.animate(now);
        if self.flash_until.is_some_and(|until| now >= until) {
            self.flash_until = None;
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
            font: self.font.as_ref(),
        };
        draw::render(pixmap, &scene);
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
