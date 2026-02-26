use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessageResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

pub async fn call_anthropic(prompt: &str, system: &str, config: &Config) -> Result<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY not set. Run `cortex sleep --micro` for LLM-free consolidation.")?;

    let client = reqwest::Client::new();
    let body = MessageRequest {
        model: config.consolidation.model.clone(),
        max_tokens: 4096,
        system: system.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
    };

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Failed to call Anthropic API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic API error ({}): {}", status, text);
    }

    let response: MessageResponse = resp.json().await.context("Failed to parse Anthropic response")?;
    response
        .content
        .into_iter()
        .find_map(|b| b.text)
        .context("No text in Anthropic response")
}
