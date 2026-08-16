# SMS Web Admin Redesign

This document has two parts:

- Current state: what the existing SMS Web Admin already provides (features/APIs/limitations)
- Redesign proposal: a maintainable, extensible, observable, and evolvable front-end redesign (plus the required BFF/API contracts)

Target readers: SMS/Spearlet developers, platform operations, and scheduling/resource developers.

## 1. Background and Goals

### 1.1 Background

The current SMS Web Admin follows a “static frontend + backend-for-frontend (BFF)” architecture:

- Backend: Axum router that serves `/admin` and `/admin/api/*` JSON endpoints
- Frontend: an engineered frontend (React + Radix primitives + Tailwind + React Query), hash-based routing, built assets embedded into `assets/admin/*`

It is functional, but has the following issues:

- The frontend code was historically centralized, leading to high coupling and high change cost
- UI density and affordances are not ideal (many details are shown as raw JSON), and actionable UI is limited
- The auth model is minimal (optional Bearer token), lacking fine-grained permissions and auditability
- No consistent patterns for error/empty/loading states and reusable UI building blocks
- Mixed refresh strategies (polling + SSE) without a cohesive, domain-based refresh policy

### 1.2 Goals

- Modularization: split by domain (Nodes/Tasks/Files/Executions/Settings…), separate UI and API layers
- Extensibility: add new pages/modules without touching unrelated core logic
- Observability: quickly identify “why a node is offline / whether heartbeat is stale / whether resource reporting is stale”
- Visual polish: consistent layout/theme/density/detail views/table interactions; dark mode by default
- Best practices: typed frontend, linting, tests, routing, state boundaries, error boundaries, auth

Non-goals (not in this phase):

- Full RBAC / multi-tenancy
- Replacing the monitoring stack (Prometheus/Grafana remains primary)

## 2. Current State

### 2.1 Pages and Features (Current)

The frontend (source under `web-admin/`) currently contains 4 main pages:

- Nodes
  - StatsBar: Total/Online/Offline/Recent(60s)
  - Nodes Table: search/sort/pagination
  - Node detail: click UUID, show `/admin/api/nodes/{uuid}` JSON in a dialog
  - Refresh: React Query polling + (when no token) SSE-triggered refresh

- Tasks
  - Tasks Table: search/sort/pagination
  - Task Detail: JSON dialog
  - Create Task: form-based creation; supports executable config; can pick `smsfile://` URIs from Files

- Files
  - List/upload (presign + upload)/download/copy `smsfile://` URI/delete
  - Multi-select uploads executed sequentially (no backend batch APIs required)
  - File meta JSON dialog

- Settings
  - Dark mode toggle
  - Admin token store/apply

### 2.2 Backend Endpoints (Current `/admin/api`)

Routes live in [web_admin.rs](../src/sms/web_admin.rs).

#### 2.2.1 Authentication

- Optional env var `SMS_WEB_ADMIN_TOKEN`
  - If set: requires `Authorization: Bearer <token>`
  - If not set: no auth

#### 2.2.2 Nodes

- `GET /admin/api/nodes`
  - query: `status`, `q`, `sort` or `sort_by+order`, `limit`, `offset`
  - response: `{ nodes: NodeSummary[], total_count }`

- `GET /admin/api/nodes/{uuid}`
  - response: `{ found, node, resource }`
  - resource currently returns a subset: cpu/mem/disk + memory bytes

- `GET /admin/api/nodes/stream`
  - SSE
  - events: `snapshot`
  - query: `once=true` to return a single snapshot and exit

- `GET /admin/api/stats`
  - response: `{ total_count, online_count, offline_count, recent_60s_count }`

#### 2.2.3 Tasks

- `GET /admin/api/tasks`
  - query: `q`, `sort`, `sort_by+order`, `limit`, `offset`
  - response: `{ tasks: TaskSummary[], total_count }`

- `POST /admin/api/tasks`
  - registers a task (maps to gRPC RegisterTask)
  - response: `{ success, task_id, message }`
  - semantics:
    - tasks no longer bind to a single `node_uuid`
    - the request expresses workload spec, including `desired_replicas` and `scheduling_strategy`
    - execution is still triggered via `POST /admin/api/invocations`, and only targets nodes that already have ready replicas; if assignments are still converging, it returns warming up

