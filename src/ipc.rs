//! CLI to daemon protocol: one line request, one line reply, over a Unix socket.

use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Cancel,
    Status,
}

impl Command {
    pub fn parse(line: &str) -> Option<Self> {
        match line.trim() {
            "toggle" => Some(Self::Toggle),
            "cancel" => Some(Self::Cancel),
            "status" => Some(Self::Status),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::Cancel => "cancel",
            Self::Status => "status",
        }
    }
}

/// A command from a trigger together with the way to answer it.
#[derive(Debug)]
pub struct Request {
    pub command: Command,
    pub reply: Sender<String>,
}

/// Sends one command to the running daemon and returns its reply.
pub fn send(socket: &Path, command: Command) -> Result<String> {
    let mut stream = UnixStream::connect(socket).with_context(|| {
        format!("cannot reach the wisprs daemon at {} (is `wisprs daemon` running?)", socket.display())
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    writeln!(stream, "{}", command.name())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).context("no reply from the daemon")?;
    let reply = reply.trim().to_string();
    match reply.strip_prefix("error: ") {
        Some(message) => bail!("{message}"),
        None => Ok(reply),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_commands_only() {
        assert_eq!(Command::parse("toggle\n"), Some(Command::Toggle));
        assert_eq!(Command::parse(" cancel "), Some(Command::Cancel));
        assert_eq!(Command::parse("status"), Some(Command::Status));
        assert_eq!(Command::parse("rm -rf"), None);
    }

    #[test]
    fn names_round_trip() {
        for c in [Command::Toggle, Command::Cancel, Command::Status] {
            assert_eq!(Command::parse(c.name()), Some(c));
        }
    }
}
