//! Persistent Forge AI provider configuration (API keys, models).
//!
//! Stored at `{config_dir}/oxide/forge_config.json`. Environment variables
//! (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `XAI_API_KEY`)
//! are merged on load and do not overwrite saved keys.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Supported LLM backends for Oxide Forge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeProvider {
    Anthropic,
    Openai,
    Gemini,
    Xai,
}

impl ForgeProvider {
    pub const ALL: [ForgeProvider; 4] = [
        ForgeProvider::Anthropic,
        ForgeProvider::Openai,
        ForgeProvider::Gemini,
        ForgeProvider::Xai,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ForgeProvider::Anthropic => "anthropic",
            ForgeProvider::Openai => "openai",
            ForgeProvider::Gemini => "gemini",
            ForgeProvider::Xai => "xai",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ForgeProvider::Anthropic => "Anthropic",
            ForgeProvider::Openai => "OpenAI",
            ForgeProvider::Gemini => "Google Gemini",
            ForgeProvider::Xai => "xAI",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            ForgeProvider::Anthropic => "claude-opus-4-7",
            ForgeProvider::Openai => "gpt-4o",
            // Stable id per https://ai.google.dev/gemini-api/docs/models
            ForgeProvider::Gemini => "gemini-2.5-flash",
            ForgeProvider::Xai => "grok-2-latest",
        }
    }

    pub fn env_var(self) -> &'static str {
        match self {
            ForgeProvider::Anthropic => "ANTHROPIC_API_KEY",
            ForgeProvider::Openai => "OPENAI_API_KEY",
            ForgeProvider::Gemini => "GEMINI_API_KEY",
            ForgeProvider::Xai => "XAI_API_KEY",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        match s {
            "anthropic" => Some(ForgeProvider::Anthropic),
            "openai" => Some(ForgeProvider::Openai),
            "gemini" => Some(ForgeProvider::Gemini),
            "xai" => Some(ForgeProvider::Xai),
            _ => None,
        }
    }

    /// Map persisted model ids to a currently available API model name.
    pub fn normalize_model(self, model: &str) -> String {
        let model = model.trim();
        if model.is_empty() {
            return self.default_model().to_string();
        }
        match self {
            ForgeProvider::Gemini => normalize_gemini_model(model),
            _ => model.to_string(),
        }
    }
}

/// Gemini 2.0 and older ids return 404 — see [model deprecations](https://ai.google.dev/gemini-api/docs/models).
pub fn normalize_gemini_model(model: &str) -> String {
    match model.trim() {
        "gemini-2.0-flash"
        | "gemini-2.0-flash-lite"
        | "gemini-2.0-flash-001"
        | "gemini-2.0-flash-lite-001"
        | "gemini-1.5-flash"
        | "gemini-1.5-flash-8b"
        | "gemini-1.5-pro"
        | "gemini-pro"
        | "gemini-3-pro-preview"
        | "gemini-3-pro" => ForgeProvider::Gemini.default_model().to_string(),
        other => other.to_string(),
    }
}

impl fmt::Display for ForgeProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Per-provider credentials and model choice.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ForgeProviderSettings {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
}

impl ForgeProviderSettings {
    pub fn model_or_default(&self, provider: ForgeProvider) -> String {
        provider.normalize_model(&self.model)
    }

    pub fn has_key(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

/// User-facing Forge configuration persisted on disk.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForgeUserConfig {
    #[serde(default = "default_active_provider")]
    pub active_provider: ForgeProvider,
    #[serde(default)]
    pub providers: HashMap<String, ForgeProviderSettings>,
    #[serde(default)]
    pub settings_open: bool,
}

fn default_active_provider() -> ForgeProvider {
    ForgeProvider::Anthropic
}

impl Default for ForgeUserConfig {
    fn default() -> Self {
        Self {
            active_provider: ForgeProvider::Anthropic,
            providers: HashMap::new(),
            settings_open: false,
        }
    }
}

impl ForgeUserConfig {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("oxide")
            .join("forge_config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        let mut cfg = if path.is_file() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            ForgeUserConfig::default()
        };
        cfg.merge_env_keys();
        cfg.migrate_deprecated_models();
        cfg
    }

