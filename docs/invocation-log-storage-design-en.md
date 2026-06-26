# Invocation Log Persistence & Viewing Design

## Background

Today, a user-triggered run in Web Admin (`POST /admin/api/executions`) returns `invocation_id`/`execution_id`, but there is no first-class, user-facing way to persist and view per-invocation logs (stdout/stderr and runtime/system events). This makes troubleshooting and auditing hard.

This document proposes an industry-standard design to persist invocation logs and expose them through the SMS Web Admin BFF + Web Admin UI.

Related docs:

- Execution model: [invocation-execution-model-refactor-en.md](./invocation-execution-model-refactor-en.md)
- Web Admin overview: [web-admin-overview-en.md](./web-admin-overview-en.md)

## Goals

- Persist logs for each invocation (across retries/spillback executions).
- Let users quickly view logs from Web Admin: list → detail → follow → download.
- Support filtering (stream, level) and pagination/seek with stable cursors.
- Enforce safe defaults: retention, size limits, backpressure, redaction.
- Keep Spearlet service logs (control-plane) separate from user invocation logs.

## Non-goals

- Replacing enterprise log platforms (Loki/Elastic/Splunk). We keep an integration path.
- Providing arbitrary full-text search over years of logs in-app (can be phased in).
- Implementing an interactive console (covered separately).

## Definitions

- **Invocation**: a user/client request to run a function; stable `invocation_id`.
- **Execution**: a concrete attempt of an invocation on a node; `execution_id`.
- **Invocation log**: user-facing log stream attached to a specific execution, including:
  - process stdout/stderr
  - runtime/system events (optional)
  - structured app logs emitted via runtime APIs (optional)

## Requirements

### Functional

- Persist logs per **execution**; aggregate at **invocation** level.
- Provide **follow** (near real-time) and **history** (pagination) reads.
- Provide **download** of a whole execution log as a file.
- Provide consistent correlation keys in every log line:
  - `invocation_id`, `execution_id`, `task_id`, `function_name`, `node_uuid`, `instance_id`.

### Non-functional

- **Retention**: default TTL, configurable (e.g., 7–30 days).
- **Cost & size control**: per-execution max bytes, line truncation, chunking + compression.
- **Backpressure**: do not OOM Spearlet on noisy workloads; allow dropping policy.
- **Security**: authz, tenant isolation (future), secret redaction, safe downloads.
- **Reliability**: tolerate SMS outage; best-effort persistence with clear UX state.

## High-level Architecture

### Components

1. **Spearlet Log Collector (per execution)**
   - Captures stdout/stderr and optional runtime events.
   - Buffers in memory with bounded queues.
   - Flushes to central storage in chunks.

2. **SMS Log Storage (central)**
   - **Blob store** for log chunks (append-only): object storage in production; the existing `smsfile://` store can be a dev/default backend.
   - **Metadata store** for indices/cursors/retention: KV store (RocksDB/Sled) already exists in the project.

3. **SMS Web Admin BFF**
   - Lists invocations/executions and serves log history.
   - Provides polling-friendly history APIs (short term: no SSE).

4. **Web Admin UI**
   - Executions list and log viewer.
   - “View logs” deep link from task execution actions.

### Data flow

1. User triggers execution in Web Admin → SMS BFF spills back to a Spearlet node.
2. Spearlet starts execution and emits log events into the log collector.
3. Collector flushes chunk files + metadata updates to SMS Log Storage.
4. UI reads history via SMS BFF; follow mode uses short polling for incremental reads (later: SSE/WebSocket).

## Async Execution & Log Lifecycle (Key)

### Problem

For `execution_mode=async`, the semantics are: acknowledge submission and return `running` immediately, while the actual work continues in the background. If we flush/finalize logs at submission time (as if sync), we get:

- flush reads empty (logs not produced yet);
- finalize closes the stream too early, so later logs can no longer be appended.

### Recommended best practice

- Treat execution logs as an append-only stream keyed by `execution_id`, with a server-enforced lifecycle:
  - `open` → `finalizing` → `finalized` (finalized rejects writes; finalize is idempotent)
- Only when an execution reaches a terminal status (completed/failed/timeout/cancelled) do we:
  - write `execution_completed/failed` system logs,
  - flush remaining runtime logs,
  - finalize the stream.
- Async runtimes must emit a completion signal back to the orchestration layer (TaskExecutionManager), which triggers the final flush/finalize.
- Follow mode should prefer cursor-based incremental polling first; SSE/WebSocket is an optional optimization and should not change storage semantics.

