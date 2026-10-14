//! `$XDG_CONFIG_HOME/rustle/config.toml`. Every key is optional.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::paths;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub model: Option<PathBuf>,
    /// ISO code such as "en", or "auto" to let the model detect it.
    pub language: String,
    pub max_length_secs: u64,
    /// Run the model on the GPU (Vulkan). CPU is far slower and meant for machines without a usable GPU.
    pub gpu: bool,
    /// PipeWire node name or serial of the microphone; the default source when unset.
    pub source: Option<String>,
    pub overlay: OverlayConfig,
    pub paste: PasteConfig,
    /// Hyprland submap that binds Escape to `rustle cancel`; entered while recording or transcribing.
    pub cancel_submap: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct OverlayConfig {
    pub width: u32,
    pub height: u32,
    pub bottom_margin: i32,
    pub radius: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PasteConfig {
    /// Modifiers held together with V, in Hyprland `sendshortcut` syntax.
    pub keys: String,
    pub terminal_keys: String,
    /// Case-insensitive substrings of the window class that identify a terminal.
    pub terminal_classes: Vec<String>,
    /// Exact (case-insensitive) class to modifiers, overriding the two defaults above.
    pub classes: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: None,
            language: "auto".into(),
            max_length_secs: 300,
            gpu: true,
            source: None,
            overlay: OverlayConfig::default(),
            paste: PasteConfig::default(),
            cancel_submap: None,
        }
    }
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self { width: 360, height: 56, bottom_margin: 48, radius: 12.0 }
    }
}

impl Default for PasteConfig {
    fn default() -> Self {
        Self {
            keys: "CTRL".into(),
            terminal_keys: "CTRL SHIFT".into(),
            terminal_classes: ["ghostty", "alacritty", "kitty", "foot", "konsole", "terminal", "wezterm"]
                .map(String::from)
                .to_vec(),
            classes: HashMap::new(),
        }
    }
}

impl PasteConfig {
    /// Modifiers to press together with V in a window of this class.
    pub fn modifiers_for(&self, class: &str) -> &str {
        let class = class.to_lowercase();
        if let Some((_, keys)) = self.classes.iter().find(|(name, _)| name.to_lowercase() == class) {
            return keys;
        }
        if self.terminal_classes.iter().any(|t| class.contains(&t.to_lowercase())) {
            &self.terminal_keys
        } else {
            &self.keys
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).context("invalid config")
    }

    /// A missing file yields the defaults; a malformed one is an error.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).with_context(|| path.display().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| path.display().to_string()),
        }
    }

    pub fn load_default() -> Result<Self> {
        Self::load(&paths::config_dir().join("config.toml"))
    }

    /// Whisper language code, `None` for auto-detection.
    pub fn language_code(&self) -> Option<&str> {
        match self.language.as_str() {
            "" | "auto" => None,
            code => Some(code),
        }
    }

    pub fn model_path(&self) -> Result<PathBuf> {
        if let Some(path) = &self.model {
            if path.is_file() {
                return Ok(path.clone());
            }
            bail!("configured model {} does not exist", path.display());
        }
        let candidates = paths::model_candidates();
        match candidates.iter().find(|p| p.is_file()) {
            Some(path) => Ok(path.clone()),
            None => bail!(
                "no model found; looked in {}",
                candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn partial_override_keeps_other_defaults() {
        let c = Config::parse("language = \"de\"\n[overlay]\nwidth = 400\n").unwrap();
        assert_eq!(c.language_code(), Some("de"));
        assert_eq!(c.overlay.width, 400);
        assert_eq!(c.overlay.height, 56);
        assert_eq!(c.max_length_secs, 300);
    }

    #[test]
    fn auto_language_means_detection() {
        assert_eq!(Config::default().language_code(), None);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::parse("languge = \"de\"").is_err());
    }

    #[test]
    fn paste_keys_by_window_class() {
        let mut c = Config::default().paste;
        assert_eq!(c.modifiers_for("com.mitchellh.ghostty"), "CTRL SHIFT");
        assert_eq!(c.modifiers_for("chromium-browser"), "CTRL");
        c.classes.insert("Emacs".into(), "ALT".into());
        assert_eq!(c.modifiers_for("emacs"), "ALT");
    }

    #[test]
    fn documented_example_matches_the_defaults() {
        assert_eq!(Config::parse(include_str!("../contrib/config.toml")).unwrap(), Config::default());
    }

    #[test]
    fn missing_file_is_default() {
        assert_eq!(Config::load(Path::new("/nonexistent/rustle.toml")).unwrap(), Config::default());
    }
}
