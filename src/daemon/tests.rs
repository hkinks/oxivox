//! The daemon driven by fake adapters, from request to pasted text.

use super::*;
use std::sync::Mutex;

type Log = Arc<Mutex<Vec<String>>>;

struct FakeCapture(Log);
impl Capture for FakeCapture {
    fn start(&mut self, _: Box<dyn FnMut(CaptureEvent) + Send>) -> Result<()> {
        self.0.lock().unwrap().push("capture start".into());
        Ok(())
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().push("capture stop".into());
    }
}

struct FakeOverlay(Mutex<Vec<OverlayCmd>>);
impl OverlaySink for FakeOverlay {
    fn send(&self, cmd: OverlayCmd) {
        self.0.lock().unwrap().push(cmd);
    }
}

struct FakeFocus;
impl Focus for FakeFocus {
    fn active(&self) -> Result<Option<Window>> {
        Ok(Some(Window { id: "0x1".into(), class: "ghostty".into() }))
    }
}

struct FakeOutput(Log);
impl Output for FakeOutput {
    fn deliver(&self, text: &str, target: Option<&Window>) -> Result<()> {
        self.0.lock().unwrap().push(format!("paste {text:?} into {}", target.map(|w| w.id.as_str()).unwrap_or("?")));
        Ok(())
    }
}

struct FakeMedia(Log);
impl Media for FakeMedia {
    fn pause(&mut self) {
        self.0.lock().unwrap().push("media pause".into());
    }
    fn resume(&mut self) {
        self.0.lock().unwrap().push("media resume".into());
    }
}

struct FakeEngine {
    reply: &'static str,
    calls: Log,
}
impl Engine for FakeEngine {
    fn transcribe(&mut self, audio: &[f32], _: &Arc<AtomicBool>) -> Result<String> {
        self.calls.lock().unwrap().push(format!("engine {} samples", audio.len()));
        Ok(self.reply.into())
    }
}

/// An engine whose warm-up takes as long as the test lets it.
struct SlowWarmEngine {
    warm_up_gate: Receiver<()>,
    calls: Log,
}
impl Engine for SlowWarmEngine {
    fn transcribe(&mut self, audio: &[f32], _: &Arc<AtomicBool>) -> Result<String> {
        self.calls.lock().unwrap().push(format!("engine {} samples", audio.len()));
        Ok("Hello".into())
    }
    fn warm_up(&mut self) -> Result<()> {
        self.calls.lock().unwrap().push("warm-up start".into());
        let _ = self.warm_up_gate.recv();
        self.calls.lock().unwrap().push("warm-up done".into());
        Ok(())
    }
}

struct Harness {
    daemon: Daemon,
    messages: Receiver<Msg>,
    log: Log,
    overlay: Arc<FakeOverlay>,
}

fn harness(engine_reply: &'static str) -> Harness {
    let log: Log = Arc::default();
    let engine = Box::new(FakeEngine { reply: engine_reply, calls: log.clone() });
    harness_with(log, engine)
}

fn harness_with(log: Log, engine: Box<dyn Engine>) -> Harness {
    let overlay = Arc::new(FakeOverlay(Mutex::default()));
    let platform = Platform {
        capture: Box::new(FakeCapture(log.clone())),
        overlay: overlay.clone(),
        focus: Box::new(FakeFocus),
        output: Arc::new(FakeOutput(log.clone())),
        media: Box::new(FakeMedia(log.clone())),
        cancel_key: None,
    };
    let (daemon, messages) = Daemon::new(Config::default(), platform, engine);
    Harness { daemon, messages, log, overlay }
}

impl Harness {
    fn request(&mut self, command: Command) -> String {
        let (reply, answer) = mpsc::channel();
        self.daemon.on_request(Request { command, reply });
        answer.recv().unwrap()
    }

    fn tone(&mut self, seconds: f32) {
        let samples =
            (0..(SAMPLE_RATE as f32 * seconds) as usize).map(|i| 0.2 * (i as f32 * 0.3 + 0.5).sin()).collect();
        self.daemon.on_capture(CaptureEvent::Samples(samples));
    }

    /// Delivers the next engine result like the main loop would.
    fn pump_result(&mut self) {
        match self.messages.recv_timeout(Duration::from_secs(5)).expect("engine result") {
            Msg::Transcribed { job, result } => self.daemon.on_transcribed(job, result),
            _ => panic!("unexpected message"),
        }
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn views(&self) -> Vec<View> {
        self.overlay
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|cmd| if let OverlayCmd::Show(v) = cmd { Some(v.clone()) } else { None })
            .collect()
    }
}

#[test]
fn dictation_ends_in_a_paste_into_the_original_window() {
    let mut h = harness(" [BLANK_AUDIO] Hello world. ");
    assert_eq!(h.request(Command::Toggle), "warming-up");
    // Digital silence first, as from a Bluetooth headset switching profiles.
    h.daemon.on_capture(CaptureEvent::Samples(vec![0.0; 1600]));
    assert_eq!(h.request(Command::Status), "warming-up");
    h.tone(1.0);
    assert_eq!(h.request(Command::Status), "recording");
    assert_eq!(h.request(Command::Toggle), "transcribing");
    h.pump_result();
    assert_eq!(h.request(Command::Status), "idle");

    assert_eq!(
        h.log(),
        [
            "media pause",
            "capture start",
            "capture stop",
            "engine 16000 samples",
            "paste \"Hello world.\" into 0x1",
            "media resume"
        ]
    );
    assert_eq!(h.views(), [View::WarmingUp, View::Recording, View::Transcribing, View::Hidden]);
}

