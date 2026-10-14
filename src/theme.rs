//! Overlay palette. Read from `$XDG_CONFIG_HOME/wisprs/theme.toml`, Tokyo Night when absent.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Accepts `#rrggbb` and `#rrggbbaa` (the `#` is optional).
    pub fn parse(s: &str) -> Result<Self> {
        let hex = s.trim().trim_start_matches('#');
        if !hex.is_ascii() || !(hex.len() == 6 || hex.len() == 8) {
            bail!("invalid colour {s:?}, expected #rrggbb or #rrggbbaa");
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).with_context(|| format!("invalid colour {s:?}"));
        Ok(Self { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: if hex.len() == 8 { byte(6)? } else { 255 } })
    }

    pub fn with_alpha(self, alpha: f32) -> Self {
        Self { a: (self.a as f32 * alpha.clamp(0.0, 1.0)).round() as u8, ..self }
    }

    /// Linear blend towards `other`; the alpha channel is blended too.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self { r: lerp(self.r, other.r), g: lerp(self.g, other.g), b: lerp(self.b, other.b), a: lerp(self.a, other.a) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub border: Color,
    pub accent: Color,
    pub text: Color,
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::rgb(0x1a, 0x1b, 0x26),
            surface: Color::rgb(0x24, 0x28, 0x3b),
            border: Color::rgb(0x41, 0x48, 0x68),
            accent: Color::rgb(0x7a, 0xa2, 0xf7),
            text: Color::rgb(0xc0, 0xca, 0xf5),
            error: Color::rgb(0xf7, 0x76, 0x8e),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawTheme {
    background: Option<String>,
    surface: Option<String>,
    border: Option<String>,
    accent: Option<String>,
    text: Option<String>,
    error: Option<String>,
}

impl Theme {
    /// Keys missing from the text fall back to Tokyo Night individually.
    pub fn parse(text: &str) -> Result<Self> {
        let raw: RawTheme = toml::from_str(text).context("invalid theme")?;
        let mut theme = Self::default();
        let slots = [
            (&mut theme.background, raw.background),
            (&mut theme.surface, raw.surface),
            (&mut theme.border, raw.border),
            (&mut theme.accent, raw.accent),
            (&mut theme.text, raw.text),
            (&mut theme.error, raw.error),
        ];
        for (slot, value) in slots {
            if let Some(value) = value {
                *slot = Color::parse(&value)?;
            }
        }
        Ok(theme)
    }

    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).with_context(|| path.display().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| path.display().to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colours() {
        assert_eq!(Color::parse("#7aa2f7").unwrap(), Color::rgb(0x7a, 0xa2, 0xf7));
        assert_eq!(Color::parse("ff000080").unwrap(), Color { r: 255, g: 0, b: 0, a: 0x80 });
        assert!(Color::parse("#12345").is_err());
        assert!(Color::parse("#gggggg").is_err());
    }

    #[test]
    fn missing_keys_fall_back_to_tokyo_night() {
        let t = Theme::parse("accent = \"#ff9e64\"").unwrap();
        assert_eq!(t.accent, Color::rgb(0xff, 0x9e, 0x64));
        assert_eq!(t.background, Theme::default().background);
    }

    #[test]
    fn bad_colour_is_an_error() {
        assert!(Theme::parse("accent = \"blue\"").is_err());
    }

    #[test]
    fn missing_file_is_default() {
        assert_eq!(Theme::load(Path::new("/nonexistent/theme.toml")).unwrap(), Theme::default());
    }

    #[test]
    fn mix_endpoints() {
        let (a, b) = (Color::rgb(0, 0, 0), Color::rgb(200, 100, 50));
        assert_eq!(a.mix(b, 0.0), a);
        assert_eq!(a.mix(b, 1.0), b);
    }
}
