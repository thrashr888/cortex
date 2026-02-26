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

#[derive(Serialize)]
struct BedrockRequest {
    anthropic_version: String,
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
    // Check if we have a direct API key (non-empty)
    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();

    if !api_key.is_empty() {
        call_direct_api(prompt, system, config, &api_key).await
    } else if resolve_aws_credentials().is_some() {
        call_bedrock(prompt, system, config).await
    } else {
        anyhow::bail!(
            "No LLM credentials found. Set ANTHROPIC_API_KEY for direct API, \
             or AWS credentials (env vars or ~/.aws/credentials) for Bedrock. \
             Run `cortex sleep --micro` for LLM-free consolidation."
        )
    }
}

/// AWS credential triple
struct AwsCreds {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
}

/// Resolve AWS credentials from env vars or ~/.aws/credentials file
fn resolve_aws_credentials() -> Option<AwsCreds> {
    // Try env vars first
    if let (Ok(ak), Ok(sk)) = (
        std::env::var("AWS_ACCESS_KEY_ID"),
        std::env::var("AWS_SECRET_ACCESS_KEY"),
    ) {
        if !ak.is_empty() && !sk.is_empty() {
            return Some(AwsCreds {
                access_key: ak,
                secret_key: sk,
                session_token: std::env::var("AWS_SESSION_TOKEN").ok().filter(|s| !s.is_empty()),
            });
        }
    }

    // Try ~/.aws/credentials file
    let home = std::env::var("HOME").ok()?;
    let creds_path = std::path::PathBuf::from(&home).join(".aws").join("credentials");
    let content = std::fs::read_to_string(&creds_path).ok()?;

    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".to_string());
    let section_header = format!("[{}]", profile);

    let mut in_section = false;
    let mut access_key = None;
    let mut secret_key = None;
    let mut session_token = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == section_header;
            continue;
        }
        if !in_section {
            continue;
        }
        // Handle both "key = value" and "export KEY=value" formats
        let (key, value) = if let Some(rest) = trimmed.strip_prefix("export ") {
            if let Some((k, v)) = rest.split_once('=') {
                (k.trim().to_lowercase(), v.trim().to_string())
            } else {
                continue;
            }
        } else if let Some((k, v)) = trimmed.split_once('=') {
            (k.trim().to_lowercase(), v.trim().to_string())
        } else {
            continue;
        };

        match key.as_str() {
            "aws_access_key_id" => access_key = Some(value),
            "aws_secret_access_key" => secret_key = Some(value),
            "aws_session_token" => session_token = Some(value),
            _ => {}
        }
    }

    match (access_key, secret_key) {
        (Some(ak), Some(sk)) if !ak.is_empty() && !sk.is_empty() => Some(AwsCreds {
            access_key: ak,
            secret_key: sk,
            session_token,
        }),
        _ => None,
    }
}

async fn call_direct_api(prompt: &str, system: &str, config: &Config, api_key: &str) -> Result<String> {
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

    let client = reqwest::Client::new();
    let body = MessageRequest {
        model: config.consolidation.model.clone(),
        max_tokens: 8192,
        system: system.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
    };

    let resp = client
        .post(format!("{}/v1/messages", base_url))
        .header("x-api-key", api_key)
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

async fn call_bedrock(prompt: &str, system: &str, config: &Config) -> Result<String> {
    let region = std::env::var("AWS_REGION")
        .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
        .unwrap_or_else(|_| "us-west-2".to_string());

    let creds = resolve_aws_credentials()
        .context("No AWS credentials found in env vars or ~/.aws/credentials")?;
    let access_key = creds.access_key;
    let secret_key = creds.secret_key;
    let session_token = creds.session_token;

    // Map model name to Bedrock model ID
    let model_id = bedrock_model_id(&config.consolidation.model);

    let body = BedrockRequest {
        anthropic_version: "bedrock-2023-05-31".to_string(),
        max_tokens: 8192,
        system: system.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
    };

    let body_bytes = serde_json::to_vec(&body)?;
    let host = format!("bedrock-runtime.{}.amazonaws.com", region);
    // URL uses the raw model ID — reqwest handles encoding in the HTTP request
    let url = format!("https://{}/model/{}/invoke", host, model_id);

    // AWS SigV4 signing
    let now = chrono::Utc::now();
    let date_stamp = now.format("%Y%m%d").to_string();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

    // Canonical URI must use percent-encoded path segments per SigV4 spec
    let encoded_model_id = uri_encode(&model_id);
    let canonical_uri = format!("/model/{}/invoke", encoded_model_id);
    let canonical_querystring = "";

    let payload_hash = sha256_hex(&body_bytes);

    let mut canonical_headers = format!(
        "content-type:application/json\nhost:{}\nx-amz-date:{}\n",
        host, amz_date
    );
    let mut signed_headers = "content-type;host;x-amz-date".to_string();

    if let Some(ref token) = session_token {
        canonical_headers = format!(
            "content-type:application/json\nhost:{}\nx-amz-date:{}\nx-amz-security-token:{}\n",
            host, amz_date, token
        );
        signed_headers = "content-type;host;x-amz-date;x-amz-security-token".to_string();
    }

    let canonical_request = format!(
        "POST\n{}\n{}\n{}\n{}\n{}",
        canonical_uri, canonical_querystring, canonical_headers, signed_headers, payload_hash
    );

    let credential_scope = format!("{}/{}/bedrock/aws4_request", date_stamp, region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date, credential_scope, sha256_hex(canonical_request.as_bytes())
    );

    let signing_key = get_signature_key(&secret_key, &date_stamp, &region, "bedrock");
    let signature = hmac_sha256_hex(&signing_key, string_to_sign.as_bytes());

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        access_key, credential_scope, signed_headers, signature
    );

    let client = reqwest::Client::new();
    let mut req = client
        .post(&url)
        .header("content-type", "application/json")
        .header("x-amz-date", &amz_date)
        .header("authorization", &authorization);

    if let Some(ref token) = session_token {
        req = req.header("x-amz-security-token", token);
    }

    let resp = req
        .body(body_bytes)
        .send()
        .await
        .context("Failed to call Bedrock")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Bedrock API error ({}): {}", status, text);
    }

    let response: MessageResponse = resp.json().await.context("Failed to parse Bedrock response")?;
    response
        .content
        .into_iter()
        .find_map(|b| b.text)
        .context("No text in Bedrock response")
}

