//! The long-running process: owns the engine and the platform adapters, runs the state machine.

use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::audio;
use crate::config::Config;
use crate::engine::Engine;
use crate::ipc::{Command, Request};
use crate::overlay::{FlashKind, FlashMessage, OverlayCmd, OverlaySink, View};
use crate::platform::{Capture, CaptureEvent, CancelKey, Focus, Media, Output, Window};
use crate::spectrogram::{Analyzer, SAMPLE_RATE};
use crate::state::{Action, Event, Flash, MicFailure, Outcome, State, step};
use crate::tokens;

/// The microphone has this long to deliver real audio.
const WARM_UP_TIMEOUT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(50);

pub struct Platform {
    pub capture: Box<dyn Capture>,
    pub overlay: Arc<dyn OverlaySink>,
    pub focus: Box<dyn Focus>,
    pub output: Arc<dyn Output>,
    pub media: Box<dyn Media>,
    pub cancel_key: Option<Box<dyn CancelKey>>,
}

pub enum Msg {
    Request(Request),
    Capture(CaptureEvent),
    Transcribed { job: u64, result: Result<String> },
}

struct Job {
    id: u64,
    audio: Vec<f32>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Recording {
    samples: Vec<f32>,
    started: Option<Instant>,
    /// Real (non-zero) audio has arrived.
    live: bool,
    stream_up: bool,
    analyzer: Analyzer,
    target: Option<Window>,
}

pub struct Daemon {
    config: Config,
    platform: Platform,
    state: State,
    recording: Recording,
    jobs: Sender<Job>,
    inbox: Sender<Msg>,
    job_id: u64,
    job_cancel: Arc<AtomicBool>,
    /// Text of the finished job, waiting for `Action::Deliver`.
    transcript: String,
}

fn flash_message(flash: Flash) -> FlashMessage {
    let (text, kind) = match flash {
        Flash::NothingHeard => ("nothing heard", FlashKind::Info),
        Flash::NoMicrophone => ("no microphone", FlashKind::Error),
        Flash::MicDidNotStart => ("mic didn't start", FlashKind::Error),
        Flash::TranscriptionFailed => ("transcription failed", FlashKind::Error),
    };
    FlashMessage { text: text.into(), kind }
}

fn state_name(state: State) -> &'static str {
    match state {
        State::Idle => "idle",
        State::WarmingUp => "warming-up",
        State::Recording => "recording",
        State::Transcribing => "transcribing",
    }
}

