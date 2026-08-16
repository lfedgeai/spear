# Web Admin Overview

This document summarizes the current Web Admin in `spear-next`.

## What It Provides

- Independent port (default `127.0.0.1:8081`) with Axum router
- Nodes list with search, sort, pagination
- AI Backends control page for backend definitions, placements, and credentials
- AI Models list (read-only aggregated view; split into Local/Remote with a model detail page)
- AI Backends page-level `Backends` / `Model Views` switch
- Dedicated create flows for:
  - `Create Remote Backend`
  - `Create Local Backend`
- Backend detail page panels for:
  - placements
  - node status
  - read-model views
- MCP server management
- Execution history page
- Stats cards (total, online, offline, recent 60s)
- SSE stream `GET /admin/api/nodes/stream`
  - For testing: `?once=true` returns a single snapshot event
- Theme toggle (Dark/Light)
- Optional auth via `SMS_WEB_ADMIN_TOKEN` (Bearer token)

## Configuration

- Enable: `--enable-web-admin`
- Address: `--web-admin-addr 0.0.0.0:8081`
- ENV: `SMS_ENABLE_WEB_ADMIN`, `SMS_WEB_ADMIN_ADDR`

## Implementation Notes

- UI is delivered via embedded static files (`index.html`, `main.js`, `main.css`)
- UI source lives in `web-admin/` and build output overwrites `assets/admin/*`
- UI stack: Radix primitives + Tailwind (shadcn/ui style), enterprise console look
- SSE cancellation uses a `CancellationToken` to allow graceful shutdown

## Endpoints

- `GET /admin/api/nodes` → JSON list with `uuid`, `name`, `ip_address`, `port`, `status`, `last_heartbeat`, `registered_at`
- `GET /admin/api/nodes/:uuid` → Node + optional resource info
- `GET /admin/api/stats` → counts (total/online/offline/recent_60s)
- `GET /admin/api/nodes/stream[?once=true]` → SSE snapshot events
- `GET /admin/api/ai-model-views` → unified read-model view of AI Models (provider/model/hosting aggregation with placement/runtime details)
- `GET /admin/api/ai-backends` → canonical AI backend list (control-plane write resources)
- `GET /admin/api/ai-backend-placements` → AI backend placement list
- `GET /admin/api/ai-backend-statuses` → node-side runtime status list
- `GET /admin/api/ai-backend-statuses/{backend_id}` → node-side runtime status list for one backend
- `GET /admin/api/ai-backend-assignments/{node_uuid}` → assignments materialized onto one node
- `GET /admin/api/ai/credentials` → reusable credential registry for AI backends

### Historical Endpoints (Removed)

- The following legacy AI Models / model deployments / remote backends Web Admin endpoints are removed:
- `/admin/api/ai-models`
- `/admin/api/nodes/:node_uuid/ai-models*`
- `/admin/api/ai/remote-backends*`

### Tasks Endpoints

- `GET /admin/api/tasks` → returns task list with fields:
  - `task_id`, `name`, `description`, `status`, `priority`, `desired_replicas`, `scheduling_strategy`, `endpoint`, `version`

#### Create vs Execute (Two Flows)

Web Admin treats “create task (register)” and “execute task (schedule + run)” as two steps:

- Step 1: create/register
  - `POST /admin/api/tasks`
  - The request defines task spec rather than task ownership
  - The minimal scheduling-related fields are:
    - `desired_replicas`
    - `scheduling_strategy` (currently `spread`)
- Step 2: trigger execution (optional)
  - `POST /admin/api/invocations`
  - only invokes on nodes that already have ready replicas; if replicas are still converging, it returns warming up / no ready replicas

Behavior differences:

- Tasks no longer have pinned-node / owner-node semantics:
  - `task` represents workload spec
  - `instance` is what binds to a concrete `node_uuid`
  - execution is triggered via `POST /admin/api/invocations`; `Run after create` uses the current ready replicas, and returns warming up if assignments have not converged yet

## Secret/Key Management Guidance

If you add an “API key configuration” component to Web Admin, design it as “secret reference management”, not plaintext key entry/storage.

- UI/control-plane manages: mapping between backend instances and `credential_ref` (or `credential_refs`)
- Secret values are provisioned by: the deployment system (Kubernetes Secrets / Vault Agent / systemd drop-in)
- Observability: show only “present/usable” (e.g., spearlet heartbeat reports `HAS_ENV:<ENV_NAME>=true`), never the value
  - `executable_type`, `executable_uri`, `executable_name`
  - `registered_at`, `last_heartbeat`, `metadata`, `config`
  - `result_uris`, `last_result_uri`, `last_result_status`, `last_completed_at`, `last_result_metadata`
- `GET /admin/api/tasks/{task_id}` → returns detail with the same fields
- `POST /admin/api/tasks` → create task
  - Body includes `name`, `description`, `priority`, `desired_replicas`, `scheduling_strategy`, `endpoint`, `version`, `capabilities`, `metadata`, `config`, optional `executable`

## Testing

- Integration test for SSE uses `?once=true` to avoid blocking
- Frontend includes Playwright UI tests (`make test-ui`)

## Current UX Notes

- Dialogs use a viewport-constrained modal layout:
  - sticky header
  - scrollable body
  - fixed footer actions
- Long forms such as `Create Task` and `Create Local Backend` are grouped into explicit sections to reduce scanning cost.

## Related Current Docs

- [web-admin-ui-guide-en.md](./web-admin-ui-guide-en.md) for step-by-step user flows
- [backend-support-matrix-en.md](./backend-support-matrix-en.md) for the current backend capability / create matrix
- [ai-backend-unified-control-plane-design-en.md](./ai-backend-unified-control-plane-design-en.md) for the underlying control-plane model
