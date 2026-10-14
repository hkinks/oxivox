//! Voice level to dot sizes: automatic gain, history that ripples outward from the centre, easing.

use std::collections::VecDeque;
use std::time::Instant;

/// Dots on each side of the centre one, so the row has `2 * HALF - 1` dots.
pub const HALF: usize = 10;
/// Level frames (16 ms) folded into one history step.
const FRAMES_PER_STEP: usize = 4;
const STEP_SECS: f32 = FRAMES_PER_STEP as f32 * 0.016;
/// The loudest recent step maps to a full-size dot; this much quieter maps to the smallest.
const RANGE_DB: f32 = 24.0;
/// The reference never drops below this, so room noise stays small even in a silent recording.
const MIN_REFERENCE_DB: f32 = -38.0;
const REFERENCE_DECAY_DB_PER_SEC: f32 = 3.0;
const ATTACK_SECS: f32 = 0.03;
const RELEASE_SECS: f32 = 0.12;

pub struct Dots {
    reference_db: f32,
    step_peak: f32,
    step_frames: usize,
    /// Newest first; each entry is 0..1 after gain.
    history: VecDeque<f32>,
    shown: [f32; HALF],
    last_animated: Option<Instant>,
}

impl Default for Dots {
    fn default() -> Self {
        Self {
            reference_db: MIN_REFERENCE_DB,
            step_peak: 0.0,
            step_frames: 0,
            history: VecDeque::new(),
            shown: [0.0; HALF],
            last_animated: None,
        }
    }
}

impl Dots {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Feeds RMS levels (linear, one per 16 ms frame).
    pub fn push(&mut self, levels: &[f32]) {
        for &level in levels {
            self.step_peak = self.step_peak.max(level);
            self.step_frames += 1;
            if self.step_frames == FRAMES_PER_STEP {
                self.finish_step();
            }
        }
    }

    fn finish_step(&mut self) {
        let db = 20.0 * self.step_peak.max(1e-6).log10();
        self.reference_db = (self.reference_db - REFERENCE_DECAY_DB_PER_SEC * STEP_SECS).max(MIN_REFERENCE_DB).max(db);
        let level = ((db - (self.reference_db - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0);
        self.history.push_front(level);
        self.history.truncate(HALF);
        self.step_peak = 0.0;
        self.step_frames = 0;
    }

    /// Moves the displayed sizes towards the history: quick to grow, slow to shrink.
    pub fn animate(&mut self, now: Instant) {
        let dt = self.last_animated.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32());
        self.last_animated = Some(now);
        for (k, shown) in self.shown.iter_mut().enumerate() {
            let target = self.history.get(k).copied().unwrap_or(0.0);
            let tau = if target > *shown { ATTACK_SECS } else { RELEASE_SECS };
            *shown += (target - *shown) * (1.0 - (-dt / tau).exp());
        }
    }

    /// Displayed size 0..1 per dot, centre first.
    pub fn sizes(&self) -> &[f32; HALF] {
        &self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn settle(dots: &mut Dots, from: Instant) -> Instant {
        let mut now = from;
        for _ in 0..60 {
            now += Duration::from_millis(16);
            dots.animate(now);
        }
        now
    }

    /// Frames of constant level, `secs` long.
    fn speech(level: f32, secs: f32) -> Vec<f32> {
        vec![level; (secs / 0.016 + 0.5) as usize]
    }

    #[test]
    fn quiet_headset_speech_fills_the_range() {
        // About -30 dBFS RMS, a normal level for speech in headset call mode.
        let mut dots = Dots::default();
        dots.push(&speech(0.03, 1.0));
        settle(&mut dots, Instant::now());
        assert!(dots.sizes()[0] > 0.9, "centre dot at {}", dots.sizes()[0]);
    }

    #[test]
    fn silence_stays_small_and_loud_after_quiet_adapts() {
        let mut dots = Dots::default();
        dots.push(&speech(1e-4, 1.0));
        settle(&mut dots, Instant::now());
        assert!(dots.sizes().iter().all(|&s| s < 0.05));
        // A very loud voice sets the reference, so a following normal voice is clearly smaller.
        dots.push(&speech(0.5, 1.0));
        dots.push(&speech(0.03, 1.0));
        settle(&mut dots, Instant::now());
        assert!(dots.sizes()[0] < 0.5);
    }

    #[test]
    fn reference_recovers_after_loud_burst() {
        let mut dots = Dots::default();
        dots.push(&speech(0.5, 0.3));
        dots.push(&speech(1e-4, 12.0));
        dots.push(&speech(0.03, 1.0));
        settle(&mut dots, Instant::now());
        assert!(dots.sizes()[0] > 0.9, "centre dot at {}", dots.sizes()[0]);
    }

    #[test]
    fn history_ripples_outward() {
        let mut dots = Dots::default();
        dots.push(&speech(0.1, 0.064));
        dots.push(&speech(1e-4, 0.064 * 5.0));
        settle(&mut dots, Instant::now());
        let sizes = dots.sizes();
        assert!(sizes[5] > 0.5 && sizes[0] < 0.1 && sizes[9] < 0.1, "{sizes:?}");
    }

    #[test]
    fn sizes_ease_instead_of_jumping() {
        let mut dots = Dots::default();
        let start = Instant::now();
        dots.animate(start);
        dots.push(&speech(0.1, 0.064));
        dots.animate(start + Duration::from_millis(10));
        assert!(dots.sizes()[0] > 0.0 && dots.sizes()[0] < 0.6);
    }
}
