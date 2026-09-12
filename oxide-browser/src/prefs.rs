//! Persisted browser chrome preferences (appearance and page zoom).
//!
//! Stored at `{config_dir}/oxide/prefs.json`. Missing or corrupt files fall
//! back to system theme and 100% zoom.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// How the desktop chrome (and `api_system_theme`) should pick a colour scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    /// Follow the OS light/dark setting.
    #[default]
    System,
    /// Force the dark palette.
    Dark,
    /// Force the light palette.
    Light,
}

impl ThemePreference {
    /// Cycle System → Dark → Light → System.
    pub fn cycle(self) -> Self {
        match self {
            Self::System => Self::Dark,
            Self::Dark => Self::Light,
            Self::Light => Self::System,
        }
    }

    /// Short label for the settings page and toolbar.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    /// Resolve to a concrete dark/light choice using the OS when set to System.
    pub fn is_dark(self) -> bool {
        match self {
            Self::Dark => true,
            Self::Light => false,
            Self::System => matches!(dark_light::detect(), Ok(dark_light::Mode::Dark) | Err(_)),
        }
    }
}

/// Chrome zoom steps, matching common browser increments (percent).
const ZOOM_STEPS: &[u32] = &[50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];

/// User-facing browser preferences persisted between launches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrowserPrefs {
    #[serde(default)]
    pub theme: ThemePreference,
    /// Page zoom as a multiplier (`1.0` = 100%). Clamped to 50%–300%.
    #[serde(default = "default_zoom")]
    pub zoom: f32,
}

fn default_zoom() -> f32 {
    1.0
}

impl Default for BrowserPrefs {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            zoom: 1.0,
        }
    }
}

impl BrowserPrefs {
    /// `{config_dir}/oxide/prefs.json`.
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("oxide")
            .join("prefs.json")
    }

    /// Load from disk, or return defaults if the file is missing or invalid.
    pub fn load() -> Self {
        Self::load_from(&Self::config_path())
    }

    fn load_from(path: &std::path::Path) -> Self {
        if !path.is_file() {
            return Self::default();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .map(|mut p: Self| {
                p.zoom = clamp_zoom(p.zoom);
                p
            })
            .unwrap_or_default()
    }

    /// Write the current preferences to disk.
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::config_path())
    }

    fn save_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("serialise prefs")?;
        std::fs::write(path, json).with_context(|| format!("write {}", path.display()))
    }

    /// Nearest zoom step at or above the current value (caps at 300%).
    pub fn zoom_in(&mut self) {
        let pct = zoom_percent(self.zoom);
        self.zoom = ZOOM_STEPS
            .iter()
            .find(|&&s| s > pct)
            .map(|&s| s as f32 / 100.0)
            .unwrap_or(3.0);
    }

    /// Nearest zoom step below the current value (floors at 50%).
    pub fn zoom_out(&mut self) {
        let pct = zoom_percent(self.zoom);
        self.zoom = ZOOM_STEPS
            .iter()
            .rev()
            .find(|&&s| s < pct)
            .map(|&s| s as f32 / 100.0)
            .unwrap_or(0.5);
    }

    /// Reset page zoom to 100%.
    pub fn zoom_reset(&mut self) {
        self.zoom = 1.0;
    }

    /// `100` for 1.0×, `125` for 1.25×, etc.
    pub fn zoom_percent(&self) -> u32 {
        zoom_percent(self.zoom)
    }
}

/// Clamp a raw zoom multiplier to the supported range.
pub fn clamp_zoom(zoom: f32) -> f32 {
    if !zoom.is_finite() {
        return 1.0;
    }
    zoom.clamp(0.5, 3.0)
}

fn zoom_percent(zoom: f32) -> u32 {
    (clamp_zoom(zoom) * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_cycle_order() {
        assert_eq!(ThemePreference::System.cycle(), ThemePreference::Dark);
        assert_eq!(ThemePreference::Dark.cycle(), ThemePreference::Light);
        assert_eq!(ThemePreference::Light.cycle(), ThemePreference::System);
    }

    #[test]
    fn explicit_theme_ignores_os() {
        assert!(ThemePreference::Dark.is_dark());
        assert!(!ThemePreference::Light.is_dark());
    }

    #[test]
    fn zoom_steps_walk_the_ladder() {
        let mut p = BrowserPrefs::default();
        p.zoom_in();
        assert_eq!(p.zoom_percent(), 110);
        p.zoom_in();
        assert_eq!(p.zoom_percent(), 125);
        p.zoom_reset();
        assert_eq!(p.zoom_percent(), 100);
        p.zoom_out();
        assert_eq!(p.zoom_percent(), 90);
    }

    #[test]
    fn zoom_clamps_at_ends() {
        let mut p = BrowserPrefs {
            zoom: 3.0,
            ..BrowserPrefs::default()
        };
        p.zoom_in();
        assert_eq!(p.zoom_percent(), 300);
        p.zoom = 0.5;
        p.zoom_out();
        assert_eq!(p.zoom_percent(), 50);
    }

    #[test]
    fn clamp_rejects_non_finite() {
        assert_eq!(clamp_zoom(f32::NAN), 1.0);
        assert_eq!(clamp_zoom(f32::INFINITY), 1.0);
        assert_eq!(clamp_zoom(0.1), 0.5);
        assert_eq!(clamp_zoom(9.0), 3.0);
    }

    #[test]
    fn prefs_roundtrip_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let original = BrowserPrefs {
            theme: ThemePreference::Light,
            zoom: 1.25,
        };
        original.save_to(&path).unwrap();
        let loaded = BrowserPrefs::load_from(&path);
        assert_eq!(loaded, original);
    }

    #[test]
    fn corrupt_prefs_file_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(BrowserPrefs::load_from(&path), BrowserPrefs::default());
    }
}
