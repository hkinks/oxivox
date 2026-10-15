//! Trigger for compositors without a global-shortcut API: the user binds a key to `oxivox toggle`,
//! which reaches the daemon over a Unix socket.

use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use super::Trigger;
use crate::ipc::{Command, Request};

pub struct SocketTrigger {
    listener: UnixListener,
}

/// Removes the socket file when the daemon exits.
pub struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl SocketTrigger {
    /// Binds the socket now, so a second daemon is refused before any model gets loaded.
    pub fn bind(path: PathBuf) -> Result<(Self, SocketGuard)> {
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                bail!("another oxivox daemon is already listening on {}", path.display());
            }
            std::fs::remove_file(&path).context("removing stale socket")?;
        }
        let listener = UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
        Ok((Self { listener }, SocketGuard(path)))
    }
}

fn serve_connection(mut stream: UnixStream, on_request: &(dyn Fn(Request) + Send)) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    if line.is_empty() {
        return Ok(()); // probe: the peer connected and left again
    }
    let Some(command) = Command::parse(&line) else {
        writeln!(stream, "error: unknown command {:?}", line.trim())?;
        return Ok(());
    };
    let (reply, answer) = mpsc::channel();
    on_request(Request { command, reply });
    match answer.recv_timeout(Duration::from_secs(5)) {
        Ok(text) => writeln!(stream, "{text}")?,
        Err(_) => writeln!(stream, "error: daemon did not answer")?,
    }
    Ok(())
}

impl Trigger for SocketTrigger {
    fn spawn(self: Box<Self>, on_request: Box<dyn Fn(Request) + Send>) -> Result<()> {
        std::thread::Builder::new().name("socket-trigger".into()).spawn(move || {
            for stream in self.listener.incoming().flatten() {
                if let Err(e) = serve_connection(stream, &*on_request) {
                    tracing::warn!("socket request failed: {e:#}");
                }
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc;

    fn socket_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("oxivox-test-{}-{name}.sock", std::process::id()))
    }

    #[test]
    fn commands_reach_the_handler_and_replies_come_back() {
        let path = socket_path("roundtrip");
        let (trigger, _guard) = SocketTrigger::bind(path.clone()).unwrap();
        Box::new(trigger)
            .spawn(Box::new(|request| {
                let _ = request.reply.send(format!("got {}", request.command.name()));
            }))
            .unwrap();
        assert_eq!(ipc::send(&path, Command::Toggle).unwrap(), "got toggle");
        assert_eq!(ipc::send(&path, Command::Status).unwrap(), "got status");
    }

    #[test]
    fn a_second_daemon_is_refused_and_a_stale_socket_is_replaced() {
        let path = socket_path("exclusive");
        let (trigger, guard) = SocketTrigger::bind(path.clone()).unwrap();
        Box::new(trigger).spawn(Box::new(|_| {})).unwrap();
        assert!(SocketTrigger::bind(path.clone()).is_err());

        // A socket file nobody listens on is left over from a crashed daemon.
        let stale = socket_path("stale");
        drop(UnixListener::bind(&stale).unwrap());
        assert!(SocketTrigger::bind(stale).is_ok());
        drop(guard);
    }

    #[test]
    fn the_client_explains_a_missing_daemon() {
        let message = format!("{:#}", ipc::send(&socket_path("missing"), Command::Toggle).unwrap_err());
        assert!(message.contains("oxivox daemon"), "{message}");
    }
}
