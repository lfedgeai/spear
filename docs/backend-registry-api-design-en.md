# Backend Registry API and Web Admin Tab Design

## Background

Spearlet builds a runtime backend registry from `SpearletConfig.ai.backends` and `ai.credentials`, materialized via [builder.rs](../src/spearlet/execution/ai/router/builder.rs). Today, there is no user-facing API to list and inspect all currently registered backends, and Web Admin has no view to display available backends across nodes.

Implementation note (current repo):

- Spearlet already reports a periodic backend snapshot to SMS (including local model “managed” backends).
- `hosting` is configuration-driven (required: `local|remote`); inference is intentionally avoided.

This document defines an elegant, best-practice design for:

- Backend listing APIs (node-scoped and aggregated), and
- A Web Admin tab to visualize available backends and their capabilities.

## Goals

- Provide node-scoped API to list active backends (those passing env checks).
- Provide cluster-scoped API to aggregate per-node backend availability.
- Ensure secrets are never exposed; only presence/availability indicators are shown.
- Offer efficient pagination, filtering, and incremental updates (SSE).

## Non-goals

- Full configuration editing in Web Admin.
- Exposing credential values; only show whether required envs are present.

## Data Model

BackendInfo (node-scoped):

- `name`: string
- `kind`: string (e.g., `openai_chat_completion`, `openai_realtime_ws`)
- `operations`: string[] (e.g., `chat_completions`)
- `features`: string[] (optional)
- `transports`: string[] (e.g., `http`, `ws`)
- `weight`: number
- `priority`: number
- `base_url`: string (if applicable)
- `status`: enum `available | unavailable`
- `status_reason`: string (e.g., `missing env OPENAI_CHAT_API_KEY`)
- `instance_id`: string (optional, if mapped to a runtime instance)

AggregatedBackendInfo (cluster-scoped):

- `name`: string
- `kind`: string
- `operations`: string[]
- `features`: string[]
- `transports`: string[]
- `available_nodes`: number
- `total_nodes`: number
- `nodes`: [{ `node_uuid`: string, `status`: `available|unavailable`, `status_reason`: string }]

## Spearlet API (Node-scoped)

Add a new service to Spearlet:

- gRPC: `BackendService.ListBackends()` → `BackendInfo[]`
- HTTP gateway: `GET /backends` → JSON array of `BackendInfo`

Implementation sketch:

- The registry is built by [builder.rs](../src/spearlet/execution/ai/router/builder.rs). Extend the function service or add a dedicated `BackendServiceImpl` that collects:
  - All instances from `BackendRegistry.instances()`.
  - For each configured backend that failed env checks, include it as `status=unavailable` with `status_reason`.

Filtering and pagination:

- Query params: `kind`, `operation`, `transport`, `status`, `limit`, `offset`.
- Default `limit=200` for node-scoped listing.

## Node Push Reporting (Option A, Recommended)

To avoid N×fanout calls (Web Admin aggregating by querying every node on every refresh), prefer a **push** model:

- Spearlet proactively reports its current backend snapshot to SMS.
- SMS stores the latest snapshot per node and serves cluster aggregation from storage.

Why not heartbeat?

- `HeartbeatRequest.health_info` is not persisted/used in SMS today.
- `map<string,string>` is not a good fit for an evolvable, structured backend capability model.

### RPC / proto sketch

Add a dedicated RPC in `proto/sms` (new service or extend `NodeService`):

- `ReportNodeBackends(ReportNodeBackendsRequest) -> ReportNodeBackendsResponse`

Request fields (recommended):

- `node_uuid`: string
- `reported_at_ms`: int64
- `revision`: uint64 (monotonic per node, for idempotency)
- `backends[]`: BackendInfo

Response:

- `success`: bool
- `message`: string
- `accepted_revision`: uint64

Security:

- Never report or return credential values.
- `status_reason` may include non-sensitive indicators (e.g., missing env var name), but never env values.

### SMS storage

Store the latest backend snapshot per node:

- `node_uuid -> { revision, reported_at_ms, backends[] }`

This can start as in-memory / existing KV abstraction and later move to Sled/RocksDB.

### Spearlet reporting triggers

- Report once after startup.
- Periodic full resync (e.g., 60s or 300s) as a safety net.
- Report on config/availability changes (credentials/env changes, hot reload).

## SMS Web Admin BFF (Cluster-scoped)

New endpoints:

- `GET /admin/api/backends` → `AggregatedBackendInfo[]`
  - Parameters: `kind`, `operation`, `status`, `limit`, `offset`.
  - Implementation: aggregate directly from SMS stored node snapshots.
  - Caching: optional short-lived cache (e.g., 5–15s) to reduce repeated aggregation work.

- `GET /admin/api/nodes/{uuid}/backends` → default reads the SMS snapshot for that node.
- `GET /admin/api/nodes/{uuid}/backends?source=node` → optional pass-through to node real-time API for debugging and consistency checks.

- `GET /admin/api/backends/stream[?once=true]` → SSE snapshots for incremental refresh.

SSE should be driven by push events: when SMS accepts `ReportNodeBackends`, it emits an update for that node.

Security:

- Never include credential values; only show required env names (optional) and whether present.
- Reuse `SMS_WEB_ADMIN_TOKEN` bearer auth for Web Admin endpoints.

## Web Admin UI

Add a new tab: “Backends”.

List view:

- Source: `GET /admin/api/backends`.
- Columns: name, kind, operations, transports, available_nodes/total_nodes, status.
- Filters: kind, operation, status.
- Actions: view node-wise distribution (opens details drawer), refresh.

Details drawer:

- Shows per-node availability: node name/uuid, status, status_reason.
- Optional: show `base_url` and capabilities.

Node-scoped view:

- From a node details page (existing nodes tab), add a “Backends” section calling `GET /admin/api/nodes/{uuid}/backends`.

UX best practices:

- Paginate and virtualize long lists.
- Use badges for status and chips for operations/transports.
- Persist filter state in query params for shareable links.

## Observability

- Metrics: aggregation latency, per-request fanout count, cache hit rate.
- Tracing: span per-node fetch; record error classification.

## Rollout Plan

Phase 0:

- Build BackendInfo list in Spearlet (including unavailable + reason).
- Implement `ReportNodeBackends` push from Spearlet to SMS.
- Store snapshots in SMS.
- Web Admin tab reads cluster-scoped view from SMS snapshots.

Phase 1:

- Add `source=node` pass-through for debugging and sampling.
- Add low-frequency pull audits to validate push correctness.

Phase 2:

- SSE stream for backend topology updates.
- Advanced filters and export.

## Compatibility & Safety

- If a node is unreachable, mark its backends as `status=unavailable` with `status_reason=unreachable` and continue aggregation.
- Do not degrade or leak secrets; surface only presence signals and public metadata.