fn bedrock_model_id(model: &str) -> String {
    // If it already looks like a full Bedrock inference profile ID, use as-is
    if model.starts_with("us.anthropic.") || model.starts_with("eu.anthropic.") {
        return model.to_string();
    }
    // If it's a direct model ID (anthropic.*), convert to cross-region inference profile
    if model.starts_with("anthropic.") {
        return format!("us.{}", model);
    }
    // Map common short names to cross-region inference profile IDs
    match model {
        "claude-haiku-4-5" | "claude-haiku-4-5-20241022" | "claude-haiku-4-5-20251001" => {
            "us.anthropic.claude-haiku-4-5-20251001-v1:0".to_string()
        }
        "claude-sonnet-4-5" | "claude-sonnet-4-5-20250929" => {
            "us.anthropic.claude-sonnet-4-5-20250929-v1:0".to_string()
        }
        "claude-sonnet-4" | "claude-sonnet-4-20250514" => {
            "us.anthropic.claude-sonnet-4-20250514-v1:0".to_string()
        }
        "claude-3-5-haiku" | "claude-3-5-haiku-20241022" => {
            "us.anthropic.claude-3-5-haiku-20241022-v1:0".to_string()
        }
        "claude-3-5-sonnet" | "claude-3-5-sonnet-20241022" => {
            "us.anthropic.claude-3-5-sonnet-20241022-v2:0".to_string()
        }
        _ => format!("us.anthropic.{}-v1:0", model),
    }
}

// --- AWS SigV4 helpers ---

/// URI-encode a path segment per AWS SigV4 rules (encode everything except unreserved chars)
fn uri_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'~' | b'.' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let digest = sha256(data);
    let mut s = String::with_capacity(64);
    for byte in &digest {
        write!(s, "{:02x}", byte).unwrap();
    }
    s
}

fn sha256(data: &[u8]) -> [u8; 32] {
    // Minimal SHA-256 implementation using ring-like approach
    // We'll use the hmac helper with a simple hash
    // Actually, let's use a basic implementation
    sha256_impl(data)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let block_size = 64;
    let mut ikey = vec![0x36u8; block_size];
    let mut okey = vec![0x5cu8; block_size];

    let key = if key.len() > block_size {
        sha256(key).to_vec()
    } else {
        key.to_vec()
    };

    for (i, &b) in key.iter().enumerate() {
        ikey[i] ^= b;
        okey[i] ^= b;
    }

    let mut inner = ikey;
    inner.extend_from_slice(data);
    let inner_hash = sha256(&inner);

    let mut outer = okey;
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

fn hmac_sha256_hex(key: &[u8], data: &[u8]) -> String {
    let hash = hmac_sha256(key, data);
    let mut s = String::with_capacity(64);
    for byte in &hash {
        use std::fmt::Write;
        write!(s, "{:02x}", byte).unwrap();
    }
    s
}

fn get_signature_key(key: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", key).as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request").to_vec()
}

// Minimal SHA-256 implementation (no external dependency)
fn sha256_impl(data: &[u8]) -> [u8; 32] {
    let h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut hash = h;

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g; g = f; f = e;
            e = d.wrapping_add(temp1);
            d = c; c = b; b = a;
            a = temp1.wrapping_add(temp2);
        }

        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, &val) in hash.iter().enumerate() {
        result[i*4..i*4+4].copy_from_slice(&val.to_be_bytes());
    }
    result
}