- `GET /admin/api/tasks/{task_id}`
  - returns structured task detail JSON

#### 2.2.4 Executions

- `POST /admin/api/invocations`
  - BFF behavior:
    - If request includes `node_uuid=<uuid>`: execute directly on that node (no placement)
    - Otherwise: only invoke on nodes that already have ready replicas
    - If no replica is ready yet: best-effort trigger assignment reconcile, then return warming up / no ready replicas
  - response: `{ success, ... }` (currently not a unified schema)

#### 2.2.5 Files

- `GET /admin/api/files`
  - query: `q`, `limit`, `offset`
  - response: `{ files: FileItem[], total_count }`
- `POST /admin/api/files/presign-upload`
- `POST /admin/api/files` (upload)
- `GET /admin/api/files/{id}` (download)
- `DELETE /admin/api/files/{id}`
- `GET /admin/api/files/{id}/meta`

Note: the upload endpoint is “one file per request”. Multi-file upload is implemented as multiple requests from the UI. Batch/multipart APIs are only needed if you want true bulk presign/bulk upload.

### 2.3 Pain Points (Usability and Extensibility)

#### 2.3.1 Insufficient information structure

- Node details lack structured presentation for resources and heartbeat health
- “Offline reasons” are not easily diagnosable (e.g. last heartbeat time, timeout thresholds, resource report freshness)

#### 2.3.2 Frontend structure is hard to scale

- Historical single-file or tightly-coupled structure lacks clear module boundaries, reusable components, and consistent API typing

#### 2.3.3 Refresh strategy is inconsistent

- Tables use React Query polling; stats mixes polling and SSE
- No per-module/per-route refresh policy

## 3. Redesign Proposal (Frontend)

### 3.0 UI Stack Decision (Not AntD; a new console feel)

To intentionally provide a “new look and feel”, we avoid Ant Design in this redesign. The default assumption for design and implementation is the Radix + Tailwind (shadcn/ui) approach.

#### 3.0.1 Final stack

- Design system/components: shadcn/ui (Radix primitives, vendored components for customization)
- Styling: Tailwind CSS + CSS variables (theme tokens), supports dark mode
- Icons: lucide-react
- Routing: react-router
- Data fetching/caching: @tanstack/react-query
- Tables: @tanstack/react-table (wrap into a unified DataTable component: filter/pagination/sort/empty/skeleton)
- Forms/validation: react-hook-form + zod
- Feedback: sonner (toast)
- Charts (optional for Dashboard): echarts preferred, recharts as fallback

#### 3.0.2 Rationale

- A clear visual departure from AntD; closer to modern SaaS consoles
- Radix primitives + Tailwind are a good fit for widget-based dashboards
- Easier to build feature-scoped components instead of relying on a global monolith component library

#### 3.0.3 Costs and constraints

- Requires a real build pipeline (Vite/TS), no longer suitable for “pure CDN single file”
- Requires building a small internal component toolkit (DataTable/FormField/PageLayout/Empty/Error/WidgetContainer)

#### 3.0.4 Component layers and boundaries

- `src/components/ui/*`: shadcn/ui generated and lightly customized primitives (Button/Card/Dialog/Drawer/Tabs/DropdownMenu…)
- `src/components/*`: reusable, domain-agnostic composites (DataTable/JsonViewer/PageHeader/EmptyState/ErrorState/WidgetContainer…)
- `src/features/*`: domain code (Nodes/Tasks/Files/Executions/Dashboard)

#### 3.0.4.1 Code split principles

- Feature-first: each module is self-contained under `src/features/<feature>` (pages/components/hooks/local types)
- Dependency direction rules:
  - `features/*` may depend on `components/*`, `components/ui/*`, `api/*`, `lib/*`
  - `components/*` must not depend on `features/*`
  - `api/*` must not depend on UI
- `src/app/*` only owns layout/routes/global providers

Suggested directory shape:

```
src/
  app/
    App.tsx
    AppShell.tsx
    routes.tsx
    providers/
  api/
    client.ts
    nodes.ts
    tasks.ts
    files.ts
    stats.ts
    executions.ts
    types.ts
  components/
    ui/
    DataTable/
    EmptyState/
    ErrorState/
    WidgetContainer/
  features/
    dashboard/
      DashboardPage.tsx
      widgets/
    nodes/
      NodesPage.tsx
      NodeDetailDialog.tsx
    tasks/
    files/
    settings/
      SettingsPage.tsx
  lib/
    utils.ts
```