    /// Rewrite deprecated Gemini model strings in saved config.
    fn migrate_deprecated_models(&mut self) {
        let mut changed = false;
        if let Some(entry) = self.providers.get_mut(ForgeProvider::Gemini.id()) {
            let normalized = ForgeProvider::Gemini.normalize_model(&entry.model);
            if entry.model != normalized {
                entry.model = normalized;
                changed = true;
            }
        }
        if changed {
            let _ = self.save();
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("serialise forge config")?;
        std::fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Fill missing keys from environment variables (does not overwrite saved keys).
    pub fn merge_env_keys(&mut self) {
        for provider in ForgeProvider::ALL {
            if let Ok(key) = std::env::var(provider.env_var()) {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    let entry = self.provider_mut(provider);
                    if !entry.has_key() {
                        entry.api_key = trimmed.to_string();
                    }
                }
            }
        }
        // Alternate env name for Gemini.
        if let Ok(key) = std::env::var("GOOGLE_API_KEY") {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                let entry = self.provider_mut(ForgeProvider::Gemini);
                if !entry.has_key() {
                    entry.api_key = trimmed.to_string();
                }
            }
        }
    }

    pub fn provider(&self, p: ForgeProvider) -> ForgeProviderSettings {
        self.providers
            .get(p.id())
            .cloned()
            .unwrap_or_default()
    }

    pub fn provider_mut(&mut self, p: ForgeProvider) -> &mut ForgeProviderSettings {
        self.providers
            .entry(p.id().to_string())
            .or_default()
    }

    pub fn active_settings(&self) -> ForgeProviderSettings {
        self.provider(self.active_provider)
    }

    pub fn active_api_key(&self) -> Option<String> {
        let key = self.active_settings().api_key.trim().to_string();
        if key.is_empty() {
            None
        } else {
            Some(key)
        }
    }

    pub fn active_model(&self) -> String {
        self.active_settings()
            .model_or_default(self.active_provider)
    }

    pub fn any_provider_configured(&self) -> bool {
        ForgeProvider::ALL
            .iter()
            .any(|p| self.provider(*p).has_key())
    }

    pub fn configured_providers(&self) -> Vec<ForgeProvider> {
        ForgeProvider::ALL
            .iter()
            .filter(|p| self.provider(**p).has_key())
            .copied()
            .collect()
    }

    pub fn set_api_key(&mut self, provider: ForgeProvider, key: String) {
        let key = key.trim().to_string();
        if key.is_empty() {
            return;
        }
        self.provider_mut(provider).api_key = key;
    }

    pub fn set_model(&mut self, provider: ForgeProvider, model: String) {
        self.provider_mut(provider).model = model.trim().to_string();
    }
}

/// Mask an API key for display (`sk-…abcd`).
pub fn mask_api_key(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        return String::new();
    }
    if key.len() <= 8 {
        return "•".repeat(key.chars().count());
    }
    let prefix: String = key.chars().take(4).collect();
    let suffix: String = key.chars().rev().take(4).collect::<String>().chars().rev().collect();
    format!("{prefix}…{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_long_keys() {
        let m = mask_api_key("sk-ant-api03-abcdefghijklmnop");
        assert!(m.contains('…'));
        assert!(m.starts_with("sk-a"));
    }

    #[test]
    fn migrates_deprecated_gemini_models() {
        assert_eq!(
            normalize_gemini_model("gemini-2.0-flash"),
            "gemini-2.5-flash"
        );
        assert_eq!(
            normalize_gemini_model("gemini-3-pro-preview"),
            "gemini-2.5-flash"
        );
        assert_eq!(
            normalize_gemini_model("gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
    }
}
