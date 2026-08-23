# SMS Endpoint Gateway Design (WS → WS → Instance)

## 1. Background & Goals

We want SMS (Metadata Server) to expose a single WebSocket route that hosts all registered task `endpoint`s. The endpoint format is restricted to alphanumeric, underscore, and hyphen only (`[A-Za-z0-9_-]+`). When a client connects to an endpoint:

- SMS resolves `endpoint -> task`
- SMS finds a usable execution (whose instance can serve this task) and connects to that execution’s WS data plane
- If no usable execution exists, SMS starts one and connects after it becomes available
- SMS proxies frames bidirectionally between client WS and upstream execution WS
- Client and task exchange multiplexed request/response over SSF (binary frames)

Goals:

- An endpoint behaves like a long-lived function entry: connect via `GET /e/{endpoint}/ws` and then exchange frames
- Reuse existing user-stream + SSF + Spearlet WebSocket data plane; no new low-level protocol
- Provide observability, timeouts, clear error attribution, and resource protection (rate limiting/backpressure/circuit breaking)

Non-goals (for this phase):

- Multi-tenant isolation and complex quota systems (only leave extension points)
- Fully dynamic OpenAPI generation (optional later)
- Streaming HTTP responses (start with request/response; extend later)
- Connection-level session resume / stream continuation (no resume in v1; can be added later)

## 2. Current State & Dependencies

### 2.1 Current State

- Task registration already stores a string field `endpoint` in SMS, but it is currently not used by the execution pipeline.
- SMS already provides an execution stream session + WS proxy mechanism:
  - Control plane: `POST /api/v1/executions/{execution_id}/streams/session` generates a short-lived token (TTL=60s)
  - Data plane: `GET /api/v1/executions/{execution_id}/streams/ws?token=...` proxies to Spearlet
- Spearlet exposes `GET /api/v1/executions/{execution_id}/streams/ws` which writes incoming WS frames into an execution-scoped user-stream hub and flushes outbound frames back to WS.

### 2.2 Key Constraints

- The user-stream hub is keyed by `execution_id`. Therefore, an endpoint call must bind to an `execution_id` to reuse the existing data plane.
- Host(Linux) errno values may differ from WASI errno values. SDK/protocol logic must use Spear ABI constants, not `libc::*`, for cross-target stability.

## 3. API Design

### 3.1 Endpoint Route

Add:

- `GET /e/{endpoint}/ws` (WebSocket Upgrade)

Rules:

- `endpoint` must match: `^[A-Za-z0-9_-]+$`
- Recommended max length: 64 (configurable)
- Recommended normalization: store and match in lower-case to avoid case ambiguity, and treat endpoint as case-insensitive
- Recommend using `Sec-WebSocket-Protocol` to negotiate the application subprotocol version (e.g. `ssf.v1`) for forward compatibility

### 3.2 Registration Rules

On task registration, validate and index `endpoint`:

- endpoint is non-empty
- endpoint matches the regex
- endpoint is unique (global for now; can be namespaced later)

### 3.3 Error Codes

During WS handshake (HTTP status codes):

- 400 `INVALID_ENDPOINT`: invalid endpoint format
- 404 `ENDPOINT_NOT_FOUND`: endpoint not registered
- 409 `ENDPOINT_CONFLICT`: duplicate registration (at register time)
- 401 `UNAUTHENTICATED`: missing/invalid credentials
- 403 `FORBIDDEN`: not allowed to access the endpoint (policy hook)
- 429 `RATE_LIMITED`: connection/user/endpoint rate limiting triggered
- 413 `FRAME_TOO_LARGE`: frame size exceeds limits (only static checks possible at handshake time)
- 503 `NO_EXECUTION_AVAILABLE`: no usable execution and cannot start one
- 504 `UPSTREAM_TIMEOUT`: startup/connect timeout
- 502 `UPSTREAM_WS_FAILED`: upstream WS connect/proxy failure

After WS is established (WebSocket Close or in-protocol error frames):

- 4400 `INVALID_PROTOCOL`: client frame does not follow SSF/conventions
- 4500 `TASK_ERROR`: task returned an error frame
- 4501 `UPSTREAM_OVERLOADED`: upstream overload/backpressure (SMS may translate and optionally reroute)
- 4502 `STREAM_NOT_FOUND`: unknown stream_id (missing mapping or already reclaimed)

Recommendations:

- Prefer standard WS close codes for connection-level failures (e.g. 1008/1011) with a short reason; keep stable business error codes inside SSF error frames to avoid client-specific close-code handling differences
- For stream-level failures, prefer terminating the stream with an SSF error frame instead of closing the whole WS (except for protocol violations/resource protection)

