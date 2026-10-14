//! XDG locations used by rustle.

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home().join(fallback),
    }
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("rustle")
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

/// `$RUSTLE_SOCKET` overrides the default; the `--socket` option overrides both.
pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("RUSTLE_SOCKET") {
        return PathBuf::from(path);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    runtime.join("rustle.sock")
}

/// Default model locations, in order of preference.
pub fn model_candidates() -> Vec<PathBuf> {
    const FILE: &str = "ggml-large-v3-turbo.bin";
    let data = data_dir();
    vec![data.join("rustle/models").join(FILE), data.join("voxtype/models").join(FILE)]
}
