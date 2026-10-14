//! Pauses MPRIS players through `playerctl` and resumes the ones this paused.

use std::process::{Command, Stdio};

use super::Media;

#[derive(Default)]
pub struct Playerctl {
    paused: Vec<String>,
}

fn playerctl(args: &[&str]) -> Option<String> {
    let out = Command::new("playerctl").args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl Media for Playerctl {
    fn pause(&mut self) {
        self.paused.clear();
        let Some(players) = playerctl(&["--list-all"]) else { return };
        for player in players.lines().filter(|l| !l.is_empty()) {
            if playerctl(&["--player", player, "status"]).as_deref() == Some("Playing")
                && playerctl(&["--player", player, "pause"]).is_some()
            {
                self.paused.push(player.to_string());
            }
        }
    }

    fn resume(&mut self) {
        for player in self.paused.drain(..) {
            playerctl(&["--player", &player, "play"]);
        }
    }
}
