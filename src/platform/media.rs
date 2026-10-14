//! Pauses MPRIS players through `playerctl` and resumes the ones this paused.

use std::process::{Command, Stdio};

use super::Media;

#[derive(Default)]
pub struct Playerctl {
    /// Players to leave alone: the full MPRIS name or the part before the instance suffix.
    ignore: Vec<String>,
    paused: Vec<String>,
}

impl Playerctl {
    pub fn new(ignore: Vec<String>) -> Self {
        Self { ignore, paused: Vec::new() }
    }

    fn is_ignored(&self, player: &str) -> bool {
        let base = player.split('.').next().unwrap_or(player);
        self.ignore.iter().any(|name| name == player || name == base)
    }
}

/// Stdout of `playerctl args`, or the reason it failed.
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

/// Some players acknowledge `pause` before they stop; check, and ask once more if they did not.
fn pause_and_confirm(player: &str) -> Result<(), String> {
    for attempt in 0..2 {
        playerctl(&["--player", player, "pause"])?;
        for _ in 0..8 {
            if playerctl(&["--player", player, "status"])? != "Playing" {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        tracing::warn!(player, attempt, "media player still playing after pause");
    }
    Err("still playing after two pause requests".into())
}

impl Media for Playerctl {
    fn pause(&mut self) {
        self.paused.clear();
        let players = match playerctl(&["--list-all"]) {
            Ok(players) => players,
            // "No players found" is how playerctl reports an empty bus.
            Err(e) => return tracing::debug!("no media players: {e}"),
        };
        for player in players.lines().filter(|l| !l.is_empty()) {
            if self.is_ignored(player) {
                tracing::info!(player, "media player ignored");
                continue;
            }
            match playerctl(&["--player", player, "status"]) {
                Ok(status) if status == "Playing" => match pause_and_confirm(player) {
                    Ok(()) => {
                        tracing::info!(player, "paused media player");
                        self.paused.push(player.to_string());
                    }
                    Err(e) => tracing::warn!(player, "cannot pause media player: {e}"),
                },
                Ok(status) => tracing::info!(player, status, "media player not playing"),
                Err(e) => tracing::warn!(player, "cannot read media player status: {e}"),
            }
        }
    }

    fn resume(&mut self) {
        for player in self.paused.drain(..) {
            match playerctl(&["--player", &player, "play"]) {
                Ok(_) => tracing::info!(player, "resumed media player"),
                Err(e) => tracing::warn!(player, "cannot resume media player: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_matches_full_name_or_base() {
        let p = Playerctl::new(vec!["chromium".into(), "vlc".into()]);
        assert!(p.is_ignored("chromium.instance11459"));
        assert!(p.is_ignored("vlc"));
        assert!(!p.is_ignored("mpv"));
    }
}
