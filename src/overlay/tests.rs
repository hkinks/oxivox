//! Drawing tests render into a pixmap, check pixels at known places and write PNGs to
//! `target/overlay-snapshots/` for looking at.

use super::*;
use crate::spectrogram::BANDS;
use crate::theme::Color;

fn overlay_with_font() -> Overlay {
    Overlay::new(OverlayConfig::default(), Theme::default(), text::load_system_font())
}

fn snapshot(name: &str, overlay: &Overlay, now: Instant, scale: u32) -> Pixmap {
    let (w, h) = overlay.size();
    let mut pixmap = Pixmap::new(w * scale, h * scale).unwrap();
    overlay.render(&mut pixmap, now);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/overlay-snapshots");
    std::fs::create_dir_all(&dir).unwrap();
    pixmap.save_png(dir.join(format!("{name}.png"))).unwrap();
    pixmap
}

fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let p = pixmap.pixel(x, y).unwrap();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

/// Column of mid-level energy in the lower bands, like a voice.
fn voice_columns(count: usize) -> Vec<Vec<f32>> {
    (0..count)
        .map(|i| {
            (0..BANDS)
                .map(|b| {
                    let envelope = 0.5 + 0.5 * (i as f32 / 9.0).sin();
                    (envelope * (1.0 - b as f32 / BANDS as f32) * 1.1).clamp(0.0, 1.0)
                })
                .collect()
        })
        .collect()
}

fn distance(a: [u8; 4], b: Color) -> i32 {
    (a[0] as i32 - b.r as i32).abs() + (a[1] as i32 - b.g as i32).abs() + (a[2] as i32 - b.b as i32).abs()
}

#[test]
fn hidden_overlay_is_not_visible() {
    assert!(!overlay_with_font().is_visible());
}

#[test]
fn body_is_rounded_and_themed() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::WarmingUp), now);
    let pm = snapshot("warming-up", &o, now, 1);
    assert_eq!(pixel(&pm, 0, 0)[3], 0, "corner is outside the rounded shape");
    assert_eq!(pixel(&pm, 180, 0)[3] > 200, true, "top border is drawn");
    let theme = Theme::default();
    assert!(distance(pixel(&pm, 180, 52), theme.background) < 12, "body shows the background colour");
}

#[test]
fn recording_draws_accent_where_there_is_energy() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::WarmingUp), now);
    let quiet = snapshot("recording-quiet", &o, now, 1);
    o.apply(OverlayCmd::Show(View::Recording), now);
    o.apply(OverlayCmd::Columns(voice_columns(200)), now);
    let loud = snapshot("recording", &o, now, 1);
    let theme = Theme::default();
    // Low-frequency rows at the right edge of the spectrogram are bright; the same place is plain background when warming up.
    let probe = (300, 36);
    assert!(distance(pixel(&loud, probe.0, probe.1), theme.surface) > 80);
    assert!(distance(pixel(&quiet, probe.0, probe.1), theme.background) < 12);
    // The state dot is accent coloured while recording.
    assert!(distance(pixel(&loud, 22, 28), theme.accent) < 12);
}

#[test]
fn newest_column_is_on_the_right() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    o.apply(OverlayCmd::Columns(voice_columns(5)), now);
    let pm = snapshot("recording-start", &o, now, 1);
    let theme = Theme::default();
    assert!(distance(pixel(&pm, 60, 36), theme.surface) < 12, "left side is still empty");
    assert!(distance(pixel(&pm, 340, 36), theme.surface) > 40, "right side has the new columns");
}

#[test]
fn transcribing_dims_the_frozen_spectrogram() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    o.apply(OverlayCmd::Columns(voice_columns(200)), now);
    let recording = snapshot("recording-before-stop", &o, now, 1);
    o.apply(OverlayCmd::Show(View::Transcribing), now);
    let transcribing = snapshot("transcribing", &o, now, 1);
    let theme = Theme::default();
    let probe = (200, 36);
    assert!(
        distance(pixel(&transcribing, probe.0, probe.1), theme.surface) < distance(pixel(&recording, probe.0, probe.1), theme.surface)
    );
}

#[test]
fn error_flash_has_error_border_and_expires() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(
        OverlayCmd::Show(View::Flash(FlashMessage { text: "no microphone".into(), kind: FlashKind::Error })),
        now,
    );
    let pm = snapshot("error-flash", &o, now, 1);
    let theme = Theme::default();
    assert!(distance(pixel(&pm, 180, 0), theme.error) < 40, "border turns to the error colour");
    assert!(distance(pixel(&pm, 22, 28), theme.error) < 12, "dot is the error colour");
    if o.font.is_some() {
        // Text pixels in the middle differ from the plain body.
        let plain = (30..330).any(|x| (20..36).any(|y| distance(pixel(&pm, x, y), theme.background) > 150));
        assert!(plain, "message text is drawn");
    }
    o.tick(now + Duration::from_millis(500));
    assert!(o.is_visible());
    o.tick(now + Duration::from_secs(2));
    assert!(!o.is_visible());
}

#[test]
fn info_flash_is_not_error_coloured() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(
        OverlayCmd::Show(View::Flash(FlashMessage { text: "nothing heard".into(), kind: FlashKind::Info })),
        now,
    );
    let pm = snapshot("nothing-heard", &o, now, 1);
    assert!(distance(pixel(&pm, 180, 0), Theme::default().error) > 100);
}

#[test]
fn scaled_buffer_has_same_layout() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    o.apply(OverlayCmd::Columns(voice_columns(200)), now);
    let pm = snapshot("recording-2x", &o, now, 2);
    assert_eq!((pm.width(), pm.height()), (720, 112));
    assert!(distance(pixel(&pm, 44, 56), Theme::default().accent) < 12);
}

#[test]
fn theme_change_applies() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    let orange = Theme::parse("accent = \"#ff9e64\"").unwrap();
    o.apply(OverlayCmd::Theme(orange.clone()), now);
    let pm = snapshot("recording-orange", &o, now, 1);
    assert!(distance(pixel(&pm, 22, 28), orange.accent) < 12);
}

#[test]
fn history_is_capped() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Columns(voice_columns(draw::VISIBLE_COLUMNS * 2)), now);
    assert_eq!(o.columns.len(), draw::VISIBLE_COLUMNS);
}
