# SMS Task API Mapper Cleanup

## Scope

This cleanup targets the task-facing HTTP and admin entrypoints in the SMS module:

- `src/sms/handlers/task.rs`
- `src/sms/web_admin/task_admin.rs`
- `src/sms/task_api.rs`

The goal is to make the architecture easier to read by separating transport handling from task request construction and task presentation mapping.

## Architectural Change

Before this cleanup, both task entrypoints were doing the following locally:

- Converting public executable type strings into proto enums
- Converting scheduling strategy strings into proto enums
- Building `RegisterTaskRequest`
- Mapping proto `Task` records into public/admin response payloads

That duplicated logic across two files and made both endpoints own protocol translation details.

This cleanup introduces a shared mapper module, `src/sms/task_api.rs`, which now owns:

- Task registration request construction
- Executable type normalization
- Scheduling strategy normalization
- Public task response projection
- Admin task summary/detail/deletion response projection

After the change:

- `handlers/task.rs` focuses on HTTP request parsing, error handling, and gRPC invocation
- `web_admin/task_admin.rs` focuses on admin query flow and pagination/filter orchestration
- `task_api.rs` is the single place for task API translation rules

## Benefits

- Reduces repeated task mapping logic across HTTP and admin surfaces
- Makes task-facing controller files shorter and easier to scan
- Clarifies where protocol translation belongs in the SMS architecture
- Creates a reusable pattern for future cleanup of other duplicated API adapters

## Verification

- `cargo test handlers_test task_api --lib`
- Targeted diagnostics for:
  - `src/sms/handlers/task.rs`
  - `src/sms/web_admin/task_admin.rs`
  - `src/sms/task_api.rs`
