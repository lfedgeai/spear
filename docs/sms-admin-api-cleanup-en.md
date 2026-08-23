# SMS Admin API Cleanup

## Scope

This cleanup targets credential and MCP admin endpoints in SMS Web Admin:

- `src/sms/web_admin.rs`
- `src/sms/admin_api.rs`
- `src/sms/web_admin/presenter.rs`

## Architectural Change

Before this cleanup, credential and MCP handlers in `web_admin.rs` still used local `json!` envelopes and presenter helpers. That kept response shapes implicit inside handler bodies and left admin endpoints inconsistent with the newer `task_api`, `node_api`, and `runtime_api` structure.

This cleanup introduces `src/sms/admin_api.rs` as a shared admin response layer for:

- credential list rows
- credential mutation responses
- MCP server list/detail responses
- revision-only mutation responses

After the change:

- `web_admin.rs` uses typed admin responses for credential and MCP endpoints
- `admin_api.rs` owns the credential/MCP response models and projection helpers
- legacy credential/MCP presenter helpers are removed from `presenter.rs`

## Benefits

- Makes credential/MCP admin endpoints easier to read
- Reduces local `json!` assembly inside `web_admin.rs`
- Gives admin response shapes a stable internal home
- Extends the same modular API-mapper pattern across more of SMS Web Admin

## Verification

- `cargo test admin_api --lib`
- `cargo test handlers_test --lib`
- Targeted diagnostics for:
  - `src/sms/admin_api.rs`
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/presenter.rs`