#### 3.0.5 Theme and visual guidelines (recommended)

- Tokens: CSS variables (`--background/--foreground/--muted/--border/--primary/...`)
- Style: “enterprise console” (restrained, readable, stable); minimal gradients/decorations
- Typography: default `text-sm` for density; optional compact tables with `text-xs`
- Radius/shadows: small radius (6–8px), light shadows, use borders/dividers for hierarchy
- Status colors: use Badge/Chip + icon; don’t rely on color alone
- Dangerous actions: always require confirmation dialogs
- Layout: left nav + top toolbar; content area with cards/tables on a subtle background

### 3.1 Overall architecture

Keep the Admin BFF model, but upgrade the frontend into a proper project:

- Frontend: TypeScript + React + routing + component toolkit + server state
- API: unified apiClient (baseURL/auth headers/error normalization/retry policy)
- State:
  - Server state: TanStack Query
  - UI state: local state + URL search params
  - Global: Theme/Auth/Timezone lightweight contexts

Suggested layout:

```
admin/
  src/
    components/
      ui/
    app/
      App.tsx
      routes.tsx
      layout/
      providers/
    features/
      nodes/
      tasks/
      files/
      executions/
      dashboard/
      settings/
    components/
      DataTable/
      JsonViewer/
      PageHeader/
      HealthBadge/
      EmptyState/
      ErrorState/
      WidgetContainer/
    api/
      client.ts
      types.ts
      nodes.ts
      tasks.ts
      files.ts
      executions.ts
    theme/
    utils/
  index.html
  package.json
```

### 3.2 Information Architecture (IA) and page design

#### 3.2.1 Navigation

- Dashboard (new, default landing)
  - Cluster overview: Nodes (online/offline/recent heartbeat), Tasks (active/inactive), Files (count/total bytes)
  - Health summary: top N offline nodes, heartbeat-stale nodes, resource-report-stale nodes
  - Activity: recent registrations/offline transitions, recent task changes, recent file uploads
  - Quick actions: Create Task, Run Execution, Upload Files

- Nodes
  - List: table + filters + refresh/auto-refresh toggle
  - Detail: structured sections (Summary/Resources/Metadata/Recent events)

- Tasks
  - List
  - Detail (visualize task spec/executable/last result)
  - Create/Edit (step form)

- Executions
  - Trigger execution (naturally from Task detail)
  - Execution history (requires backend support; or show “current execution result” only at first)

- Files
  - List/upload/download/delete
  - Structured meta view

- Settings
  - Theme/TZ/Token
  - “connection status / version info”

#### 3.2.0 Dashboard (pluggable widget design)

Dashboard should be actionable overview, not a monitoring replacement. Since contents will evolve, implement it as a pluggable widget system.

- Principles:
  - Widget-based: dashboard is composed of independent widgets
  - Low coupling: widgets fetch via the API client only; no cross-widget state dependencies
  - Config-driven: layout/visibility/refresh/permissions are configuration
  - Degradable: widget failures don’t break the page; each widget supports skeleton/empty/error

- Layout skeleton:
  - Top: time range/timezone, refresh button, auto-refresh toggle
  - Body: responsive grid, each cell renders a widget

- Registry/extension:
  - A `WidgetRegistry`: `id -> ReactComponent + defaultConfig + dataDependencies`
  - Dashboard reads config → selects widgets → renders containers
  - Each widget includes:
    - `title/description`
    - `requiredPermissions`
    - `queryKeys` + `refetchPolicy`