## 4. Execution Model

### 4.1 endpoint → task

To avoid scanning all tasks, SMS should maintain an index:

- `endpoint -> task_id`

Update the index on `RegisterTask/UnregisterTask`.

### 4.2 task → instance / execution

Minimal selection strategy:

1. `ListTaskInstances(task_id)` → filter active & fresh
2. Pick a usable execution for a candidate instance (e.g. most recent running execution)
3. If none available: trigger placement/invocation to start a new execution (Spearlet starts/reuses an instance as needed)

Notes:

- Tasks no longer bind to a single `node_uuid`; placement uses the task's `desired_replicas`, `scheduling_strategy`, and current active-instance distribution
- The MVP now supports `SPREAD`: prefer nodes that currently host fewer instances for the same task
- Later we can add load/concurrency/affinity/LRU scoring

### 4.3 execution binding strategy

For this phase, prefer “reuse execution per endpoint”:

- A task can serve multiple logical requests on a long-running execution by multiplexing over SSF `stream_id`
- When an endpoint WS is established, SMS prefers reusing an existing usable execution; if none exists, SMS starts one and connects
- Execution lifecycle can be idle-timeout based reclamation, or configured as always-on

### 4.4 Multiplexing across multiple instances/executions

When a single task has multiple instances across nodes (and therefore multiple usable executions), the endpoint WS must multiplex traffic across those executions. Key takeaways:

- Load-balancing should be done per “logical stream” (SSF `stream_id`), not by binding an endpoint to a single execution.
- To allow multiple client connections to share the same execution, SMS must provide `stream_id` namespace isolation (rewrite/mapping). Otherwise different clients will naturally reuse `stream_id=1/2/...` and collide inside an execution.
- The current implementation prefers preserving the original `client_stream_id` (for example the common `1/2` text/voice pair) whenever that id is still free inside the execution. A new `upstream_stream_id` is allocated only on collision, so single-client user-stream workloads keep stable stream semantics while multi-client sharing still remains isolated.

#### 4.4.1 Per-stream routing

For each endpoint WS connection, SMS maintains routing tables:

- `(client_conn_id, client_stream_id) -> upstream_execution_id`
- `(client_conn_id, client_stream_id) -> upstream_stream_id`

On each incoming SSF frame from the client:

1. Parse SSF header to get `client_stream_id`
2. If `(client_conn_id, client_stream_id)` is not bound yet:
   - pick an execution from the candidate pool (see strategies below)
   - allocate an execution-unique `upstream_stream_id`
   - record mappings and increment `active_streams` on that execution
3. Rewrite `stream_id` to `upstream_stream_id`
4. Forward the frame to the chosen execution’s upstream WS

On each SSF frame from upstream WS:

1. Parse `upstream_stream_id`
2. Reverse-map to `(client_conn_id, client_stream_id)`
3. Rewrite `stream_id` back to `client_stream_id`
4. Forward to the corresponding client WS connection

#### 4.4.2 Execution selection (load balancing)

Candidate pool sources:

- Prefer reusing active executions for the task (running and WS-connectable)
- If capacity is insufficient, start new executions and add them to the pool

Strategy evolution:

- Round-robin
- Random
- Least active streams (recommended): pick the execution with smallest `active_streams`
- Metrics-based: combine instance metrics (CPU, concurrency, queue length, etc.)

#### 4.4.3 stream_id rewrite (namespace isolation)

Why rewrite is required:

- The execution-scoped user-stream hub routes by `(execution_id, stream_id)`
- Multiple client connections sharing the same execution will reuse small integers for `stream_id` by default, causing collisions

Therefore SMS must guarantee:

- For a given execution: all `upstream_stream_id` values are globally unique
- On the client side: `stream_id` only needs to be unique within the client connection

Allocation:

- Each execution maintains a monotonic `next_upstream_stream_id: u32`
- Allocate `upstream_stream_id = next++` (skip 0)
- On client disconnect or close/error, reclaim mappings and decrement `active_streams`

#### 4.4.4 Failure and recovery semantics

- Upstream execution WS disconnects:
  - mark the execution unhealthy and remove it from the pool
  - for streams already bound to that execution: send an error frame or close to clients
  - new streams route to other executions
- Execution start failure:
  - if the pool is empty, fail the WS handshake (503/504)
  - otherwise keep serving on remaining executions and emit alerts/metrics

## 5. Data Plane Protocol (WS/SSF)

### 5.1 Overview

