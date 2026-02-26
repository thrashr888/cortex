---
name: mcp-server-implementation
description: Learned patterns for mcp-server-implementation
---

# MCP Server Implementation

## Pattern
Implement MCP servers using plain JSON-RPC over stdio instead of the rmcp crate.

## Why
- rmcp v0.1.5 has incompatible macro API
- Plain JSON-RPC is simpler and more reliable
- More portable across different environments

## Implementation
- Use stdio for communication (stdin/stdout)
- Implement JSON-RPC 2.0 protocol directly
- Handle method dispatch in application code
- Avoid macro-based approaches that create API instability

## References
- Decision IDs: 2, 4, 14

