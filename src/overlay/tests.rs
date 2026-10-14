//! Drawing tests render into a pixmap, check pixels at known places and write PNGs to
//! `target/overlay-snapshots/` for looking at.

use super::*;
use crate::theme::Color;

fn overlay_with_font() -> Overlay {
    Overlay::new(OverlayConfig::default(), Theme::default(), text::load_system_font())
}

/// Long enough after `Show` for the fade-in to be over.
const SETTLED: Duration = Duration::from_millis(200);

fn snapshot(name: &str, overlay: &Overlay, now: Instant, scale: f32) -> Pixmap {
    let (w, h) = overlay.size();
    let mut pixmap = Pixmap::new((w as f32 * scale).round() as u32, (h as f32 * scale).round() as u32).unwrap();
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

fn distance(a: [u8; 4], b: Color) -> i32 {
    (a[0] as i32 - b.r as i32).abs() + (a[1] as i32 - b.g as i32).abs() + (a[2] as i32 - b.b as i32).abs()
}

/// Feeds speech-like levels (about -28 dBFS RMS with pauses) and lets the animation settle.
fn speak(overlay: &mut Overlay, mut now: Instant, steps: usize) -> Instant {
    for i in 0..steps {
        let level = if i % 7 < 5 { 0.04 * (1.0 + (i % 3) as f32 * 0.4) } else { 0.002 };
        overlay.apply(OverlayCmd::Levels(vec![level; 4]), now);
        for _ in 0..4 {
            now += Duration::from_millis(16);
            overlay.tick(now);
        }
    }
    now
}

const CENTRE: (u32, u32) = (180, 28);

fn dot_x(index_from_centre: i32) -> u32 {
    (180 + index_from_centre * 17) as u32
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
    let pm = snapshot("warming-up", &o, now + SETTLED, 1.0);
    assert_eq!(pixel(&pm, 0, 0)[3], 0, "corner is outside the rounded shape");
    assert!(pixel(&pm, 180, 0)[3] > 200, "top border is drawn");
    assert!(distance(pixel(&pm, 20, 28), Theme::default().background) < 12, "body shows the background colour");
}

#[test]
fn warming_up_is_a_row_of_small_neutral_dots_that_breathe() {
    let now = Instant::now();
    let o = warming_up(now);
    let dim = snapshot("warming-up", &o, now + SETTLED, 1.0);
    let bright = snapshot("warming-up-breath", &o, now + Duration::from_millis(1200), 1.0);
    let theme = Theme::default();
    // Dots exist at the expected places, between them it is background.
    assert!(distance(pixel(&dim, dot_x(0), 28), theme.background) > 40);
    assert!(distance(pixel(&dim, dot_x(0) + 8, 28), theme.background) < 12);
    // ...and they are small: nothing above or below the centre line.
    assert!(distance(pixel(&bright, dot_x(0), 20), theme.background) < 12);
    // The breath changes only the opacity, and the colour stays the neutral text colour, never the accent.
    let (a, b) = (
        distance(pixel(&dim, dot_x(0), 28), theme.background),
        distance(pixel(&bright, dot_x(0), 28), theme.background),
    );
    assert!(b > a + 20, "breath brightens the dot ({a} -> {b})");
    // Taking the opacity from the red channel, the other channels match the text colour, not the accent.
    let p = pixel(&bright, dot_x(0), 28);
    let (bg, t) = (theme.background, theme.text);
    let a = (p[0] as f32 - bg.r as f32) / (t.r as f32 - bg.r as f32);
    let expected_green = bg.g as f32 + a * (t.g as f32 - bg.g as f32);
    assert!((p[1] as f32 - expected_green).abs() < 6.0, "warming up is neutral text colour ({p:?})");
}

fn warming_up(now: Instant) -> Overlay {
    let mut o = overlay_with_font();
    o.apply(OverlayCmd::Show(View::WarmingUp), now);
    o
}

/// Warming up, then the microphone goes live; returns the overlay and that moment.
fn going_live(now: Instant) -> (Overlay, Instant) {
    let mut o = warming_up(now);
    let live = now + Duration::from_millis(1500);
    o.apply(OverlayCmd::Show(View::Recording), live);
    (o, live)
}

#[test]
fn going_live_pings_outward_once_and_turns_the_dots_to_the_accent() {
    let now = Instant::now();
    let (o, live) = going_live(now);
    let theme = Theme::default();
    let before = snapshot("live-before", &warming_up(now), live, 1.0);
    let tall_at = |pm: &Pixmap, i: i32| distance(pixel(pm, dot_x(i), 28 - 4), theme.background) > 40;

    // Early in the ping the dots next to the centre have grown; the outer dots have not been reached.
    let early = snapshot("live-ping-early", &o, live + Duration::from_millis(40), 1.0);
    assert!((tall_at(&early, 1) || tall_at(&early, 2)) && !tall_at(&early, 8));
    // Mid-way the front is away from the centre, on both sides equally, and the centre is back down.
    let mid = snapshot("live-ping-mid", &o, live + Duration::from_millis(150), 1.0);
    assert!(tall_at(&mid, 4) && tall_at(&mid, -4) && !tall_at(&mid, 0) && !tall_at(&mid, 9));
    // After it the row is calm again, every dot the accent colour at base size.
    let after = snapshot("live-silent", &o, live + Duration::from_millis(600), 1.0);
    for i in -9..=9 {
        assert!(!tall_at(&after, i), "dot {i} still big after the ping");
        let p = pixel(&after, dot_x(i), 28);
        assert!(distance(p, theme.accent) < 40, "dot {i} is accent ({p:?})");
    }
    // Before going live they were neutral.
    assert!(distance(pixel(&before, dot_x(0), 28), theme.accent) > 100);
    // The ping happens once: later frames are identical.
    let later = snapshot("live-silent-later", &o, live + Duration::from_millis(900), 1.0);
    assert_eq!(after.data(), later.data());
}

#[test]
fn repeated_recording_commands_do_not_ping_again() {
    let now = Instant::now();
    let (mut o, live) = going_live(now);
    let late = live + Duration::from_millis(800);
    let a = snapshot("live-no-reping-a", &o, late, 1.0);
    o.apply(OverlayCmd::Show(View::Recording), late);
    let b = snapshot("live-no-reping-b", &o, late, 1.0);
    assert_eq!(a.data(), b.data());
}

#[test]
fn speech_makes_the_dots_react_with_the_accent_colour() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    let silent = snapshot("recording-silent", &o, now + SETTLED, 1.0);
    let end = speak(&mut o, now, 60);
    let loud = snapshot("recording", &o, end, 1.0);
    let theme = Theme::default();
    // Some dot has grown tall: pixels well above the centre line are now accent-ish.
    let tall = (0..19).any(|i| {
        let x = dot_x(i - 9);
        (0..6).any(|dy| {
            distance(pixel(&loud, x, 28 - 7 - dy), theme.accent) < 120
                && distance(pixel(&loud, x, 28 - 7 - dy), theme.background) > 80
        })
    });
    assert!(tall, "no dot grew with speech");
    assert!(
        silent.data().iter().zip(loud.data()).filter(|(a, b)| a != b).count() > 500,
        "the picture changed substantially"
    );
}

