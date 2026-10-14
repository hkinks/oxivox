//! Reloads the theme when its file changes.

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
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
