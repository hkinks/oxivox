mod audio;
mod config;
mod daemon;
mod demo;
mod engine;
mod ipc;
mod overlay;
mod paths;
mod platform;
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
use crate::platform::headset::PwDumpProbe;
use crate::platform::hyprland::{Hyprland, HyprlandOutput, SubmapCancel};
use crate::platform::media::{MediaControl, Playerctl};
use crate::platform::pipewire::PipewireCapture;
use crate::platform::trigger::SocketTrigger;
use crate::platform::wayland::LayerShellSurface;
use crate::platform::{CancelKey, LogOutput, Output, Trigger};
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
    Daemon {
        /// Log the transcript instead of pasting it
        #[arg(long)]
        dry_run: bool,
    },
    /// Start recording, or stop and paste when already recording
    Toggle,
    /// Discard the current recording or transcription
    Cancel,
    /// Print the daemon's state
    Status,
    /// Show the overlay with synthetic audio, without microphone or model
    #[command(hide = true)]
    OverlayDemo {
        #[arg(long, default_value_t = 6.0)]
        seconds: f32,
    },
    /// Paste text into a window the way the daemon does
    #[command(hide = true)]
    PasteTest {
        text: String,
        /// Hyprland window address; the focused window when omitted
        #[arg(long)]
        window: Option<String>,
    },
    /// Time the engine on a WAV file, repeatedly, with pauses in between
    #[command(hide = true)]
    Bench {
        file: PathBuf,
        #[arg(long, default_value_t = 3)]
        runs: u32,
        /// Seconds of idle time before every run
        #[arg(long, default_value_t = 0)]
        pause: u64,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, default_value_t = 5)]
        beam: i32,
    },
    /// Transcribe a WAV file and print the text
    Transcribe {
        file: PathBuf,
        /// Model file; defaults to the configured or standard location
        #[arg(long)]
        model: Option<PathBuf>,
        /// Language code, or "auto"; defaults to the configured language
        #[arg(long)]
        language: Option<String>,
        /// Run on the CPU instead of the GPU
        #[arg(long)]
        cpu: bool,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "rustle=info".into()))
        .with_writer(std::io::stderr)
        .init();
    match Cli::parse().command {
        Action::Daemon { dry_run } => run_daemon(dry_run),
        Action::PasteTest { text, window } => paste_test(&text, window.as_deref()),
        Action::OverlayDemo { seconds } => demo::run(seconds),
        Action::Bench { file, runs, pause, language, beam } => bench(&file, runs, pause, language, beam),
        Action::Toggle => client(Command::Toggle),
        Action::Cancel => client(Command::Cancel),
        Action::Status => client(Command::Status),
        Action::Transcribe { file, model, language, cpu } => transcribe(file, model, language, !cpu),
    }
}

fn client(command: Command) -> Result<()> {
    println!("{}", ipc::send(&paths::socket_path(), command)?);
    Ok(())
}

fn transcribe(file: PathBuf, model: Option<PathBuf>, language: Option<String>, gpu: bool) -> Result<()> {
    let config = Config::load_default()?;
    let model = match model {
        Some(model) => model,
        None => config.model_path()?,
    };
    let language = language.unwrap_or_else(|| config.language.clone());
    let language = (language != "auto" && !language.is_empty()).then_some(language);
    let audio = audio::read_wav(&file)?;
    let mut engine = WhisperEngine::load(&model, language.as_deref(), gpu && config.gpu)?;
    let raw = engine.transcribe(&audio, &Default::default())?;
    println!("{}", tokens::clean(&raw));
    Ok(())
}

fn paste_test(text: &str, window: Option<&str>) -> Result<()> {
    let config = Config::load_default()?;
    let hypr = Hyprland::from_env()?;
    let target = match window {
        Some(address) => Some(hypr.find_window(address).with_context(|| format!("no window {address}"))?),
        None => None,
    };
    HyprlandOutput::new(hypr, config.paste).deliver(text, target.as_ref())?;
    // The clipboard is restored from a background thread.
    std::thread::sleep(std::time::Duration::from_millis(700));
    Ok(())
}

fn run_daemon(dry_run: bool) -> Result<()> {
    let config = Config::load_default()?;
    let model = config.model_path()?;
    let (trigger, _socket_guard) = SocketTrigger::bind(paths::socket_path())?;

    let engine = WhisperEngine::load(&model, config.language_code(), config.gpu)?;
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

    let cancel_key =
        config.cancel_submap.clone().map(|name| Box::new(SubmapCancel::new(hypr.clone(), name)) as Box<dyn CancelKey>);
    let platform = daemon::Platform {
        capture: Box::new(PipewireCapture::new(config.source.clone())),
        overlay: sink,
        focus: Box::new(hypr.clone()),
        output: if dry_run {
            std::sync::Arc::new(LogOutput)
        } else {
            std::sync::Arc::new(HyprlandOutput::new(hypr, config.paste.clone()))
        },
        media: Box::new(MediaControl::new(
            std::sync::Arc::new(Playerctl),
            std::sync::Arc::new(PwDumpProbe),
            config.media_ignore.clone(),
        )),
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

fn bench(file: &std::path::Path, runs: u32, pause: u64, language: Option<String>, beam: i32) -> Result<()> {
    let config = Config::load_default()?;
    let audio = audio::read_wav(file)?;
    let language = language.filter(|l| l != "auto");
    let started = std::time::Instant::now();
    let mut engine = WhisperEngine::load(&config.model_path()?, language.as_deref(), true)?;
    engine.beam = beam;
    eprintln!("load: {:.2}s, audio {:.1}s", started.elapsed().as_secs_f32(), audio.len() as f32 / 16000.0);
    for run in 1..=runs {
        std::thread::sleep(std::time::Duration::from_secs(pause));
        let started = std::time::Instant::now();
        let text = engine.transcribe(&audio, &Default::default())?;
        eprintln!("run {run}: {:.2}s ({} chars)", started.elapsed().as_secs_f32(), text.len());
    }
    engine.print_timings();
    Ok(())
}