#[test]
fn recording_row_is_symmetric_about_the_centre() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    let end = speak(&mut o, now, 40);
    let pm = snapshot("recording-symmetry", &o, end, 1.0);
    for i in 1..9 {
        let (left, right) = (pixel(&pm, dot_x(-i), 22), pixel(&pm, dot_x(i), 22));
        assert_eq!(left, right, "dot {i} differs between the sides");
    }
}

#[test]
fn quiet_room_noise_stays_calm() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    let mut t = now;
    for _ in 0..80 {
        o.apply(OverlayCmd::Levels(vec![0.0005; 4]), t);
        t += Duration::from_millis(64);
        o.tick(t);
    }
    let pm = snapshot("recording-noise", &o, t, 1.0);
    assert!(distance(pixel(&pm, dot_x(0), 22), Theme::default().background) < 12, "dots stay at their smallest");
}

#[test]
fn transcribing_wave_moves_and_stays_clean() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    let end = speak(&mut o, now, 30);
    o.apply(OverlayCmd::Show(View::Transcribing), end);
    let a = snapshot("transcribing", &o, end, 1.0);
    let b = snapshot("transcribing-later", &o, end + Duration::from_millis(400), 1.0);
    assert_ne!(a.data(), b.data(), "the wave travels");
    // The dots from speech do not linger: the wave does not depend on the voice.
    let mut quiet = overlay_with_font();
    quiet.apply(OverlayCmd::Show(View::Transcribing), now);
    let c = snapshot("transcribing-fresh", &quiet, now + (end - now), 1.0);
    assert_eq!(a.data(), c.data());
}

#[test]
fn error_flash_has_error_border_and_expires() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Flash(FlashMessage { text: "no microphone".into(), kind: FlashKind::Error })), now);
    let pm = snapshot("error-flash", &o, now + SETTLED, 1.0);
    let theme = Theme::default();
    assert!(distance(pixel(&pm, 180, 0), theme.error) < 40, "border turns to the error colour");
    assert!(distance(pixel(&pm, 22, 28), theme.error) < 12, "dot is the error colour");
    if o.font.is_some() {
        let text_drawn = (30..330).any(|x| (20..36).any(|y| distance(pixel(&pm, x, y), theme.background) > 150));
        assert!(text_drawn, "message text is drawn");
    }
    o.tick(now + Duration::from_millis(500));
    assert!(o.is_visible());
    o.tick(now + Duration::from_secs(2));
    assert!(o.is_visible(), "fades out first");
    o.tick(now + Duration::from_millis(2200));
    assert!(!o.is_visible());
}

#[test]
fn info_flash_is_not_error_coloured() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Flash(FlashMessage { text: "nothing heard".into(), kind: FlashKind::Info })), now);
    let pm = snapshot("nothing-heard", &o, now + SETTLED, 1.0);
    assert!(distance(pixel(&pm, 180, 0), Theme::default().error) > 100);
}

