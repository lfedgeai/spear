# SMS AI Admin API Cleanup

## Scope

This cleanup targets AI backend related admin endpoints in SMS Web Admin:

- `src/sms/web_admin.rs`
- `src/sms/ai_admin_api.rs`

## Architectural Change

Before this cleanup, AI backend handlers in `web_admin.rs` still mixed:

- RPC orchestration
- response-shape definition
- local JSON row projection
- nested read-model rendering

That made the AI backend area one of the largest remaining “old style” regions inside Web Admin.

This cleanup introduces `src/sms/ai_admin_api.rs` as a shared typed response layer for:

- backend list/detail/mutation responses
- placement list/mutation responses
- assignment list responses
- node status list responses
- model view list responses

After the change:

- `web_admin.rs` keeps request validation and control flow
- `ai_admin_api.rs` owns AI backend admin response models and mapping rules
- local AI backend JSON projection helpers are removed from `web_admin.rs`

## Benefits

- Makes AI backend admin handlers shorter and easier to scan
- Gives nested AI backend response shapes a stable internal home
- Removes one more large pocket of ad-hoc `json!` assembly from `web_admin.rs`
- Extends the same modular admin-response pattern across a broader part of SMS

## Verification

- `cargo test ai_admin_api --lib`
- `cargo test handlers_test --lib`
- Targeted diagnostics for:
  - `src/sms/ai_admin_api.rs`
  - `src/sms/web_admin.rs`
