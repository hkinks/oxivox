//! Pauses MPRIS players while dictating and resumes the ones this paused.
//!
//! Resuming waits for a Bluetooth headset to leave call mode: WirePlumber switches it back to A2DP a moment after
//! the microphone stream closes, which recreates the sink, and players that start before that pause themselves
//! on the lost output device.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::Media;

pub trait Players: Send + Sync + 'static {
    fn list(&self) -> Result<Vec<String>, String>;
    fn status(&self, player: &str) -> Result<String, String>;
    fn pause(&self, player: &str) -> Result<(), String>;
    fn play(&self, player: &str) -> Result<(), String>;
}

pub trait OutputProbe: Send + Sync + 'static {
    /// The default output is a Bluetooth headset still in its telephony profile.
    fn in_call_mode(&self) -> bool;
}

/// MPRIS through the `playerctl` command.
pub struct Playerctl;

fn playerctl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("playerctl")
        .args(args)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("cannot run playerctl: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!("{}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim()))
    }
}

impl Players for Playerctl {
    fn list(&self) -> Result<Vec<String>, String> {
        Ok(playerctl(&["--list-all"])?.lines().filter(|l| !l.is_empty()).map(String::from).collect())
    }

    fn status(&self, player: &str) -> Result<String, String> {
        playerctl(&["--player", player, "status"])
    }

    fn pause(&self, player: &str) -> Result<(), String> {
        playerctl(&["--player", player, "pause"]).map(drop)
    }

    fn play(&self, player: &str) -> Result<(), String> {
        playerctl(&["--player", player, "play"]).map(drop)
    }
}

#[derive(Clone, Copy)]
pub struct Timing {
    pub poll: Duration,
    /// Resume anyway after this long.
    pub give_up: Duration,
    /// Extra wait after the headset is back, for the sink to reappear.
    pub settle: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self { poll: Duration::from_millis(150), give_up: Duration::from_secs(5), settle: Duration::from_millis(400) }
    }
}

pub struct MediaControl {
    players: Arc<dyn Players>,
    probe: Arc<dyn OutputProbe>,
    /// Players to leave alone: the full MPRIS name or the part before the instance suffix.
    ignore: Vec<String>,
    timing: Timing,
    /// Players this paused and has not resumed yet.
    owed: Arc<Mutex<Vec<String>>>,
    /// Bumped by every pause, so a resume still waiting from the previous dictation gives way.
    generation: Arc<AtomicU64>,
}

impl MediaControl {
    pub fn new(players: Arc<dyn Players>, probe: Arc<dyn OutputProbe>, ignore: Vec<String>) -> Self {
        Self::with_timing(players, probe, ignore, Timing::default())
    }

    pub fn with_timing(
        players: Arc<dyn Players>,
        probe: Arc<dyn OutputProbe>,
        ignore: Vec<String>,
        timing: Timing,
    ) -> Self {
        Self { players, probe, ignore, timing, owed: Arc::default(), generation: Arc::default() }
    }

    fn is_ignored(&self, player: &str) -> bool {
        let base = player.split('.').next().unwrap_or(player);
        self.ignore.iter().any(|name| name == player || name == base)
    }

    /// Some players acknowledge `pause` before they stop; check, and ask once more if they did not.
    fn pause_and_confirm(&self, player: &str) -> Result<(), String> {
        for attempt in 0..2 {
            self.players.pause(player)?;
            for _ in 0..8 {
                if self.players.status(player)? != "Playing" {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            tracing::warn!(player, attempt, "media player still playing after pause");
        }
        Err("still playing after two pause requests".into())
    }
}

fn resume_when_settled(
    players: Arc<dyn Players>,
    probe: Arc<dyn OutputProbe>,
    owed: Arc<Mutex<Vec<String>>>,
    generation: Arc<AtomicU64>,
    mine: u64,
    timing: Timing,
) {
    let started = Instant::now();
    let mut waited = false;
    while probe.in_call_mode() {
        if !waited {
            tracing::info!("waiting for headset to leave call mode");
            waited = true;
        }
        if generation.load(Ordering::SeqCst) != mine {
            return;
        }
        if started.elapsed() >= timing.give_up {
            tracing::warn!(seconds = started.elapsed().as_secs_f32(), "headset still in call mode, resuming anyway");
            break;
        }
        std::thread::sleep(timing.poll);
    }
    if waited && started.elapsed() < timing.give_up {
        tracing::info!(seconds = started.elapsed().as_secs_f32(), "headset back in a2dp");
        std::thread::sleep(timing.settle);
    }
    // A new dictation started meanwhile: it owns the players now and its own resume will play them.
    if generation.load(Ordering::SeqCst) != mine {
        tracing::info!("new recording started, not resuming media yet");
        return;
    }
    let players_to_resume: Vec<String> = std::mem::take(&mut *owed.lock().unwrap());
    for player in players_to_resume {
        match players.play(&player) {
            Ok(()) => tracing::info!(player, "resumed media player"),
            Err(e) => tracing::warn!(player, "cannot resume media player: {e}"),
        }
    }
}

impl Media for MediaControl {
    fn pause(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let players = match self.players.list() {
            Ok(players) => players,
            // "No players found" is how playerctl reports an empty bus.
            Err(e) => return tracing::debug!("no media players: {e}"),
        };
        for player in players {
            if self.is_ignored(&player) {
                tracing::info!(player, "media player ignored");
                continue;
            }
            if self.owed.lock().unwrap().contains(&player) {
                continue; // still paused by the previous dictation, whose resume has not run yet
            }
            match self.players.status(&player) {
                Ok(status) if status == "Playing" => match self.pause_and_confirm(&player) {
                    Ok(()) => {
                        tracing::info!(player, "paused media player");
                        self.owed.lock().unwrap().push(player);
                    }
                    Err(e) => tracing::warn!(player, "cannot pause media player: {e}"),
                },
                Ok(status) => tracing::info!(player, status, "media player not playing"),
                Err(e) => tracing::warn!(player, "cannot read media player status: {e}"),
            }
        }
    }

    /// Returns at once; the players are resumed from a background thread when the output has settled.
    fn resume(&mut self) {
        if self.owed.lock().unwrap().is_empty() {
            return;
        }
        let mine = self.generation.load(Ordering::SeqCst);
        let (players, probe, owed, generation, timing) =
            (self.players.clone(), self.probe.clone(), self.owed.clone(), self.generation.clone(), self.timing);
        std::thread::Builder::new()
            .name("media-resume".into())
            .spawn(move || resume_when_settled(players, probe, owed, generation, mine, timing))
            .expect("spawning media resume thread");
    }
}

#[cfg(test)]
mod tests;