## Storage Design

### Log event schema (logical)

Each line is a structured record; store as NDJSON.

```json
{
  "ts_ms": 1730000000000,
  "seq": 42,
  "invocation_id": "...",
  "execution_id": "...",
  "task_id": "...",
  "function_name": "__default__",
  "node_uuid": "...",
  "instance_id": "...",
  "stream": "stdout",
  "level": "info",
  "message": "hello",
  "attrs": {"k": "v"}
}
```

Notes:

- `seq` is a monotonic sequence per execution, used for stable pagination.
- `stream` is one of `stdout|stderr|system`.
- `attrs` is optional and can carry runtime-specific fields.

### Chunk format

- Chunk file: `invocations/{invocation_id}/executions/{execution_id}/chunks/{chunk_seq}.ndjson.zst`
- Chunk size: target 1–4 MiB compressed (configurable).
- Compression: Zstd (or gzip as a fallback).

### Metadata (KV)

Key patterns (logical):

- `exec:{execution_id}:meta` → { task_id, invocation_id, node_uuid, instance_id, started_at, completed_at, status, byte_count, first_seq, last_seq }
- `exec:{execution_id}:chunks` → list of { chunk_seq, first_seq, last_seq, uri, byte_count, created_at }
- `inv:{invocation_id}:executions` → ordered execution_ids (attempts)

This keeps listing fast without scanning blob storage.

### Retention & cleanup

- TTL policy runs in SMS:
  - delete chunk blobs and KV metadata when `completed_at + ttl < now`.
  - enforce per-execution `max_bytes` by truncating and marking `truncated=true`.

## API Design (SMS Web Admin BFF)

This design keeps UI-facing APIs in SMS to avoid the browser calling Spearlet directly.

### 1) Execution create response

Update `POST /admin/api/executions` response to include `invocation_id` and a log link:

```json
{
  "success": true,
  "invocation_id": "...",
  "execution_id": "...",
  "node_uuid": "...",
  "message": "ok",
  "log_url": "/admin/executions/{execution_id}"
}
```

### 2) List executions

`GET /admin/api/executions?task_id=&invocation_id=&limit=&cursor=`

Returns summaries with `execution_id`, `invocation_id`, `status`, timestamps, and `node_uuid`.

### 3) Get execution

`GET /admin/api/executions/{execution_id}`

Returns the execution meta and a `log_state`:

- `ready`: history available
- `pending`: execution running but not yet flushed
- `unavailable`: storage failure

### 4) Read logs (history)

`GET /admin/api/executions/{execution_id}/logs?limit=500&cursor=...&stream=stdout,stderr&level=info,warn,error`

Response:

```json
{
  "execution_id": "...",
  "lines": [ {"ts_ms":..., "seq":..., "stream":"stdout", "message":"..."} ],
  "next_cursor": "...",
  "truncated": false,
  "completed": false
}
```

Cursor recommendation:

- Use an opaque cursor encoded from `(seq, chunk_seq, offset)`.
- Support `direction=backward|forward` for “scroll up” behavior.

### 5) Download

`GET /admin/api/executions/{execution_id}/logs/download`

- Content-Type: `text/plain` or `application/x-ndjson`
- Optional `?format=text|ndjson`

## Spearlet Design

### Capture sources

- **Process runtime**: pipe child stdout/stderr, attach to execution.
- **WASM runtime**: support a hostcall “log” API that maps to invocation logs.
- **System events**: optionally log lifecycle milestones (scheduled/started/completed, resource usage).

### Backpressure strategy

- Per-execution bounded ring buffer (e.g., by bytes and/or line count).
- Flush trigger:
  - size threshold reached
  - time threshold (e.g., every 1s)
  - execution completion

### Async (no_wait) strategy

- For `execution_mode=async|console|stream`, Spearlet returns `execution_status=running`. The orchestration layer must not finalize logs at this point.
- When the runtime actually finishes, it must emit a completion signal to the orchestration layer, which performs:
  - flush (including WASM hostcall logs),
  - `execution_completed/failed` system log,
  - finalize.
- To avoid log mixing under instance reuse, WASM hostcall logs should be attributed to `execution_id` (or carry `execution_id` and be filtered at flush time).

Drop policy (configurable):

- `drop_oldest` (default): preserve latest context.
- `drop_newest`: preserve earliest context.

### Failure handling

- If SMS log storage is unreachable:
  - keep a small local buffer;
  - mark execution `log_state=unavailable` in metadata;
  - UI shows “logs unavailable, retry later”.

