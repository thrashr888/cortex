use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_consolidation")]
    pub consolidation: ConsolidationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    #[serde(default = "default_threshold")]
    pub auto_micro_threshold: u32,
    #[serde(default = "default_decay")]
    pub decay_threshold: f64,
    #[serde(default = "default_model")]
    pub model: String,
    /// Extract entities with an LLM call on every save. Turn it off for always-on agents that
    /// save often; `sleep` then extracts the unprocessed memories in one batched call.
    #[serde(default = "default_extract_on_save")]
    pub extract_on_save: bool,
}

fn default_consolidation() -> ConsolidationConfig {
    ConsolidationConfig::default()
}
fn default_threshold() -> u32 { 10 }
fn default_decay() -> f64 { 0.1 }
fn default_model() -> String { "claude-haiku-5-5".to_string() }
fn default_extract_on_save() -> bool { true }

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            auto_micro_threshold: default_threshold(),
            decay_threshold: default_decay(),
            model: default_model(),
            extract_on_save: default_extract_on_save(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            consolidation: ConsolidationConfig::default(),
        }
    }
}

/// Reads `config.toml`, then applies `CORTEX_MODEL` and `CORTEX_EXTRACT_ON_SAVE` from the
/// environment, so a host running several agents can set them per agent without editing files.
pub fn load_config(cortex_dir: &Path) -> Result<Config> {
    let config_path = cortex_dir.join("config.toml");
    let mut config: Config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        toml::from_str(&content)?
    } else {
        Config::default()
    };
    apply_env(&mut config, |key| std::env::var(key).ok());
    Ok(config)
}

fn apply_env(config: &mut Config, var: impl Fn(&str) -> Option<String>) {
    if let Some(model) = var("CORTEX_MODEL").filter(|m| !m.trim().is_empty()) {
        config.consolidation.model = model.trim().to_string();
    }
    if let Some(value) = var("CORTEX_EXTRACT_ON_SAVE") {
        match value.trim().to_lowercase().as_str() {
            "0" | "false" | "no" | "off" => config.consolidation.extract_on_save = false,
            "1" | "true" | "yes" | "on" => config.consolidation.extract_on_save = true,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_overrides_model_and_extraction() {
        let mut config = Config::default();
        assert!(config.consolidation.extract_on_save);
        apply_env(&mut config, |key| match key {
            "CORTEX_MODEL" => Some("claude-sonnet-5-5".into()),
            "CORTEX_EXTRACT_ON_SAVE" => Some("off".into()),
            _ => None,
        });
        assert_eq!(config.consolidation.model, "claude-sonnet-5-5");
        assert!(!config.consolidation.extract_on_save);
    }

    #[test]
    fn old_config_files_keep_extracting_on_save() {
        let config: Config = toml::from_str("[consolidation]\nmodel = \"claude-haiku-4-5\"\n").unwrap();
        assert!(config.consolidation.extract_on_save);
        assert_eq!(config.consolidation.model, "claude-haiku-4-5");
    }
}