impl Daemon {
    /// Returns the daemon and the receiver of its message loop; hand `inbox()` to the triggers.
    pub fn new(config: Config, platform: Platform, engine: Box<dyn Engine>) -> (Self, Receiver<Msg>) {
        let (inbox, messages) = mpsc::channel();
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let results = inbox.clone();
        std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || {
                let mut engine = engine;
                for job in job_rx {
                    let result = engine.transcribe(&job.audio, &job.cancel);
                    if results.send(Msg::Transcribed { job: job.id, result }).is_err() {
                        break;
                    }
                }
            })
            .expect("spawning engine thread");
        let daemon = Self {
            config,
            platform,
            state: State::Idle,
            recording: Recording::default(),
            jobs,
            inbox,
            job_id: 0,
            job_cancel: Arc::new(AtomicBool::new(false)),
            transcript: String::new(),
        };
        (daemon, messages)
    }

    pub fn inbox(&self) -> Sender<Msg> {
        self.inbox.clone()
    }

    pub fn run(mut self, messages: Receiver<Msg>) {
        loop {
            match messages.recv_timeout(TICK) {
                Ok(Msg::Request(request)) => self.on_request(request),
                Ok(Msg::Capture(event)) => self.on_capture(event),
                Ok(Msg::Transcribed { job, result }) => self.on_transcribed(job, result),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            self.on_tick();
        }
    }

    fn on_request(&mut self, request: Request) {
        match request.command {
            Command::Toggle => self.apply(Event::Toggle),
            Command::Cancel => self.apply(Event::Cancel),
            Command::Status => {}
        }
        let _ = request.reply.send(state_name(self.state).to_string());
    }

    fn on_tick(&mut self) {
        match self.state {
            State::WarmingUp if self.recording.started.is_some_and(|t| t.elapsed() > WARM_UP_TIMEOUT) => {
                let failure = if self.recording.stream_up { MicFailure::NeverStarted } else { MicFailure::NoMicrophone };
                self.apply(Event::MicFailed(failure));
            }
            State::Recording if self.recording.samples.len() >= self.config.max_length_secs as usize * SAMPLE_RATE => {
                tracing::info!("maximum recording length reached");
                self.apply(Event::MaxLength);
            }
            _ => {}
        }
    }

    fn on_capture(&mut self, event: CaptureEvent) {
        if !matches!(self.state, State::WarmingUp | State::Recording) {
            return;
        }
        match event {
            CaptureEvent::Streaming => self.recording.stream_up = true,
            CaptureEvent::Failed(failure) => self.apply(Event::MicFailed(failure)),
            CaptureEvent::Samples(samples) => {
                self.recording.stream_up = true;
                let mut audible = &samples[..];
                if !self.recording.live {
                    // A Bluetooth headset delivers zeros until its call profile is up.
                    let Some(first) = samples.iter().position(|s| s.abs() > 1e-7) else { return };
                    audible = &samples[first..];
                    self.recording.live = true;
                    self.apply(Event::MicLive);
                }
                self.recording.samples.extend_from_slice(audible);
                let columns = self.recording.analyzer.push(audible);
                if !columns.is_empty() {
                    self.platform.overlay.send(OverlayCmd::Columns(columns));
                }
            }
        }
    }

    fn on_transcribed(&mut self, job: u64, result: Result<String>) {
        if job != self.job_id {
            return; // cancelled meanwhile
        }
        let outcome = match result {
            Ok(raw) => {
                let text = tokens::clean(&raw);
                tracing::info!(chars = text.len(), "transcribed");
                self.transcript = text;
                if self.transcript.is_empty() { Outcome::Empty } else { Outcome::Text }
            }
            Err(e) => {
                tracing::error!("transcription failed: {e:#}");
                Outcome::Failed
            }
        };
        self.apply(Event::Transcribed(outcome));
    }

    fn apply(&mut self, event: Event) {
        let (next, actions) = step(self.state, event);
        if next != self.state {
            tracing::debug!(from = state_name(self.state), to = state_name(next), ?event, "state");
        }
        self.state = next;
        for action in actions {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: Action) {
        match action {
            Action::Begin => self.begin(),
            Action::ShowRecording => self.platform.overlay.send(OverlayCmd::Show(View::Recording)),
            Action::StopCapture => self.platform.capture.stop(),
            Action::ShowTranscribing => self.platform.overlay.send(OverlayCmd::Show(View::Transcribing)),
            Action::Transcribe => self.transcribe(),
            Action::CancelTranscription => {
                self.job_cancel.store(true, Ordering::Relaxed);
                self.job_id += 1;
            }
            Action::Deliver => {
                self.platform.overlay.send(OverlayCmd::Show(View::Hidden));
                let text = std::mem::take(&mut self.transcript);
                if let Err(e) = self.platform.output.deliver(&text, self.recording.target.as_ref()) {
                    tracing::error!("paste failed: {e:#}");
                    let message = FlashMessage { text: "paste failed".into(), kind: FlashKind::Error };
                    self.platform.overlay.send(OverlayCmd::Show(View::Flash(message)));
                }
                self.finish_cycle();
            }
            Action::ResumeMedia => self.finish_cycle(),
            Action::Hide => self.platform.overlay.send(OverlayCmd::Show(View::Hidden)),
            Action::Flash(flash) => self.platform.overlay.send(OverlayCmd::Show(View::Flash(flash_message(flash)))),
        }
    }

    fn begin(&mut self) {
        self.recording = Recording {
            started: Some(Instant::now()),
            target: self.platform.focus.active().unwrap_or_else(|e| {
                tracing::warn!("cannot read the focused window: {e:#}");
                None
            }),
            ..Recording::default()
        };
        self.transcript.clear();
        self.platform.overlay.send(OverlayCmd::Show(View::WarmingUp));
        self.platform.media.pause();
        if let Some(key) = &self.platform.cancel_key {
            key.set_armed(true);
        }
        let inbox = self.inbox.clone();
        let sink = Box::new(move |event| {
            let _ = inbox.send(Msg::Capture(event));
        });
        if let Err(e) = self.platform.capture.start(sink) {
            tracing::error!("cannot start capture: {e:#}");
            let _ = self.inbox.send(Msg::Capture(CaptureEvent::Failed(MicFailure::NoMicrophone)));
        }
    }

    fn transcribe(&mut self) {
        let audio = std::mem::take(&mut self.recording.samples);
        self.job_id += 1;
        self.job_cancel = Arc::new(AtomicBool::new(false));
        if !audio::has_speech(&audio) {
            tracing::info!(seconds = audio.len() / SAMPLE_RATE, "no speech energy, skipping the engine");
            let _ = self.inbox.send(Msg::Transcribed { job: self.job_id, result: Ok(String::new()) });
            return;
        }
        tracing::info!(seconds = audio.len() / SAMPLE_RATE, "transcribing");
        let job = Job { id: self.job_id, audio, cancel: self.job_cancel.clone() };
        if self.jobs.send(job).is_err() {
            let _ = self.inbox.send(Msg::Transcribed { job: self.job_id, result: Err(anyhow::anyhow!("engine thread is gone")) });
        }
    }

    /// End of a dictation: media resumes and Escape is released.
    fn finish_cycle(&mut self) {
        self.platform.media.resume();
        if let Some(key) = &self.platform.cancel_key {
            key.set_armed(false);
        }
    }
}

#[cfg(test)]
mod tests;