Client communicates with SMS via WebSocket, and SMS communicates with Spearlet (execution-scoped user-stream hub) via WebSocket. Business frames on both legs are SSF v1 binary frames (and we recommend explicit negotiation via `Sec-WebSocket-Protocol: ssf.v1`):

- `stream_id`: a unique u32 per logical request within the execution
- `msg_type`: distinguishes request/response/error (a fixed convention)
- `metadata`: JSON (method/path/headers/trace_id, etc.)
- `payload`: request body bytes (JSON or arbitrary bytes)

Task-side contract:

- Wait for ctl events (stream connected)
- For each stream: read inbound frames → handle → write outbound response frames on the same stream_id

Recommended additions (for cross-language SDKs and operations):

- `msg_type` convention:
  - `0x01` request
  - `0x02` response
  - `0x03` error
  - `0x04` cancel (client cancels the stream; SMS forwards without automatic retries)
- `metadata` schema (recommended fields):
  - `request_id`: required per stream (SMS generates if missing and propagates)
  - `traceparent`/`tracestate`: W3C Trace Context (if adopted system-wide)
  - `deadline_ms`: remaining time budget (SMS can enforce locally and use for routing)
  - `idempotency_key`: optional; enables safe retries/reroute only for explicitly idempotent calls
- `error` payload should be structured JSON (choose one location: either `metadata` or `payload`, but keep it consistent):
  - `code` (stable enum), `message` (developer-facing), `retryable` (bool), `details` (optional)
- Stream lifecycle:
  - After a terminal response/error frame, SMS reclaims the mapping immediately
  - For streams with no activity for too long, SMS may fail the stream and reclaim to avoid leaks (`stream_idle_timeout`)
- Frame size and protection (suggested defaults, configurable):
  - `max_frame_bytes` (e.g. 1–4MiB)
  - `max_active_streams_per_conn`, `max_active_streams_per_execution`
  - Prefer actionable errors (handshake: 413/429; post-handshake: error frame/close)

### 5.2 SSF Builder/Parser ownership

Extract SSF v1 build/parse into a reusable module (shared crate or a small SMS-side module) to avoid hand-crafted byte framing bugs.

## 6. End-to-End Sequence

1. Client → SMS: `GET /e/{endpoint}/ws` (WebSocket Upgrade)
2. SMS: validate endpoint; resolve task_id via index
3. SMS: find a usable execution; if none exists, start one and wait until ready
4. SMS: create stream-session token bound to execution_id
5. SMS: connect upstream WS to `ws://sms/api/v1/executions/{execution_id}/streams/ws?token=...` (or connect Spearlet directly)
6. SMS: start bidirectional proxy: Client WS ↔ Upstream WS (binary frames are forwarded)
7. Client: send SSF request frame (stream_id=rid)
8. Task: receives request via user-stream and writes back a response frame
9. Client: receives response frame (same stream_id)

## 7. Timeouts, Retries, Cancellation

- WS handshake + upstream readiness total timeout: 30s (configurable)
- Placement/startup timeout: 10s (configurable)
- Upstream WS connect timeout: 2s (configurable)
- WS idle timeout: 10min (configurable; close idle sessions and optionally reclaim execution)
- WS keepalive: enable ping/pong (e.g. 30s interval; close after N missed pongs) to detect half-open connections and release resources

Retries:

- If upstream WS connect fails or execution is unhealthy, try another execution (or rebuild) up to N times
- Request-level automatic retries require explicit idempotency (e.g. `idempotency_key` present or `metadata.method` is safe) and task-side idempotent behavior; default to no request-level retries
- On disconnect, close upstream WS; decide whether to terminate the execution based on lifecycle policy

Cancellation (recommended):

- Client sends `msg_type=cancel` to cancel a stream; SMS forwards upstream and reclaims the local mapping
- Upstream may respond with `error{code=CANCELLED}` or silently stop the stream; SDK should normalize both

## 8. Security

- Strict endpoint character set and length limit to prevent routing/path issues
- Must validate `Origin` (browser clients) and define TLS termination to prevent cross-site WS abuse and MITM risks
- Optional API-key/token authentication at the SMS handler, with a policy hook for endpoint-level ACL (RBAC/ABAC)
- Add connection/user/endpoint rate limiting and concurrency caps (prefer a shared limiter component or token bucket)
- WS token uses existing stream-session mechanism (TTL) to prevent direct external Spearlet access; recommend binding token to principal and execution_id with single-use or a short replay window
- Audit logs: record endpoint, principal (user/app), source, allow/deny decision, and stable reason codes

## 9. Observability

Suggested metrics/logs:

