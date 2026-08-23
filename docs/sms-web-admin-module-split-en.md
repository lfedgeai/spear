# SMS Web Admin Module Split

## Scope

This cleanup performs a file-level modularization step for:

- `src/sms/web_admin.rs`
- `src/sms/web_admin/ai_backend_admin.rs`

## Architectural Change

Previous cleanup rounds already extracted shared response mappers, but `web_admin.rs` still directly contained the full AI backend admin implementation:

- request body structs
- query structs
- parse/build helpers
- handler functions

That meant the main Web Admin file still carried a large self-contained feature area even after response mapping logic had been cleaned up.

This cleanup moves the AI backend admin feature into a dedicated module:

- `web_admin/ai_backend_admin.rs` now owns AI backend request bodies, input parsing helpers, proto construction helpers, and handlers
- `web_admin.rs` re-exports the module items needed by the router
- the main file keeps acting as an integration/composition surface instead of a large implementation bucket

## Benefits

- Reduces the size and cognitive load of `web_admin.rs`
- Makes AI backend admin logic easier to find and evolve independently
- Improves module boundaries by grouping request types, handlers, and input normalization in one place
- Moves the codebase from “mapper extraction only” toward real file-level modularization

## Verification

- `cargo test web_admin --lib`
- `cargo test handlers_test --lib`
- Targeted diagnostics for:
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/ai_backend_admin.rs`
