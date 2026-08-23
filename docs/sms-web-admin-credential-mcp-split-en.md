# SMS Web Admin Credential And MCP Split

## Scope

This cleanup performs a file-level modularization step for:

- `src/sms/web_admin.rs`
- `src/sms/web_admin/credential_mcp_admin.rs`

## Architectural Change

After the previous AI backend split, `web_admin.rs` still directly contained another self-contained feature area:

- credential request bodies and handlers
- MCP request bodies and handlers
- MCP proto-construction helpers

This cleanup moves that feature area into:

- `web_admin/credential_mcp_admin.rs`

The new module now owns:

- credential request bodies
- MCP request bodies
- credential handlers
- MCP handlers
- local MCP request-to-proto construction logic

`web_admin.rs` now re-exports the module items needed by the router instead of holding the full implementation directly.

## Benefits

- Further reduces the size and responsibility surface of `web_admin.rs`
- Makes credential/MCP admin logic easier to find and evolve independently
- Keeps request types, handlers, and conversion helpers grouped by feature
- Strengthens the codebase’s shift from “one big admin file” to “main entry + feature modules”

## Verification

- `cargo test web_admin --lib`
- `cargo test handlers_test --lib`
- Targeted diagnostics for:
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/credential_mcp_admin.rs`
