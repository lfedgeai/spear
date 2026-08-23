# SMS Runtime API Cleanup

## Scope

This cleanup targets instance/execution presentation logic in SMS Web Admin:

- `src/sms/web_admin.rs`
- `src/sms/runtime_api.rs`
- `src/sms/web_admin/presenter.rs`

## Architectural Change

Before this cleanup, the instance/execution admin endpoints in `web_admin.rs` mixed:

- RPC orchestration
- paging flow
- error handling
- ad-hoc JSON envelope assembly
- row/detail projection logic

That made the file harder to scan and kept runtime response shapes implicit inside handler bodies.

This cleanup introduces `src/sms/runtime_api.rs` as a shared runtime mapping layer for:

- task instance rows
- instance detail payloads
- instance execution summary rows
- execution history rows
- execution detail payloads
- execution / instance mutation responses

After the change:

- `web_admin.rs` focuses more on control flow and RPC sequencing
- `runtime_api.rs` owns runtime-facing admin response models
- legacy runtime JSON presenters are removed from `presenter.rs`

## Benefits

- Makes runtime admin endpoints shorter and easier to read
- Gives instance/execution responses a typed internal home
- Reduces hidden response-shape drift caused by local `json!` construction
- Aligns runtime endpoints with the same modular pattern already used by `task_api` and `node_api`

## Verification

- `cargo test runtime_api --lib`
- `cargo test handlers_test --lib`
- Targeted diagnostics for:
  - `src/sms/runtime_api.rs`
  - `src/sms/web_admin.rs`
  - `src/sms/web_admin/presenter.rs`