#[test]
fn scaled_buffer_has_same_layout() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::WarmingUp), now);
    let pm = snapshot("warming-up-2x", &o, now + SETTLED, 2.0);
    assert_eq!((pm.width(), pm.height()), (720, 112));
    assert!(distance(pixel(&pm, 2 * CENTRE.0, 2 * CENTRE.1), Theme::default().background) > 40);
}

#[test]
fn theme_change_applies() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    let end = speak(&mut o, now, 40);
    let orange = Theme::parse("accent = \"#ff9e64\"").unwrap();
    o.apply(OverlayCmd::Theme(orange.clone()), end);
    let pm = snapshot("recording-orange", &o, end, 1.0);
    let blue = Theme::default().accent;
    let tinted = (0..19).any(|i| {
        (10..46).any(|y| {
            distance(pixel(&pm, dot_x(i - 9), y), orange.accent) < distance(pixel(&pm, dot_x(i - 9), y), blue) - 100
        })
    });
    assert!(tinted);
}

#[test]
fn a_new_recording_starts_with_no_history() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    let end = speak(&mut o, now, 30);
    o.apply(OverlayCmd::Show(View::Hidden), end);
    o.apply(OverlayCmd::Show(View::WarmingUp), end);
    o.apply(OverlayCmd::Show(View::Recording), end);
    o.tick(end + Duration::from_millis(100));
    assert!(o.dots.sizes().iter().all(|&s| s == 0.0));
}

#[test]
fn fractional_scales_keep_the_layout_and_a_crisp_border() {
    let theme = Theme::default();
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        let mut o = overlay_with_font();
        let now = Instant::now();
        o.apply(OverlayCmd::Show(View::WarmingUp), now);
        let pm = snapshot(&format!("warming-up-{scale}x"), &o, now + SETTLED, scale);
        let (w, h) = (pm.width(), pm.height());
        assert_eq!((w, h), ((360.0 * scale).round() as u32, (56.0 * scale).round() as u32));
        // The border is a whole number of device pixels wide: the row of pixels inside it is the body again.
        let border = scale.round().max(1.0) as u32;
        let mid = w / 2;
        for y in 0..border {
            assert!(distance(pixel(&pm, mid, y), theme.border) < 12, "{scale}x: border row {y}");
        }
        assert!(distance(pixel(&pm, mid, border), theme.background) < 12, "{scale}x: border is not blurred");
        // The centre dot sits at the middle of the buffer.
        assert!(distance(pixel(&pm, w / 2, h / 2), theme.background) > 40, "{scale}x: centre dot");
    }
}

#[test]
fn full_volume_dots_do_not_touch() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::Recording), now);
    // A steady loud voice fills every dot after enough steps.
    let mut t = now;
    for _ in 0..40 {
        o.apply(OverlayCmd::Levels(vec![0.05; 4]), t);
        for _ in 0..4 {
            t += Duration::from_millis(16);
            o.tick(t);
        }
    }
    let pm = snapshot("recording-full", &o, t, 1.0);
    let theme = Theme::default();
    // Midway between two neighbours, on the centre line, is the body colour (the glow stays faint).
    for i in -8..8 {
        let between = dot_x(i) + 8;
        let gap = distance(pixel(&pm, between, 28), theme.background);
        assert!(gap < 90, "dots {i} and {} touch ({gap})", i + 1);
    }
}

#[test]
fn fades_in_and_out_and_stops_when_hidden() {
    let mut o = overlay_with_font();
    let now = Instant::now();
    o.apply(OverlayCmd::Show(View::WarmingUp), now);
    let alpha_at =
        |o: &Overlay, t| snapshot("fade", o, t, 1.0).data().iter().skip(3).step_by(4).map(|&a| a as u32).sum::<u32>();
    let (start, half, full) = (
        alpha_at(&o, now),
        alpha_at(&o, now + Duration::from_millis(60)),
        alpha_at(&o, now + Duration::from_millis(130)),
    );
    assert_eq!(start, 0, "nothing at the first instant");
    assert!(half > 0 && half < full, "partly visible half way");

    let t = now + Duration::from_secs(1);
    o.apply(OverlayCmd::Show(View::Hidden), t);
    assert!(o.is_visible(), "still fading out");
    let fading = alpha_at(&o, t + Duration::from_millis(75));
    assert!(fading > 0 && fading < full);
    o.tick(t + Duration::from_millis(160));
    assert!(!o.is_visible(), "hidden after the fade, so the surface can go");

    // Showing again while fading out turns around from the current opacity instead of jumping.
    let t = now + Duration::from_secs(2);
    o.apply(OverlayCmd::Show(View::Recording), t);
    o.apply(OverlayCmd::Show(View::Hidden), t + Duration::from_millis(200));
    o.apply(OverlayCmd::Show(View::Recording), t + Duration::from_millis(275));
    o.tick(t + Duration::from_millis(300));
    assert!(o.is_visible());
}
