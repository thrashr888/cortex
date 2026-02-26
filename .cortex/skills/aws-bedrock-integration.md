---
name: aws-bedrock-integration
description: Learned patterns for aws-bedrock-integration
---

# AWS Bedrock Integration

## Pattern
Properly configure AWS Bedrock API calls with correct credential handling and request signing.

## Key Points

### Model IDs
- Use cross-region inference profile IDs: `us.anthropic.*` instead of bare model IDs
- Example: `us.anthropic.claude-3-5-sonnet-20241022-v2:0` for cross-region support

### Request Signing (SigV4)
- Percent-encode special characters in model IDs for canonical URI
- Colons (:) must be encoded as %3A
- This affects Authorization header computation

### Credentials
- Read from environment variables first (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
- Fallback to ~/.aws/credentials file if env vars not set
- Respect AWS_REGION or credential file region

## References
- Bugfix IDs: 10, 12
- Pattern ID: 13

