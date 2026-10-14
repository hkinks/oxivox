//! `rustle overlay-demo`: walks the overlay through its states with synthetic audio, no microphone or model.

use anyhow::Result;
use std::time::Duration;

use crate::config::Config;
use crate::overlay::{FlashKind, FlashMessage, Overlay, OverlayCmd, Surface, View, text};
use crate::paths;
use crate::platform::wayland::LayerShellSurface;
use crate::spectrogram::{Analyzer, HOP, SAMPLE_RATE};
use crate::theme::Theme;

/// A wandering pair of tones with a slow syllable-like envelope.
fn voice(sample: usize) -> f32 {
    let t = sample as f32 / SAMPLE_RATE as f32;
    let envelope = (0.5 + 0.5 * (t * 7.0).sin()).powi(2);
    let pitch = 180.0 + 60.0 * (t * 2.3).sin();
    let tau = std::f32::consts::TAU;
    envelope
        * 0.15
        * ((tau * pitch * t).sin()
            + 0.6 * (tau * pitch * 3.0 * t).sin()
            + 0.4 * (tau * (900.0 + 400.0 * (t * 1.7).sin()) * t).sin())
}

pub fn run(record_secs: f32) -> Result<()> {
    let config = Config::load_default()?;
    let theme = Theme::load(&paths::config_dir().join("theme.toml")).unwrap_or_default();
    let sink = Box::new(LayerShellSurface).spawn(Overlay::new(config.overlay, theme, text::load_system_font()))?;

    sink.send(OverlayCmd::Show(View::WarmingUp));
    std::thread::sleep(Duration::from_millis(1200));
    sink.send(OverlayCmd::Show(View::Recording));

    let mut analyzer = Analyzer::new();
    let mut sample = 0;
    let frames = (record_secs * SAMPLE_RATE as f32) as usize / HOP;
    for _ in 0..frames {
        let chunk: Vec<f32> = (sample..sample + HOP).map(voice).collect();
        sample += HOP;
        sink.send(OverlayCmd::Columns(analyzer.push(&chunk)));
        std::thread::sleep(Duration::from_secs_f32(HOP as f32 / SAMPLE_RATE as f32));
    }

    sink.send(OverlayCmd::Show(View::Transcribing));
    std::thread::sleep(Duration::from_millis(2500));
    let error = FlashMessage { text: "no microphone".into(), kind: FlashKind::Error };
    sink.send(OverlayCmd::Show(View::Flash(error)));
    std::thread::sleep(Duration::from_millis(2000));
    Ok(())
}
