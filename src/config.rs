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
}

fn default_consolidation() -> ConsolidationConfig {
    ConsolidationConfig::default()
}
fn default_threshold() -> u32 { 10 }
fn default_decay() -> f64 { 0.1 }
fn default_model() -> String { "claude-haiku-4-5".to_string() }

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            auto_micro_threshold: default_threshold(),
            decay_threshold: default_decay(),
            model: default_model(),
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

pub fn load_config(cortex_dir: &Path) -> Result<Config> {
    let config_path = cortex_dir.join("config.toml");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        Ok(toml::from_str(&content)?)
    } else {
        Ok(Config::default())
    }
}