## Web Admin UI Changes (frontend)

Target codebase: `web-admin/` (React + TanStack Query). The UI already calls `POST /admin/api/executions` via [executions.ts](../web-admin/src/api/executions.ts).

### New pages & navigation

- Add “Executions” page:
  - route: `/executions`
  - list table with filters (task_id, status, time range, node)
  - click row → `/executions/:execution_id`

### Execution detail page

- Header: status, task_id, node_uuid, invocation_id, timestamps.
- Log viewer:
  - “Follow” toggle (auto-scroll)
  - stream filters (stdout/stderr/system)
  - level filters (if supported)
  - search-in-view (client-side)
  - download button
  - copy selected lines

### API client additions

- Add `src/api/logs.ts`:
  - `getExecution(execution_id)`
  - `getExecutionLogs(execution_id, cursor, limit, filters)`
  - follow mode via short polling (short term: no EventSource)

### UX details (best practices)

- Virtualized rendering for large logs.
- Show “truncated” banner if max bytes reached.
- Clear separation of stdout vs stderr (color/label).
- Persist “Follow” and filters in query params for shareable links.

## Security Considerations

- Never expose secret values in UI.
  - Redact common patterns (API keys, Bearer tokens) in SMS before storage.
  - Do not store environment variables by default.
- Auth:
  - Reuse `SMS_WEB_ADMIN_TOKEN` bearer mechanism for log APIs.
  - Future: add per-tenant RBAC and audit.
- Downloads:
  - Content-Disposition attachment; limit size; protect against path traversal by using IDs only.

## Implementation Plan (Function-level)

Phase the rollout to keep semantics correct first, then improve UX.

### Phase 1: Fix async log lifecycle (no premature finalize)

- [TaskExecutionManager::execute_existing_task_invocation](../src/spearlet/execution/manager.rs#L658-L882)
  - If `runtime_response.execution_status == Running`:
    - do not call `append_wasm_logs_to_sms`
    - do not write `execution_completed`
    - do not call `finalize_execution_logs_to_sms`
    - optionally write a `system` log like `execution_dispatched mode=async`

### Phase 2: Add completion signal for async runtimes

- [WasmWorkerRequest](../src/spearlet/execution/runtime/wasm.rs)
  - Extend `Invoke` to carry `execution_id` and a completion sender (tokio mpsc/oneshot).
- [WasmRuntime::execute](../src/spearlet/execution/runtime/wasm.rs#L790-L927)
  - In no_wait mode, enqueue `Invoke(execution_id, ...)` and register a completion handler.
- [TaskExecutionManager](../src/spearlet/execution/manager.rs)
  - Add a background listener for completion events:
    - `append_wasm_logs_to_sms(execution_id, ...)`
    - write `execution_completed/failed`
    - `finalize_execution_logs_to_sms(execution_id)`
    - update execution index state + timestamps (Completed/Failed)

### Phase 3: Attribute WASM hostcall logs by execution_id

- [DefaultHostApi::wasm_log_write](../src/spearlet/execution/host_api/core.rs#L151-L213)
  - Introduce a “current execution_id” context (set/clear by worker around each invoke), and attach `execution_id` to each log entry.
- [get_wasm_logs / clear_wasm_logs](../src/spearlet/execution/host_api/core.rs#L99-L126)
  - Add `get_wasm_logs_by_execution(execution_id, cursor, limit)` for flushing.
- [append_wasm_logs_to_sms](../src/spearlet/execution/manager.rs#L952-L1000)
  - Switch from “read by instance_id” to “incremental read by execution_id + cursor”.

### Phase 4: Improve follow UX (optional)

- Periodically flush running executions (e.g., every 500ms–1s); UI follows by polling `/logs?cursor=...`.

## Observability

- Every log line includes correlation IDs.
- Emit metrics:
  - bytes ingested, dropped lines, flush latency, storage failures.
- Add tracing spans around flush operations.

## Rollout Plan

1. **Phase 0 (MVP)**
   - Persist stdout/stderr logs per execution.
   - Implement history read + download.
   - UI: execution detail page with log viewer.

2. **Phase 1**
   - SSE tail streaming.
   - Execution list page.

3. **Phase 2**
   - Structured log levels and richer runtime events.
   - Optional full-text search (behind config).

## Alternatives

- **External log platform integration**: ship logs via OTEL/Fluent Bit and query in Grafana/Kibana.
  - Pros: scalable search, long retention.
  - Cons: not self-contained, requires infra.

This proposal keeps an in-product MVP while leaving a clean export path.
