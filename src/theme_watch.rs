//! Reloads the theme when its file changes.

use anyhow::Result;
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::overlay::{OverlayCmd, OverlaySink};
use crate::theme::Theme;

/// Watches the directory rather than the file: theme writers often replace the file by renaming.
pub fn spawn(path: PathBuf, sink: Arc<dyn OverlaySink>) -> Result<()> {
    let dir = path.parent().map(PathBuf::from).unwrap_or_default();
    std::fs::create_dir_all(&dir)?;
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let _ = tx.send(event);
    })?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    std::thread::Builder::new().name("theme-watch".into()).spawn(move || {
        let _watcher = watcher; // dropping it would stop the watch
        for event in rx.iter() {
            let Ok(event) = event else { continue };
            // Reading the file for a reload must not count as a change.
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) {
                continue;
            }
            if !event.paths.iter().any(|p| p.file_name() == path.file_name()) {
                continue;
            }
            // Let a burst of events settle before reading.
            std::thread::sleep(Duration::from_millis(100));
            while rx.try_recv().is_ok() {}
            match Theme::load(&path) {
                Ok(theme) => {
                    tracing::info!("theme reloaded");
                    sink.send(OverlayCmd::Theme(theme));
                }
                Err(e) => tracing::warn!("keeping the old theme: {e:#}"),
            }
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Collect(Mutex<mpsc::Sender<OverlayCmd>>);

    impl OverlaySink for Collect {
        fn send(&self, cmd: OverlayCmd) {
            let _ = self.0.lock().unwrap().send(cmd);
        }
    }

    fn accent_of(cmd: OverlayCmd) -> crate::theme::Color {
        match cmd {
            OverlayCmd::Theme(t) => t.accent,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reloads_on_write_and_on_replace_without_looping() {
        let dir = std::env::temp_dir().join(format!("oxivox-theme-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme.toml");
        let (tx, rx) = mpsc::channel();
        spawn(path.clone(), Arc::new(Collect(Mutex::new(tx)))).unwrap();
        let wait = Duration::from_secs(3);

        std::fs::write(&path, "accent = \"#112233\"").unwrap();
        assert_eq!(accent_of(rx.recv_timeout(wait).unwrap()), crate::theme::Color::rgb(0x11, 0x22, 0x33));

        // Writers like Noctalia may write a temporary file and rename it over the theme.
        std::fs::write(dir.join("theme.toml.tmp"), "accent = \"#445566\"").unwrap();
        std::fs::rename(dir.join("theme.toml.tmp"), &path).unwrap();
        loop {
            let accent = accent_of(rx.recv_timeout(wait).unwrap());
            if accent == crate::theme::Color::rgb(0x44, 0x55, 0x66) {
                break;
            }
        }

        // Reloading reads the file, which must not trigger yet another reload.
        for _ in 0..3 {
            if rx.recv_timeout(Duration::from_millis(600)).is_err() {
                break;
            }
        }
        assert!(rx.recv_timeout(Duration::from_millis(800)).is_err(), "reload loop");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
