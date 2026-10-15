//! Sample-level helpers: energy gate before the engine, WAV loading.

use anyhow::{Context, Result};
use std::path::Path;

pub const SAMPLE_RATE: usize = 16_000;

const FRAME: usize = SAMPLE_RATE / 50;
/// RMS of a frame that counts as sound (about -46 dBFS).
const FRAME_RMS: f32 = 0.005;
/// Frames of sound needed overall (100 ms).
const MIN_LOUD_FRAMES: usize = 5;

/// Cheap energy check that spares the engine from transcribing silence, where whisper hallucinates.
pub fn has_speech(samples: &[f32]) -> bool {
    samples
        .chunks(FRAME)
        .filter(|frame| {
            let energy = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
            energy.sqrt() > FRAME_RMS
        })
        .count()
        >= MIN_LOUD_FRAMES
}

/// Mono f32 at 16 kHz from interleaved samples. Downsampling averages the covered input span.
pub fn to_mono_16k(interleaved: &[f32], channels: usize, rate: u32) -> Vec<f32> {
    let mono: Vec<f32> =
        interleaved.chunks_exact(channels.max(1)).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32).collect();
    if rate as usize == SAMPLE_RATE || mono.is_empty() {
        return mono;
    }
    let step = rate as f64 / SAMPLE_RATE as f64;
    let out_len = (mono.len() as f64 / step) as usize;
    (0..out_len)
        .map(|i| {
            let from = i as f64 * step;
            let to = ((i + 1) as f64 * step).min(mono.len() as f64);
            if step > 1.0 {
                let (a, b) = (from as usize, (to.ceil() as usize).max(from as usize + 1).min(mono.len()));
                mono[a..b].iter().sum::<f32>() / (b - a) as f32
            } else {
                let (a, frac) = (from as usize, (from.fract()) as f32);
                let next = mono.get(a + 1).copied().unwrap_or(mono[a]);
                mono[a] * (1.0 - frac) + next * frac
            }
        })
        .collect()
}

pub fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    read_wav_from(std::io::BufReader::new(file)).with_context(|| path.display().to_string())
}

pub fn read_wav_from(source: impl std::io::Read) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::new(source)?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|s| s as f32 * scale)).collect::<Result<_, _>>()?
        }
    };
    Ok(to_mono_16k(&samples, spec.channels as usize, spec.sample_rate))
}

/// Samples per level frame (16 ms).
pub const LEVEL_FRAME: usize = 256;

/// Cuts a sample stream into frames and reports the RMS of each completed one.
#[derive(Default)]
pub struct FrameMeter {
    pending: Vec<f32>,
}

impl FrameMeter {
    pub fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        self.pending.extend_from_slice(samples);
        let frames = self.pending.chunks_exact(LEVEL_FRAME);
        let levels =
            frames.clone().map(|f| (f.iter().map(|s| s * s).sum::<f32>() / LEVEL_FRAME as f32).sqrt()).collect();
        let used = frames.len() * LEVEL_FRAME;
        self.pending.drain(..used);
        levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_and_faint_noise_are_not_speech() {
        assert!(!has_speech(&vec![0.0; 16_000]));
        assert!(!has_speech(&vec![0.001; 16_000]));
    }

    #[test]
    fn a_tone_is_speech_but_a_click_is_not() {
        let tone: Vec<f32> = (0..8_000).map(|i| 0.1 * (i as f32 * 0.2).sin()).collect();
        assert!(has_speech(&tone));
        let mut click = vec![0.0; 16_000];
        click[100] = 0.9;
        assert!(!has_speech(&click));
    }

    #[test]
    fn frame_meter_reports_rms_per_frame_across_chunks() {
        let mut meter = FrameMeter::default();
        assert!(meter.push(&[0.5; 100]).is_empty());
        let levels = meter.push(&[0.5; 500]);
        assert_eq!(levels.len(), 2);
        assert!(levels.iter().all(|l| (l - 0.5).abs() < 1e-6));
        assert_eq!(meter.push(&[0.0; 168]).len(), 1);
    }

    #[test]
    fn downmix_and_resample() {
        let stereo: Vec<f32> = (0..48_000).flat_map(|_| [1.0, 0.0]).collect();
        let out = to_mono_16k(&stereo, 2, 48_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn upsampling_interpolates() {
        let out = to_mono_16k(&[0.0, 1.0, 0.0, 1.0], 1, 8_000);
        assert_eq!(out.len(), 8);
        assert!((out[1] - 0.5).abs() < 1e-6);
    }
}
