use super::*;
use std::sync::atomic::AtomicBool;

#[derive(Default)]
struct FakePlayers {
    /// Player name to playing.
    state: Mutex<Vec<(String, bool)>>,
    log: Mutex<Vec<String>>,
}

impl FakePlayers {
    fn with(players: &[(&str, bool)]) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(players.iter().map(|(n, p)| (n.to_string(), *p)).collect()),
            ..Default::default()
        })
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn playing(&self, name: &str) -> bool {
        self.state.lock().unwrap().iter().any(|(n, p)| n == name && *p)
    }
}

impl Players for FakePlayers {
    fn list(&self) -> Result<Vec<String>, String> {
        Ok(self.state.lock().unwrap().iter().map(|(n, _)| n.clone()).collect())
    }

    fn status(&self, player: &str) -> Result<String, String> {
        let playing = self.playing(player);
        Ok(if playing { "Playing" } else { "Paused" }.into())
    }

    fn pause(&self, player: &str) -> Result<(), String> {
        self.log.lock().unwrap().push(format!("pause {player}"));
        self.state.lock().unwrap().iter_mut().filter(|(n, _)| n == player).for_each(|(_, p)| *p = false);
        Ok(())
    }

    fn play(&self, player: &str) -> Result<(), String> {
        self.log.lock().unwrap().push(format!("play {player}"));
        self.state.lock().unwrap().iter_mut().filter(|(n, _)| n == player).for_each(|(_, p)| *p = true);
        Ok(())
    }
}

/// Reports call mode until switched off.
struct FakeProbe(AtomicBool);

impl FakeProbe {
    fn in_call_mode(yes: bool) -> Arc<Self> {
        Arc::new(Self(AtomicBool::new(yes)))
    }
}

impl OutputProbe for FakeProbe {
    fn in_call_mode(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

const FAST: Timing =
    Timing { poll: Duration::from_millis(10), give_up: Duration::from_millis(600), settle: Duration::from_millis(30) };

fn control(players: &Arc<FakePlayers>, probe: &Arc<FakeProbe>, ignore: &[&str]) -> MediaControl {
    MediaControl::with_timing(players.clone(), probe.clone(), ignore.iter().map(|s| s.to_string()).collect(), FAST)
}

fn wait_for(condition: impl Fn() -> bool) -> bool {
    for _ in 0..200 {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn only_playing_players_are_paused_and_resumed() {
    let players = FakePlayers::with(&[("music", true), ("vlc", false)]);
    let mut media = control(&players, &FakeProbe::in_call_mode(false), &[]);
    media.pause();
    assert_eq!(players.log(), ["pause music"]);
    media.resume();
    assert!(wait_for(|| players.playing("music")));
    assert_eq!(players.log(), ["pause music", "play music"]);
}

#[test]
fn ignored_players_are_left_alone() {
    let players = FakePlayers::with(&[("chromium.instance1", true)]);
    let mut media = control(&players, &FakeProbe::in_call_mode(false), &["chromium"]);
    media.pause();
    assert!(players.playing("chromium.instance1"));
    assert!(players.log().is_empty());
}

#[test]
fn resume_waits_for_the_headset_to_leave_call_mode() {
    let players = FakePlayers::with(&[("music", true)]);
    let probe = FakeProbe::in_call_mode(true);
    let mut media = control(&players, &probe, &[]);
    media.pause();
    media.resume();
    std::thread::sleep(Duration::from_millis(150));
    assert!(!players.playing("music"), "resumed while the headset was still in call mode");
    probe.0.store(false, Ordering::SeqCst);
    assert!(wait_for(|| players.playing("music")));
}

#[test]
fn resume_gives_up_waiting_after_the_timeout() {
    let players = FakePlayers::with(&[("music", true)]);
    let mut media = control(&players, &FakeProbe::in_call_mode(true), &[]);
    media.pause();
    let started = Instant::now();
    media.resume();
    assert!(wait_for(|| players.playing("music")));
    assert!(started.elapsed() >= FAST.give_up);
}

#[test]
fn resume_returns_immediately_without_blocking_the_caller() {
    let players = FakePlayers::with(&[("music", true)]);
    let mut media = control(&players, &FakeProbe::in_call_mode(true), &[]);
    media.pause();
    let started = Instant::now();
    media.resume();
    assert!(started.elapsed() < Duration::from_millis(100));
}

#[test]
fn a_new_recording_takes_over_a_pending_resume() {
    let players = FakePlayers::with(&[("music", true)]);
    let probe = FakeProbe::in_call_mode(true);
    let mut media = control(&players, &probe, &[]);
    media.pause();
    media.resume();
    // Second dictation starts while the first resume still waits.
    media.pause();
    probe.0.store(false, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!players.playing("music"), "resumed during the second recording");
    media.resume();
    assert!(wait_for(|| players.playing("music")));
    assert_eq!(players.log(), ["pause music", "play music"]);
}

#[test]
fn nothing_to_resume_starts_no_thread_and_no_waiting() {
    let players = FakePlayers::with(&[("music", false)]);
    let mut media = control(&players, &FakeProbe::in_call_mode(true), &[]);
    media.pause();
    media.resume();
    std::thread::sleep(Duration::from_millis(50));
    assert!(players.log().is_empty());
}