- Config model:
  - `DashboardLayout`:
    - `widgets[]`: `{ id, enabled, size, order, props, refreshIntervalSeconds }`
    - `breakpoints`: `{ xs, md, lg }` (#columns/gaps)
  - Config sources:
    - P0: built-in defaults + localStorage overrides
    - P1: backend endpoint `GET /admin/api/dashboard/layout`

- Data sources:
  - P0: reuse existing endpoints `GET /admin/api/stats`, `GET /admin/api/nodes?limit=...`, `GET /admin/api/tasks?limit=...`, `GET /admin/api/files`
  - P1: add `GET /admin/api/dashboard` as an aggregation endpoint

- Observability and maintainability:
  - per-widget metrics: load latency, error class (network/auth/server), last updated time
  - error boundary isolation for render failures

#### 3.2.2 Nodes list (design)

- Top bar:
  - search (uuid/ip/name/meta)
  - status filter (online/offline/all)
  - refresh: manual + auto (default 15s)
  - density toggle (comfortable/compact)

- Table columns:
  - Name (prefer metadata.name, fallback “-”)
  - UUID (copy)
  - Status (Online/Offline/Degraded)
  - Last heartbeat (relative time + hover absolute)
  - Resource (CPU/Mem/Disk bars/badges)
  - Address (ip:port)
  - Actions (View/Copy/…)

#### 3.2.3 Node detail (design)

Use right-side Drawer or standalone route (prefer `#/nodes/:uuid` for deep linking).

- Summary
  - status, ip:port, registered_at, last_heartbeat
  - Health computed from heartbeat timeout

- Resources
  - CPU/Mem/Disk bars + numbers
  - memory bytes used/total
  - disk bytes used/total
  - resource updated_at

- Metadata
  - KV table

- Actions
  - copy node UUID
  - quick create task (prefill placement constraints / replica policy in the future)

#### 3.2.4 Tasks

- List: status/priority/target node/last result
- Detail:
  - Spec (endpoint/version/capabilities)
  - Executable (type/uri/name/args/env)
  - Last result (last_result_uri/status/completed_at)
  - Actions: run once (sync/async/stream), unregister task

#### 3.2.5 Files

- List: Name/Size/Modified/ID
- Detail: meta + copy URI
- Upload:
  - drag & drop + multi-select
  - upload queue (sequential by default; configurable concurrency 2/3)
  - per-file failure does not stop the queue; show summary at the end
  - refresh after upload

### 3.3 UI/UX best practices

- Routing and deep links: detail pages should be openable by URL
- Accessibility: proper labels, keyboard navigation, contrast
- Consistent empty/error states: `EmptyState`/`ErrorState`
- Consistent time: unified TZ settings + relative time
- Table UX: configurable columns/width/copy actions/bulk action hooks
- Theme: light/dark with unified tokens

## 4. Redesign Proposal (Backend API / BFF Contract)

`/admin/api` already covers core functionality, but the frontend benefits from unified schemas and a few missing fields.

### 4.1 Unified response envelope

Recommend a consistent response shape:

```
type ApiResponse<T> =
  | { ok: true; data: T }
  | { ok: false; error: { code: string; message: string; details?: any } }
```

So the frontend can implement a single error-handling layer.

### 4.2 Nodes API recommendations

- `GET /admin/api/nodes`
  - include `heartbeat_timeout_seconds` (for health computation)
  - include `now_ts` (avoid client/server clock skew)

- `GET /admin/api/nodes/{uuid}`
  - return full resource fields (including `updated_at`, disk bytes, load averages)
  - include computed fields:
    - `is_heartbeat_stale`
    - `heartbeat_age_seconds`

### 4.3 Executions API recommendations

The current `/admin/api/invocations` is a BFF endpoint that triggers one invoke and returns the result/state.

Recommended additions:

- `POST /admin/api/invocations`
  - request:
    - task_id
    - mode (sync/async/stream)
    - labels/metadata
  - response:
    - request_id / execution_id / node_uuid
    - warming_up / no_ready_replicas (when assignments have not converged yet)
    - final_result (on success)

Future:

- `GET /admin/api/executions?task_id=&limit=&offset=` (requires server-side storage)

### 4.4 Auth and audit

- Keep Bearer token, but consider:
  - multiple tokens (read-only/admin)
  - a `whoami` endpoint or response header (e.g. `x-sms-admin-user`) for UI display
  - audit logs for dangerous actions (delete file/create task/execute)

## 5. Engineering and Delivery

### 5.1 Build and static asset delivery

Recommended:

- Use Vite build artifacts (chunking, hashed filenames, gzip/brotli)
- SMS backend continues to serve them via `include_bytes!` or from a static directory

Compared to the prior “single-file CDN” approach, Radix + Tailwind (shadcn/ui) requires building a standalone frontend project and delivering static assets.

Two delivery options:

1) Embed mode (current style)
   - `assets/admin/dist/*` embedded via `include_bytes!`
   - pros: simple deployment
   - cons: longer compile time, larger binary

