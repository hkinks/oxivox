//! Hyprland: window focus and key injection over its IPC socket, clipboard through wl-clipboard.

use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::{CancelKey, Focus, Output, Window};
use crate::config::PasteConfig;

#[derive(Clone)]
pub struct Hyprland {
    socket: PathBuf,
}

impl Hyprland {
    /// Needs `HYPRLAND_INSTANCE_SIGNATURE` and `XDG_RUNTIME_DIR` in the environment.
    pub fn from_env() -> Result<Self> {
        let signature =
            std::env::var("HYPRLAND_INSTANCE_SIGNATURE").context("HYPRLAND_INSTANCE_SIGNATURE is not set")?;
        let runtime = std::env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
        let socket = PathBuf::from(runtime).join("hypr").join(signature).join(".socket.sock");
        if !socket.exists() {
            bail!("Hyprland socket {} does not exist", socket.display());
        }
        Ok(Self { socket })
    }

    fn request(&self, command: &str) -> Result<String> {
        let mut stream = UnixStream::connect(&self.socket).context("connecting to Hyprland")?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.write_all(command.as_bytes())?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply)?;
        Ok(reply)
    }

    fn dispatch(&self, dispatcher: &str) -> Result<()> {
        let reply = self.request(&format!("/dispatch {dispatcher}"))?;
        if reply.trim() != "ok" {
            bail!("Hyprland rejected `{dispatcher}`: {}", reply.trim());
        }
        Ok(())
    }

    /// The window with this address, if it still exists.
    pub fn find_window(&self, address: &str) -> Option<Window> {
        let clients: serde_json::Value = serde_json::from_str(&self.request("j/clients").ok()?).ok()?;
        clients
            .as_array()?
            .iter()
            .find(|c| c["address"] == address)
            .map(|c| Window { id: address.to_string(), class: c["class"].as_str().unwrap_or_default().to_string() })
    }
}

impl Focus for Hyprland {
    fn active(&self) -> Result<Option<Window>> {
        let json: serde_json::Value = serde_json::from_str(&self.request("j/activewindow")?)?;
        Ok(json["address"].as_str().filter(|a| !a.is_empty()).map(|address| Window {
            id: address.to_string(),
            class: json["class"].as_str().unwrap_or_default().to_string(),
        }))
    }
}

/// Enters a Hyprland submap that the user's config binds Escape to `rustle cancel` in, and leaves it again.
pub struct SubmapCancel {
    hypr: Hyprland,
    name: String,
}

impl SubmapCancel {
    pub fn new(hypr: Hyprland, name: String) -> Self {
        Self { hypr, name }
    }
}

impl CancelKey for SubmapCancel {
    fn set_armed(&self, armed: bool) {
        let target = if armed { self.name.as_str() } else { "reset" };
        if let Err(e) = self.hypr.dispatch(&format!("submap {target}")) {
            tracing::warn!("submap change failed: {e:#}");
        }
    }
}

/// Clipboard content saved so that pasting leaves it untouched.
struct SavedClipboard {
    mime: String,
    data: Vec<u8>,
}

fn wl_paste(args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("wl-paste").args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then_some(out.stdout)
}

fn save_clipboard() -> Option<SavedClipboard> {
    let types = String::from_utf8(wl_paste(&["--list-types"])?).ok()?;
    let types: Vec<&str> = types.lines().collect();
    let mime = ["text/plain;charset=utf-8", "text/plain"]
        .into_iter()
        .find(|m| types.contains(m))
        .or_else(|| types.first().copied())?;
    let data = wl_paste(&["--no-newline", "--type", mime])?;
    Some(SavedClipboard { mime: mime.to_string(), data })
}

/// `wl-copy` forks into the background to serve the selection; waiting only waits for that fork.
/// `sensitive` asks clipboard managers not to record the content (wl-clipboard 2.2+; older versions reject the
/// option, in which case the copy is repeated without it).
fn wl_copy(mime: Option<&str>, data: &[u8], sensitive: bool) -> Result<()> {
    let attempt = |sensitive: bool| -> Result<bool> {
        let mut command = Command::new("wl-copy");
        if sensitive {
            command.arg("--sensitive");
        }
        if let Some(mime) = mime {
            command.args(["--type", mime]);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("running wl-copy")?;
        // A rejected option makes wl-copy exit before reading, which breaks the pipe.
        let _ = child.stdin.take().context("wl-copy stdin")?.write_all(data);
        Ok(child.wait()?.success())
    };
    if attempt(sensitive)? {
        return Ok(());
    }
    if sensitive {
        tracing::warn!("wl-copy rejected --sensitive; the text may reach clipboard history");
        if attempt(false)? {
            return Ok(());
        }
    }
    bail!("wl-copy failed")
}

pub struct HyprlandOutput {
    hypr: Hyprland,
    paste: PasteConfig,
}

impl HyprlandOutput {
    pub fn new(hypr: Hyprland, paste: PasteConfig) -> Self {
        Self { hypr, paste }
    }
}

impl Output for HyprlandOutput {
    fn deliver(&self, text: &str, target: Option<&Window>) -> Result<()> {
        let saved = save_clipboard();
        wl_copy(Some("text/plain;charset=utf-8"), text.as_bytes(), true)?;

        let current = self.hypr.active().ok().flatten();
        let target = target.filter(|w| self.hypr.find_window(&w.id).is_some()).cloned().or(current.clone());
        let Some(target) = target else { bail!("no window to paste into") };
        if current.as_ref().map(|w| &w.id) != Some(&target.id) {
            self.hypr.dispatch(&format!("focuswindow address:{}", target.id))?;
        }
        let mods = self.paste.modifiers_for(&target.class);
        self.hypr.dispatch(&format!("sendshortcut {mods}, V, address:{}", target.id))?;

        // The application reads the clipboard asynchronously after the key press.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            let restored = match &saved {
                Some(clip) => wl_copy(Some(&clip.mime), &clip.data, false),
                None => Command::new("wl-copy").arg("--clear").status().map(|_| ()).map_err(Into::into),
            };
            if let Err(e) = restored {
                tracing::warn!("restoring clipboard failed: {e:#}");
            }
        });
        Ok(())
    }
}
