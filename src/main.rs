mod audio;
mod config;
mod daemon;
mod engine;
mod ipc;
mod overlay;
mod paths;
mod platform;
mod spectrogram;
mod state;
mod theme;
mod theme_watch;
mod tokens;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::config::Config;
use crate::engine::{Engine, WhisperEngine};
use crate::ipc::Command;
use crate::overlay::{Overlay, Surface, text};
use crate::platform::hyprland::{Hyprland, HyprlandOutput, SubmapCancel};
use crate::platform::media::Playerctl;
use crate::platform::pipewire::PipewireCapture;
use crate::platform::trigger::SocketTrigger;
use crate::platform::wayland::LayerShellSurface;
use crate::platform::{CancelKey, Trigger};
use crate::theme::Theme;

#[derive(Parser)]
#[command(version, about = "Dictation: press a key, speak, press again, the text appears in the app you were in")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Run the daemon: overlay, microphone, speech engine
    Daemon,
    /// Start recording, or stop and paste when already recording
    Toggle,
    /// Discard the current recording or transcription
    Cancel,
    /// Print the daemon's state
    Status,
    /// Transcribe a WAV file and print the text
    Transcribe {
        file: PathBuf,
        /// Model file; defaults to the configured or standard location
        #[arg(long)]
        model: Option<PathBuf>,
        /// Language code, or "auto"; defaults to the configured language
        #[arg(long)]
        language: Option<String>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "wisprs=info".into()))
        .with_writer(std::io::stderr)
        .init();
    match Cli::parse().command {
        Action::Daemon => run_daemon(),
        Action::Toggle => client(Command::Toggle),
        Action::Cancel => client(Command::Cancel),
        Action::Status => client(Command::Status),
        Action::Transcribe { file, model, language } => transcribe(file, model, language),
    }
}

fn client(command: Command) -> Result<()> {
    println!("{}", ipc::send(&paths::socket_path(), command)?);
    Ok(())
}

fn transcribe(file: PathBuf, model: Option<PathBuf>, language: Option<String>) -> Result<()> {
    let config = Config::load_default()?;
    let model = match model {
        Some(model) => model,
        None => config.model_path()?,
    };
    let language = language.unwrap_or_else(|| config.language.clone());
    let language = (language != "auto" && !language.is_empty()).then_some(language);
    let audio = audio::read_wav(&file)?;
    let mut engine = WhisperEngine::load(&model, language.as_deref())?;
    let raw = engine.transcribe(&audio, &Default::default())?;
    println!("{}", tokens::clean(&raw));
    Ok(())
}

fn run_daemon() -> Result<()> {
    let config = Config::load_default()?;
    let model = config.model_path()?;
    let (trigger, _socket_guard) = SocketTrigger::bind(paths::socket_path())?;

    let engine = WhisperEngine::load(&model, config.language_code())?;
    tracing::info!("model loaded");

    let hypr = Hyprland::from_env()?;
    let theme_path = paths::config_dir().join("theme.toml");
    let theme = Theme::load(&theme_path).unwrap_or_else(|e| {
        tracing::warn!("using the built-in theme: {e:#}");
        Theme::default()
    });
    let overlay = Overlay::new(config.overlay.clone(), theme, text::load_system_font());
    let sink = Box::new(LayerShellSurface).spawn(overlay)?;
    if let Err(e) = theme_watch::spawn(theme_path, sink.clone()) {
        tracing::warn!("theme changes will not be picked up: {e:#}");
    }

    let cancel_key = config
        .cancel_submap
        .clone()
        .map(|name| Box::new(SubmapCancel::new(hypr.clone(), name)) as Box<dyn CancelKey>);
    let platform = daemon::Platform {
        capture: Box::new(PipewireCapture::default()),
        overlay: sink,
        focus: Box::new(hypr.clone()),
        output: std::sync::Arc::new(HyprlandOutput::new(hypr, config.paste.clone())),
        media: Box::new(Playerctl::default()),
        cancel_key,
    };
    let (daemon, messages) = daemon::Daemon::new(config, platform, Box::new(engine));
    let inbox = daemon.inbox();
    Box::new(trigger)
        .spawn(Box::new(move |request| {
            let _ = inbox.send(daemon::Msg::Request(request));
        }))
        .context("starting the trigger")?;
    tracing::info!("ready on {}", paths::socket_path().display());
    daemon.run(messages);
    Ok(())
}
