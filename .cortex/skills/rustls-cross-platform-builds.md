---
name: rustls-cross-platform-builds
description: Learned patterns for rustls-cross-platform-builds
---

# Rustls for Cross-Platform TLS

## Pattern
Use rustls over native TLS implementations (OpenSSL, SecureTransport, etc.) for CLI tools requiring cross-platform builds.

## Rationale
- Pure Rust implementation eliminates native dependency compilation complexity
- Consistent behavior across Linux, macOS, and Windows
- Reduces build surface area and dependency management overhead
- Particularly valuable for Rust and Go CLI tools

## Implementation
When building CLI tools in Rust or Go, prefer rustls-based TLS clients over system-native TLS bindings.

## Related Preferences
- Preferred languages: Rust, Go for CLI tools
- Employment context: HashiCorp (infrastructure tooling focus)