2) Static-dir mode (recommended)
   - SMS loads assets from `--web-admin-static-dir` or config
   - pros: decoupled frontend/backend
   - cons: requires shipping extra files

### 5.2 Testing strategy

- Frontend:
  - unit tests (utils/query param parsing/formatters/api client)
  - component tests (tables/filters/detail dialogs/danger confirmations)
  - e2e (optional): start SMS locally and run Playwright against real `/admin/api` (or MSW mocks)

Recommended testing enablement:

- Testable API layer:
  - `api/client.ts` should allow injecting `baseUrl`/`fetch`
  - `api/*` handles only request + type mapping
- Testable UI structure:
  - split large pages into smaller components (e.g. `NodeDetailDialog`)
  - move pure logic (formatting/filtering/sorting) into `lib/*` and unit-test it
- Query control in tests:
  - test-specific QueryClient (disable retries/polling)
  - toggles for polling/SSE (on in prod, off in tests)

Suggested toolchain:

- unit/component: Vitest + Testing Library + jsdom
- mocking: MSW (recommended) or lightweight fetch mocks
- e2e: Playwright (smoke first)

- Backend:
  - extend existing integration tests for nodes/tasks/files/executions schema

### 5.3 Compatibility and migration

## 8. Key Interaction Details (Appendix)

### 8.1 Unified search/filter model

All list pages should use URL search params for shareability and replay:

- `q`: free text
- `status`: enum
- `sort_by` + `order`
- `limit` + `offset` (or `page` + `page_size`)

Frontend suggestion:

- manage conditions via `useSearchParams`
- conditions update URL only; fetch is driven by Query keys

### 8.2 Refresh strategy

Recommend “configurable auto refresh + visibility optimization”:

- list pages default 15s auto-refresh (Nodes/Tasks), Files default manual
- pause polling when page is not visible
- SSE is a “change hint” that triggers throttled invalidations, not a live streaming data channel

### 8.3 Time display

- Use one global time base: backend returns `now_ts` and `*_at` fields (unix seconds)
- Frontend shows:
  - relative time (e.g. 12s ago)
  - hover absolute time (user TZ)

### 8.4 Detail page organization

- support both Drawer (quick view) and standalone route (deep link)
- Drawer and route share the same panel components (e.g. `NodeDetailPanel`)

### 8.5 Error handling and observability

- api client error normalization:
  - network (offline/timeout)
  - 401/403 (token invalid)
  - 429/5xx (backoff/retry)
- list pages: `ErrorState` + retry
- action errors: toast + expandable details

## 9. Backend fields/endpoints backlog (prioritized)

P0 (immediate value):

- Nodes list/detail: expose `resource.updated_at` and explicit `node.metadata.name` mapping
- Nodes: return `now_ts` and `heartbeat_timeout_seconds`
- Executions: unify schema (attempts + final)

P1 (UX improvements):

- Nodes: `resource.load_average_*`, disk bytes, network bytes
- Tasks: structured “last execution status” fields (currently last_result_* exists but semantics need clarity)

P2 (observability):

- SSE: add typed events (node_updated/task_updated/file_updated) with minimal payloads
- Executions: add history query endpoint

Migration plan:

- Phase 1: keep `/admin/api/*` compatible, redesign frontend first
- Phase 2: introduce unified schema/missing fields in an additive way
- Phase 3: add executions history and other new capabilities

## 6. Milestones (Suggested)

- M1: frontend project bootstrap (Vite/TS/Tailwind/shadcn) + Layout/Theme/Router + Dashboard skeleton + Nodes/Tasks/Files base pages
- M2: structured detail views (Node/Task/File) + unified error handling + configurable refresh
- M3: Executions page and spillback visualization + audit logs + read-only token

## 7. Appendix: Code entry points

- Backend router: `create_admin_router` ([web_admin.rs](../src/sms/web_admin.rs#L128-L209))
- Nodes API: `list_nodes`/`get_node_detail` ([web_admin.rs](../src/sms/web_admin.rs#L398-L473))
- Stats API: `get_stats` ([web_admin.rs](../src/sms/web_admin.rs#L839-L870))
- Frontend entry: [main.tsx](../web-admin/src/main.tsx)
- Built assets: `assets/admin/index.html`, `assets/admin/main.js`, `assets/admin/main.css`
