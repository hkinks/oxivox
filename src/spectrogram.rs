//! Short-time FFT turned into log-frequency bands, one column per hop.

use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::sync::Arc;

pub const SAMPLE_RATE: usize = 16_000;
const FFT_SIZE: usize = 1024;
/// 16 ms, so 3 s of history is about 190 columns.
pub const HOP: usize = 256;
pub const BANDS: usize = 32;
const F_MIN: f32 = 80.0;
const F_MAX: f32 = 8_000.0;
/// Level range mapped to 0..1, relative to full scale.
const DB_FLOOR: f32 = -75.0;
const DB_CEIL: f32 = -15.0;

pub struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    /// Fractional FFT bin range (low, high) covered by each band.
    ranges: Vec<(f32, f32)>,
    pending: Vec<f32>,
    scratch: Vec<Complex<f32>>,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    pub fn new() -> Self {
        let window = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos())
            .collect();
        let bin_hz = SAMPLE_RATE as f32 / FFT_SIZE as f32;
        let ratio = (F_MAX / F_MIN).powf(1.0 / (BANDS - 1) as f32);
        let half_step = ratio.sqrt();
        let ranges = (0..BANDS)
            .map(|b| {
                let centre = F_MIN * ratio.powi(b as i32) / bin_hz;
                (centre / half_step, centre * half_step)
            })
            .collect();
        Self {
            fft: FftPlanner::new().plan_fft_forward(FFT_SIZE),
            window,
            ranges,
            // Start with a full window of silence so the first column appears after one hop.
            pending: vec![0.0; FFT_SIZE - HOP],
            scratch: vec![Complex::default(); FFT_SIZE],
        }
    }

    /// Feeds samples and returns the columns completed by them, each `BANDS` values in 0..1, lowest band first.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.pending.extend_from_slice(samples);
        let mut columns = Vec::new();
        let mut start = 0;
        while self.pending.len() - start >= FFT_SIZE {
            columns.push(self.column(start));
            start += HOP;
        }
        self.pending.drain(..start);
        columns
    }

    fn column(&mut self, start: usize) -> Vec<f32> {
        let frame = &self.pending[start..start + FFT_SIZE];
        for (out, (s, w)) in self.scratch.iter_mut().zip(frame.iter().zip(&self.window)) {
            *out = Complex::new(s * w, 0.0);
        }
        self.fft.process(&mut self.scratch);
        // A full-scale sine under a Hann window peaks at FFT_SIZE / 4.
        let norm = 4.0 / FFT_SIZE as f32;
        let at = |i: usize| self.scratch[i.min(FFT_SIZE / 2)].norm() * norm;
        let magnitude = |(low, high): (f32, f32)| {
            let (first, last) = (low.ceil() as usize, high.floor() as usize);
            if last >= first {
                // Wide band: the loudest bin in it, so a tone between two band centres still shows.
                (first..=last).map(at).fold(0.0, f32::max)
            } else {
                // Narrower than a bin: interpolate so the low bands do not all read the same bin.
                let centre = (low + high) / 2.0;
                let (lo, frac) = (centre.floor() as usize, centre.fract());
                at(lo) * (1.0 - frac) + at(lo + 1) * frac
            }
        };
        self.ranges
            .iter()
            .enumerate()
            .map(|(band, &range)| {
                // Slight upward tilt: speech energy falls off towards the high bands.
                let tilt = 6.0 * band as f32 / (BANDS - 1) as f32;
                let db = 20.0 * magnitude(range).max(1e-9).log10() + tilt;
                ((db - DB_FLOOR) / (DB_CEIL - DB_FLOOR)).clamp(0.0, 1.0)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(SAMPLE_RATE as f32 * seconds) as usize)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * freq * i as f32 / SAMPLE_RATE as f32).sin())
            .collect()
    }

    fn band_freq(band: usize) -> f32 {
        F_MIN * (F_MAX / F_MIN).powf(band as f32 / (BANDS - 1) as f32)
    }

    fn loudest(column: &[f32]) -> usize {
        column.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0
    }

    #[test]
    fn column_count_follows_hop() {
        let mut a = Analyzer::new();
        let columns = a.push(&vec![0.0; HOP * 10]);
        assert_eq!(columns.len(), 10);
        assert!(columns.iter().all(|c| c.len() == BANDS));
    }

    #[test]
    fn chunking_does_not_change_the_result() {
        let samples = sine(440.0, 0.5, 0.5);
        let whole = Analyzer::new().push(&samples);
        let mut a = Analyzer::new();
        let pieces: Vec<_> = samples.chunks(333).flat_map(|c| a.push(c)).collect();
        assert_eq!(whole, pieces);
    }

    #[test]
    fn sine_peaks_in_the_matching_band() {
        for freq in [200.0, 1000.0, 4000.0] {
            let columns = Analyzer::new().push(&sine(freq, 0.5, 0.02));
            let column = columns.last().unwrap();
            let peak = loudest(column);
            let ratio = band_freq(peak) / freq;
            assert!((0.88..1.14).contains(&ratio), "{freq} Hz peaked at band {peak} ({} Hz)", band_freq(peak));
            // Far-away bands stay much quieter.
            let far = (peak + BANDS / 2) % BANDS;
            assert!(column[far] < column[peak] - 0.3, "band {far} is {} vs {}", column[far], column[peak]);
        }
    }

    #[test]
    fn silence_is_dark_and_loud_is_bright() {
        let silent = Analyzer::new().push(&vec![0.0; 4096]);
        assert!(silent.iter().flatten().all(|&v| v == 0.0));
        let loud = Analyzer::new().push(&sine(1000.0, 0.5, 0.9));
        assert!(loud.last().unwrap().iter().cloned().fold(0.0, f32::max) > 0.9);
    }
}
