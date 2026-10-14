//! `rustle overlay-demo`: walks the overlay through its states with synthetic audio, no microphone or model.

use anyhow::Result;
use std::time::Duration;

use crate::audio::{FrameMeter, LEVEL_FRAME, SAMPLE_RATE};
use crate::config::Config;
use crate::overlay::{FlashKind, FlashMessage, Overlay, OverlayCmd, Surface, View, text};
use crate::paths;
use crate::platform::wayland::LayerShellSurface;
use crate::theme::Theme;

/// Syllable-like bursts with pauses, at the level of headset speech in call mode (peaks around -15 dBFS).
fn voice(sample: usize) -> f32 {
    let t = sample as f32 / SAMPLE_RATE as f32;
    let syllables = (0.5 + 0.5 * (t * 9.0).sin()).powi(2);
    let phrase = (0.5 + 0.5 * (t * 1.3).sin()).powf(0.5);
    let tau = std::f32::consts::TAU;
    syllables
        * phrase
        * 0.18
        * ((tau * 190.0 * t).sin() + 0.5 * (tau * 570.0 * t).sin() + 0.3 * (tau * 1300.0 * t).sin())
}

pub fn run(record_secs: f32) -> Result<()> {
    let config = Config::load_default()?;
    let theme = Theme::load(&paths::config_dir().join("theme.toml")).unwrap_or_default();
    let sink = Box::new(LayerShellSurface).spawn(Overlay::new(config.overlay, theme, text::load_system_font()))?;

    sink.send(OverlayCmd::Show(View::WarmingUp));
    std::thread::sleep(Duration::from_millis(1200));
    sink.send(OverlayCmd::Show(View::Recording));

    let mut meter = FrameMeter::default();
    let mut sample = 0;
    let frames = (record_secs * SAMPLE_RATE as f32) as usize / LEVEL_FRAME;
    for _ in 0..frames {
        let chunk: Vec<f32> = (sample..sample + LEVEL_FRAME).map(voice).collect();
        sample += LEVEL_FRAME;
        sink.send(OverlayCmd::Levels(meter.push(&chunk)));
        std::thread::sleep(Duration::from_secs_f32(LEVEL_FRAME as f32 / SAMPLE_RATE as f32));
    }

    sink.send(OverlayCmd::Show(View::Transcribing));
    std::thread::sleep(Duration::from_millis(2500));
    let error = FlashMessage { text: "no microphone".into(), kind: FlashKind::Error };
    sink.send(OverlayCmd::Show(View::Flash(error)));
    std::thread::sleep(Duration::from_millis(2000));
    Ok(())
}
