//! Interfaces to the desktop. Each has one implementation per platform; Hyprland comes first.

pub mod headset;
pub mod hyprland;
pub mod media;
pub mod pipewire;
pub mod trigger;
pub mod wayland;

use anyhow::Result;

use crate::ipc::Request;
use crate::state::MicFailure;

/// A window that can receive the pasted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub id: String,
    pub class: String,
}

/// What was focused when recording started.
pub trait Focus: Send + Sync {
    fn active(&self) -> Result<Option<Window>>;
}

/// Puts text into the target application.
pub trait Output: Send + Sync {
    /// Pastes `text` into `target`, or into whatever is focused now when the target is gone or unknown.
    fn deliver(&self, text: &str, target: Option<&Window>) -> Result<()>;
}

/// Logs the text instead of pasting it, to try a daemon without touching any window.
pub struct LogOutput;

impl Output for LogOutput {
    fn deliver(&self, text: &str, target: Option<&Window>) -> Result<()> {
        tracing::info!(?target, %text, "dry run: would paste");
        Ok(())
    }
}

/// Receives commands from a hotkey, the CLI, a desktop portal...
pub trait Trigger {
    fn spawn(self: Box<Self>, on_request: Box<dyn Fn(Request) + Send>) -> Result<()>;
}

#[derive(Debug)]
pub enum CaptureEvent {
    /// The stream is up (the source may still deliver silence).
    Streaming,
    /// 16 kHz mono samples.
    Samples(Vec<f32>),
    Failed(MicFailure),
}

pub trait Capture {
    /// Opens the microphone and streams its samples to `sink` until [`Capture::stop`].
    fn start(&mut self, sink: Box<dyn FnMut(CaptureEvent) + Send>) -> Result<()>;
    /// Closes the stream and waits until it is closed, so a headset can return to its music profile.
    fn stop(&mut self);
}

/// Pauses media players while dictating.
pub trait Media {
    fn pause(&mut self);
    fn resume(&mut self);
}

/// Binds Escape to `rustle cancel` while a recording is active.
pub trait CancelKey {
    fn set_armed(&self, armed: bool);
}