- endpoint resolution latency (endpoint → task_id)
- instance selection latency; placement latency
- WS connect latency
- roundtrip latency (HTTP→WS→HTTP)
- error categorization: not_found / no_instance / ws_fail / timeout / task_error

Trace:

- SMS generates `request_id` per call, includes it in SSF metadata and logs

Recommended additions (to avoid high-cardinality issues and improve alerting):

- Metric cardinality control: `endpoint` can be high-cardinality; consider sampling/bucketing or a feature flag; alert primarily on stage+code (resolve/start/connect/proxy)
- Structured log fields: `endpoint`, `task_id`, `execution_id`, `client_conn_id`, `client_stream_id`, `upstream_stream_id`, `request_id`, `error_code`, `latency_ms`
- Trace spans: handshake, upstream connect, and per-stream (request→response/error) spans; propagate `traceparent`

## 10. Implementation Placement (Suggested)

- SMS routing: add `.route("/e/{endpoint}/ws", get(endpoint_ws_proxy))` in `src/sms/routes.rs`
- New handler module: `src/sms/handlers/endpoint_gateway.rs`
- Endpoint index: maintain `endpoint -> task_id` in task store (updated on register/unregister; case-insensitive)
- Upstream WS: SMS connects to Spearlet as a WS client and proxies frames bidirectionally (optionally reuse stream-session token for stricter access control)

## 11. Web Console Support (Design)

Goal: enable Web Console (browser) to connect to an endpoint safely and observably, and interact in a request/response style for debugging or invocation.

### 11.1 UX / Interaction

- Endpoint picker: list tasks that have `endpoint`, with search/filter.
- Connection lifecycle: connect/disconnect, connection state, last error reason.
- Request panel (recommend exposing an “SSF request” abstraction):
  - `metadata` editor (JSON, with structured form view and raw mode)
  - `payload` editor (text/base64/hex, hints by content-type)
  - auto-generate and display `request_id` / `traceparent`
  - assign a local `client_stream_id` per request; show latency / error per stream
- History & replay: keep last N requests (prefer browser-local storage) and allow one-click replay.

### 11.2 Browser Protocol & Implementation

Browsers support binary WebSocket frames, so Web Console can act as a direct endpoint-gateway client:

- WebSocket: `wss://{sms}/e/{endpoint}/ws`
- Subprotocol: `Sec-WebSocket-Protocol: ssf.v1`
- Frame encoding: SSF v1 (ArrayBuffer). Each logical request uses one `stream_id` and ends with a response/error frame.

Implement a small SSF JS module (header parse/build + stream_id allocation) and enforce:

- `max_frame_bytes` (match server-side)
- per-connection concurrent stream cap (avoid accidental overload)

### 11.3 Auth & Security (Best Practice)

Browsers cannot attach arbitrary headers to WS requests (except subprotocol), so prefer a two-step flow:

1. Console calls HTTPS to mint a short-lived token (avoid exposing endpoint WS to cross-site scripts or unauthenticated users).
2. Console connects to WSS using a tokenized URL.

Recommended control-plane API (or reuse an existing “session minting” pattern):

- `POST /api/v1/endpoints/{endpoint}/session`
  - returns `ws_url` (with a short-lived token) and `expires_in_ms`
  - token binds to the authenticated principal (user/app/session) and can bind to `endpoint` plus policy flags (e.g. read-only/debug-only/prod-disabled)

On WS handshake, enforce:

- `Origin` allowlist (same-origin or trusted domains)
- TLS in production
- token validation and replay window (TTL / single-use configurable)
- connection/user rate limits and concurrency caps

### 11.4 Observability & Audit

- Console side: record `request_id`, start/end time, and outcome code per request (primarily for UI display).
- SMS side: structured logs for connection and per-stream spans; propagate `traceparent` to the task side.
- Audit: record who connected to which endpoint and when. Avoid logging full payloads by default; audit should only keep selected metadata keys.

### 11.5 Fit Into Existing Architecture

- Console does not need to understand scheduling details: SMS endpoint gateway still performs per-stream routing and stream_id rewrite.
- If production must forbid direct `/e/` access from Console, use the session-minting control plane above, plus environment/tenant/user gating and degradation.

## 12. Risks & Open Questions (Review Items)

1. Execution lifecycle per endpoint: always-on vs idle reclamation; do we need a minimum pool?
2. SSF msg_type/metadata conventions need to be finalized (especially error frames and header mapping)
3. Should we provide a higher-level task SDK (e.g. `Spear.endpoint.serve(handler)`) to standardize task behavior?
4. Route collision policy with existing `/console` and `/admin` (using `/e/` prefix avoids most conflicts)