#[test]
fn leading_silence_is_dropped_and_levels_reach_the_overlay() {
    let mut h = harness("x");
    h.request(Command::Toggle);
    h.daemon.on_capture(CaptureEvent::Samples(vec![0.0; 8000]));
    assert_eq!(h.daemon.recording.samples.len(), 0);
    h.tone(0.5);
    assert_eq!(h.daemon.recording.samples.len(), 8000);
    let levels: usize =
        h.overlay.0.lock().unwrap().iter().map(|cmd| if let OverlayCmd::Levels(c) = cmd { c.len() } else { 0 }).sum();
    assert!(levels > 20, "{levels} levels");
}

#[test]
fn silent_recording_skips_the_engine_and_flashes() {
    let mut h = harness("should not be called");
    h.request(Command::Toggle);
    h.daemon.on_capture(CaptureEvent::Samples(vec![1e-4; 16000]));
    h.request(Command::Toggle);
    h.pump_result();
    assert!(!h.log().iter().any(|l| l.starts_with("engine") || l.starts_with("paste")));
    assert!(h.log().contains(&"media resume".to_string()));
    assert!(matches!(h.views().last(), Some(View::Flash(f)) if f.text == "nothing heard"));
}

#[test]
fn only_non_speech_tokens_paste_nothing() {
    let mut h = harness("[BLANK_AUDIO]");
    h.request(Command::Toggle);
    h.tone(1.0);
    h.request(Command::Toggle);
    h.pump_result();
    assert!(!h.log().iter().any(|l| l.starts_with("paste")));
    assert!(matches!(h.views().last(), Some(View::Flash(f)) if f.text == "nothing heard"));
}

#[test]
fn cancel_while_transcribing_discards_the_late_result() {
    let mut h = harness("Hello");
    h.request(Command::Toggle);
    h.tone(1.0);
    h.request(Command::Toggle);
    assert_eq!(h.request(Command::Cancel), "idle");
    h.pump_result();
    assert!(!h.log().iter().any(|l| l.starts_with("paste")));
    assert_eq!(h.views().last(), Some(&View::Hidden));
}

#[test]
fn cancel_while_recording_discards_audio() {
    let mut h = harness("Hello");
    h.request(Command::Toggle);
    h.tone(1.0);
    assert_eq!(h.request(Command::Cancel), "idle");
    assert_eq!(h.log(), ["media pause", "capture start", "capture stop", "media resume"]);
}

#[test]
fn no_audio_flashes_no_microphone_when_the_stream_never_came_up() {
    let mut h = harness("x");
    h.request(Command::Toggle);
    h.daemon.recording.started = Some(Instant::now() - Duration::from_secs(6));
    h.daemon.on_tick();
    assert_eq!(h.request(Command::Status), "idle");
    assert!(matches!(h.views().last(), Some(View::Flash(f)) if f.text == "no microphone"));
}

#[test]
fn zeros_only_flashes_mic_did_not_start() {
    let mut h = harness("x");
    h.request(Command::Toggle);
    h.daemon.on_capture(CaptureEvent::Samples(vec![0.0; 1600]));
    h.daemon.recording.started = Some(Instant::now() - Duration::from_secs(6));
    h.daemon.on_tick();
    assert!(matches!(h.views().last(), Some(View::Flash(f)) if f.text == "mic didn't start"));
}

#[test]
fn max_length_stops_and_transcribes() {
    let mut h = harness("Long text");
    h.daemon.config.max_length_secs = 1;
    h.request(Command::Toggle);
    h.tone(1.5);
    h.daemon.on_tick();
    assert_eq!(h.request(Command::Status), "transcribing");
    h.pump_result();
    assert!(h.log().contains(&"paste \"Long text\" into 0x1".to_string()));
}

#[test]
fn dictation_during_engine_warm_up_records_now_and_transcribes_after_it() {
    let log: Log = Arc::default();
    let (release, warm_up_gate) = mpsc::channel();
    let mut h = harness_with(log.clone(), Box::new(SlowWarmEngine { warm_up_gate, calls: log }));

    // The warm-up is still running: recording starts and stops as usual.
    assert_eq!(h.request(Command::Toggle), "warming-up");
    h.tone(1.0);
    assert_eq!(h.request(Command::Status), "recording");
    assert_eq!(h.request(Command::Toggle), "transcribing");
    assert!(matches!(h.views().last(), Some(View::Transcribing)));

    // No result while the engine thread is busy warming up, and nothing is pasted.
    assert!(h.messages.recv_timeout(Duration::from_millis(300)).is_err());
    assert!(!h.log().iter().any(|l| l.starts_with("engine") || l.starts_with("paste")));

    release.send(()).unwrap();
    h.pump_result();
    assert_eq!(h.request(Command::Status), "idle");
    let log = h.log();
    let position = |entry: &str| log.iter().position(|l| l == entry).unwrap_or_else(|| panic!("{entry} in {log:?}"));
    assert!(position("warm-up done") < position("engine 16000 samples"));
    assert!(position("engine 16000 samples") < position("paste \"Hello\" into 0x1"));
}
