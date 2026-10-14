//! Speech-to-text engines.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use crate::spectrogram::SAMPLE_RATE;

pub trait Engine: Send {
    /// Raw transcript of 16 kHz mono audio. Returns early with an error once `cancel` is set.
    fn transcribe(&mut self, audio: &[f32], cancel: &Arc<AtomicBool>) -> Result<String>;
}

/// whisper.cpp keeps the model on the GPU for the lifetime of this value.
pub struct WhisperEngine {
    // The state holds its own reference to the context's weights; keep the context alive regardless.
    _ctx: WhisperContext,
    state: WhisperState,
    language: Option<String>,
}

impl WhisperEngine {
    pub fn load(model: &Path, language: Option<&str>, gpu: bool) -> Result<Self> {
        let devices = whisper_rs::vulkan::list_devices();
        if gpu {
            let Some(device) = devices.first() else {
                bail!("no Vulkan device found; set `gpu = false` in the config to run on the CPU");
            };
            tracing::info!(gpu = %device.name, vram_free_mb = device.vram.free >> 20, "loading {}", model.display());
        } else {
            tracing::info!("loading {} for the CPU", model.display());
        }
        let mut params = WhisperContextParameters::default();
        params.use_gpu(gpu);
        params.flash_attn(gpu);
        let ctx = WhisperContext::new_with_params(model, params)
            .map_err(|e| anyhow::anyhow!("{e:?}"))
            .with_context(|| format!("loading model {}", model.display()))?;
        let state = ctx.create_state().map_err(|e| anyhow::anyhow!("{e:?}")).context("creating whisper state")?;
        Ok(Self { _ctx: ctx, state, language: language.map(String::from) })
    }
}

impl Engine for WhisperEngine {
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

        let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
        params.set_language(self.language.as_deref());
        params.set_n_threads(4);
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
        Ok(self.state.as_iter().map(|segment| segment.to_str_lossy().map(|s| s.into_owned()).unwrap_or_default()).collect::<Vec<_>>().join(" "))
    }
}

/// `set_abort_callback_safe` cannot infer the closure type from its `Into<Option<F>>` bound.
fn set_abort<F: FnMut() -> bool + 'static>(params: &mut FullParams, callback: F) {
    params.set_abort_callback_safe::<Option<F>, F>(Some(callback));
}
