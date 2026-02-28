---
name: aws-bedrock-integration-guide
description: Learned patterns for aws-bedrock-integration-guide
---

---
title: AWS Bedrock Integration at HashiCorp
description: Authentication, configuration, and URI encoding requirements for Bedrock LLM access
tags: [aws, bedrock, anthropic, authentication, sigv4]
---

# AWS Bedrock Integration at HashiCorp

## Authentication: Doormat Layer

HashiCorp's Bedrock access routes through **doormat**, an authentication broker:

1. Doormat intercepts AWS SDK calls
2. Performs credential exchange and session management
3. Injects temporary credentials with cross-account access
4. Credential resolution order:
   - Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`)
   - Fallback to `~/.aws/credentials` file if env vars not set

```rust
// Correct credential resolution pattern
let credentials = if let Ok(key) = env::var("AWS_ACCESS_KEY_ID") {
    // Use env vars (doormat injected)
} else {
    // Fall back to ~/.aws/credentials
}
```

## Cross-Region Inference Profiles

Use fully qualified inference profile IDs instead of bare model IDs:

```rust
// ✅ Correct
let model = "us.anthropic.claude-3-sonnet-20240229-v1:0";

// ❌ Incorrect
let model = "claude-3-sonnet-20240229";
```

Profile format: `{region}.{provider}.{model-id}`

## SigV4 URI Encoding: Special Character Handling

**Critical**: AWS SigV4 canonical URI construction requires percent-encoding of special characters in model IDs.

### Problem
Colons (`:`) in model IDs are not automatically percent-encoded by standard URL libraries, causing signature mismatches.

### Solution
Manually percent-encode colons in model IDs before constructing canonical URI:

```rust
let model = "us.anthropic.claude-3-sonnet-20240229-v1:0";
let encoded = model.replace(":", "%3A");
// Result: "us.anthropic.claude-3-sonnet-20240229-v1%3A0"
```

### Affected Components
- SigV4 request signing
- URI path construction
- Request headers

### Verification
Test with `aws bedrock list-models` to validate canonical URI matches expected format.

## Credentials Resolution Pattern

```rust
use aws_config::meta::region::RegionProviderChain;
use aws_types::region::Region;

// Environment-first resolution (doormat compatible)
let region = RegionProviderChain::first_try(env::var("AWS_REGION"))
    .or_default_provider()
    .region()
    .await;

let config = aws_config::from_env()
    .region(region)
    .load()
    .await;
```

## Troubleshooting

| Symptom | Cause | Solution |
|---------|-------|----------|
| 403 Forbidden | Doormat credentials expired | Refresh via doormat CLI |
| SignatureDoesNotMatch | Model ID not percent-encoded | Verify `:` → `%3A` replacement |
| Invalid cross-region access | Bare model ID used | Use full inference profile ID |
| Credential not found | Env vars not set | Run through doormat proxy or set AWS_* env vars |

