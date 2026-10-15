//! Speech-to-text engines.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use crate::audio::SAMPLE_RATE;

pub trait Engine: Send {
    /// Raw transcript of 16 kHz mono audio. Returns early with an error once `cancel` is set.
    fn transcribe(&mut self, audio: &[f32], cancel: &Arc<AtomicBool>) -> Result<String>;

    /// Runs a throwaway transcription so that one-time costs do not land on the first dictation.
    fn warm_up(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A short spoken sentence for [`Engine::warm_up`].
const WARM_UP_WAV: &[u8] = include_bytes!("../assets/warmup.wav");

/// whisper.cpp keeps the model on the GPU for the lifetime of this value.
pub struct WhisperEngine {
    // The state holds its own reference to the context's weights; keep the context alive regardless.
    ctx: WhisperContext,
    state: WhisperState,
    pub language: Option<String>,
    threads: i32,
    /// Beam search width; 1 means greedy decoding.
    pub beam: i32,
}

impl WhisperEngine {
    /// `gpu_fallback_cpu`: when the GPU lacks the memory for the model, run on the CPU instead of failing.
    pub fn load(model: &Path, language: Option<&str>, gpu: bool, gpu_fallback_cpu: bool) -> Result<Self> {
        let mut gpu = gpu;
        if gpu {
            let devices = whisper_rs::vulkan::list_devices();
            let Some(device) = devices.first() else {
                bail!("no Vulkan device found; set `gpu = false` in the config to run on the CPU");
            };
            let free = device.vram.free as u64;
            tracing::info!(gpu = %device.name, vram_free_mb = free >> 20, "loading {}", model.display());
            let model_bytes = std::fs::metadata(model).with_context(|| model.display().to_string())?.len();
            gpu = choose_device(free, model_bytes, gpu_fallback_cpu)?;
        }
        if !gpu {
            tracing::info!("loading {} for the CPU", model.display());
        }
        let mut params = WhisperContextParameters::default();
        params.use_gpu(gpu);
        params.flash_attn(gpu);
        let ctx = WhisperContext::new_with_params(model, params)
            .map_err(|e| anyhow::anyhow!("{e:?}"))
            .with_context(|| format!("loading model {}", model.display()))?;
        let state = ctx.create_state().map_err(|e| anyhow::anyhow!("{e:?}")).context("creating whisper state")?;
        // The GPU does the heavy work; the CPU path wants every core it can get.
        let threads = if gpu { 4 } else { std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32 };
        Ok(Self { ctx, state, language: language.map(String::from), threads, beam: 5 })
    }

    pub fn print_timings(&self) {
        self.ctx.print_timings();
    }
}

/// Device memory beyond the model file that whisper.cpp allocates: compute buffers, KV cache, the state.
const GPU_BUFFER_BYTES: u64 = 250 << 20;

/// Whether to run on the GPU. ggml aborts the whole process when Vulkan runs out of device memory, so the
/// check happens before the model is loaded: not enough free memory means the CPU (when allowed) or an error.
fn choose_device(free: u64, model_bytes: u64, fallback_cpu: bool) -> Result<bool> {
    let needed = model_bytes + GPU_BUFFER_BYTES;
    if free >= needed {
        return Ok(true);
    }
    let (free_mb, needed_mb) = (free >> 20, needed >> 20);
    if fallback_cpu {
        tracing::warn!(free_mb, needed_mb, "not enough free GPU memory for the model; falling back to the CPU");
        return Ok(false);
    }
    bail!(
        "not enough free GPU memory for the model: {free_mb} MB free, about {needed_mb} MB needed. \
         Free some GPU memory, or set `gpu_fallback_cpu = true` (slow) or `gpu = false` in the config"
    )
}

impl Engine for WhisperEngine {
    /// ggml-vulkan compiles its shaders on first use. With a cold driver shader cache that takes 10 to 15 s,
    /// which would otherwise be the wait of the first dictation after every cache eviction.
    fn warm_up(&mut self) -> Result<()> {
        let audio = crate::audio::read_wav_from(WARM_UP_WAV)?;
        self.transcribe(&audio, &Arc::default())?;
        Ok(())
    }

    fn transcribe(&mut self, audio: &[f32], cancel: &Arc<AtomicBool>) -> Result<String> {
        // whisper.cpp rejects input shorter than a second.
        let mut padded;
        let audio = if audio.len() < SAMPLE_RATE * 11 / 10 {
            padded = audio.to_vec();
            padded.resize(SAMPLE_RATE * 11 / 10, 0.0);
            &padded[..]
        } else {
            audio
        };

        let strategy = if self.beam > 1 {
            SamplingStrategy::BeamSearch { beam_size: self.beam, patience: -1.0 }
        } else {
            SamplingStrategy::Greedy { best_of: 1 }
        };
        let mut params = FullParams::new(strategy);
        params.set_language(self.language.as_deref());
        params.set_n_threads(self.threads);
        params.set_translate(false);
        params.set_no_timestamps(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let abort = cancel.clone();
        set_abort(&mut params, move || abort.load(Ordering::Relaxed));

        self.state.full(params, audio).map_err(|e| anyhow::anyhow!("{e:?}")).context("whisper inference")?;
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        Ok(self
            .state
            .as_iter()
            .map(|segment| segment.to_str_lossy().map(|s| s.into_owned()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" "))
    }
}

/// whisper-rs's trampoline reinterprets the stored pointer as the callback type `F`, but stores a boxed trait
/// object, so `F` must be exactly `Box<dyn FnMut() -> bool>` or the callback reads garbage.
fn set_abort(params: &mut FullParams, callback: impl FnMut() -> bool + 'static) {
    type Callback = Box<dyn FnMut() -> bool>;
    params.set_abort_callback_safe::<Option<Callback>, Callback>(Some(Box::new(callback)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: u64 = 1600 << 20;

    #[test]
    fn enough_free_memory_uses_the_gpu() {
        assert!(choose_device(MODEL + GPU_BUFFER_BYTES, MODEL, false).unwrap());
        assert!(choose_device(8 << 30, MODEL, true).unwrap());
    }

    #[test]
    fn too_little_memory_is_an_error_naming_the_numbers() {
        let message = choose_device(1400 << 20, MODEL, false).unwrap_err().to_string();
        assert!(message.contains("1400 MB free"), "{message}");
        assert!(message.contains("1850 MB needed"), "{message}");
        assert!(message.contains("gpu_fallback_cpu"), "{message}");
    }

    #[test]
    fn too_little_memory_falls_back_to_the_cpu_when_allowed() {
        assert!(!choose_device(1400 << 20, MODEL, true).unwrap());
    }
}
