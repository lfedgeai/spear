# AI Backend Unified Control Plane Design

## Background

The current SMS/Spearlet control plane for AI backends is split across two separate paths:

- `remote backend`
  - Persisted by SMS
  - Synchronized to nodes through Spearlet `BackendAssignmentController`
  - Runtime snapshots are then reported back through `backend_reporter`
- `local deployment`
  - Managed by `model_deployment_registry`
  - Driven by Spearlet's unified assignment controller to start and stop local model services
  - Runtime snapshots are also reported back through `backend_reporter`

In the UI, the `AI Models` page further aggregates node-reported snapshots by `(provider, model, hosting)`, and treats that aggregation as both:

- an observation view
- a control entry point

This creates a set of structural problems:

- The control-plane primary entity has no stable identity, and `provider/model/hosting` is misused as an approximate primary key
- `remote` and `local` use split resource models, so adding a new backend driver requires duplicate control-plane work
- The "model aggregation view" and the "control-plane resource" are mixed together, which makes delete, enable/disable, pagination, and counting semantics inaccurate
- Node enable/disable has no explicit placement semantics and can only be expressed indirectly through deletion, synchronization, or credential state
- The semantics of observed state and desired state are mixed together

This design document proposes a **non-backward-compatible** new architecture that unifies `remote` and `local` into a single AI backend control plane built around UUID-based identity.

## Goals

### Core Goals

- Use `backend_id(uuid)` as the unique identity for every AI backend
- Use one unified control-plane resource model for both `remote` and `local`
- Elevate "which nodes enable this backend" from implicit logic into an explicit `placement` resource
- Change node-reported state from anonymous snapshots into observed state bound to `backend_id`
- Downgrade the `AI Models` page into a read-only aggregation view instead of a destructive/control surface
- Provide a clean foundation for future driver expansion, scheduling, state convergence, auditing, and persistence

### Non-Goals

- Do not migrate legacy `remote backend` or `model deployment` data
- Do not preserve compatibility with the current Web Admin API
- Do not preserve compatibility with the current delete/edit behavior on the `AI Models` page
- Do not solve every provider-specific local model detail in this design

## Design Principles

### 1. Stable Identity First

- `backend_id` is the control-plane primary key
- `provider/model/hosting` are attributes, not identity
- `spec.name` no longer acts as a primary key and only remains part of a runtime-visible name

### 2. Separate Control Plane and Observation Plane

- SMS stores `desired state`
- Spearlet reports `observed state`
- `AI Models` is a read model, not the source of truth

### 3. One Model for Remote and Local

- `remote` and `local` share the same control-plane resource model
- Their differences only exist in node-side materialization and driver implementation

### 4. Explicit Placement

- Whether a backend is enabled on a node must not be inferred from synchronization, deletion, or side effects
- It must be expressed explicitly by an SMS placement resource

### 5. Idempotent Node Controllers

- Spearlet only handles backends assigned to the current node
- Reconcile logic must converge idempotently by `backend_id`

### 6. Typed Models First

- The new control-plane resources, status records, and read models all use typed proto/domain models
- Loose JSON aggregation objects are no longer used as the main workflow input

## Summary of Current Problems

### 1. Split Control Planes

- `remote backend` uses `AdminBackendsState`
- `local deployment` uses `ModelDeploymentRegistryState`
- Their lifecycle, persistence, field semantics, and state flows are all different

### 2. The Aggregation View Carries Control Semantics

The current `AI Models` list aggregates by `(provider, model, hosting)`. That is acceptable for observation, but not for control. It naturally causes:

- one row mapping to multiple backend definitions
- deleting one row possibly deleting multiple backends
- incorrect pagination and total-count semantics
- node count duplication when instances are counted repeatedly

### 3. No Explicit Enable/Disable Placement Semantics

In the current system:

- `remote backend` lifecycle is mainly controlled by create/delete
- `local deployment` lifecycle is mainly controlled by create/delete
- `credential.disabled` indirectly affects runtime availability

This mixes together:

- whether a resource configuration exists
- whether it is intended to be enabled
- whether credentials are valid
- whether the node is currently ready

These states come from different sources, but are currently blended together, making the system hard to explain and hard to govern.

### 4. Node-Reported State Is Not Bound to Control-Plane Identity

Current node reports mainly use `backend_name/spec/status` snapshots and lack stable `backend_id` association, so SMS cannot precisely distinguish:

- which control-plane backend produced the implementation result
- whether the current status matches the latest generation

## New Architecture Overview

The new architecture is divided into 4 layers:

1. **AiBackend**
   - Primary control-plane resource
   - Carries a UUID
   - Describes "which backend is desired"
2. **AiBackendPlacement**
   - Controls "which nodes should enable this backend"
3. **AiBackendNodeStatus**
   - Reports "the actual state of this backend on this node"
4. **AiModelView**
   - Read-only operational view aggregated by `provider/model/hosting`

Among them:

- `AiBackend` + `Placement` = desired state
- `AiBackendNodeStatus` = observed state
- `AiModelView` = read model

## Unified Resource Model

### AiBackendRecord

The primary control-plane resource for an AI backend.

Suggested fields:

- `backend_id: string`
- `display_name: string`
- `provider: string`
- `model: string`
- `hosting: enum { REMOTE, LOCAL }`
- `backend_kind: string`
- `desired_state: enum { ENABLED, DISABLED }`
- `management_mode: enum { SMS_REMOTE, SMS_LOCAL }`
- `credential_ref: optional<string>`
- `spec: BackendSpec`
- `labels: map<string, string>`
- `metadata: Struct`
- `generation: uint64`
- `created_at_ms`
- `updated_at_ms`

Notes:

- `spec.name` is no longer the control-plane identity
- SMS can consistently generate `spec.name = "backend-{backend_id}"` when creating a backend
- `provider/model` must be explicit fields rather than inferred values

### AiBackendPlacementRecord

Expresses explicitly "this backend should be enabled on this node."

The first phase should use the simplest per-node placement model:

- `placement_id: string`
- `backend_id: string`
- `node_uuid: string`
- `desired_state: enum { ENABLED, DISABLED }`
- `weight_override: optional<int32>`
- `priority_override: optional<int32>`
- `generation: uint64`
- `created_at_ms`
- `updated_at_ms`

Possible future extensions:

- `node_selector`
- `placement_policy`
- `rollout_strategy`

### AiBackendNodeStatusRecord

The actual state observed by a node.

Suggested fields:

- `backend_id: string`
- `node_uuid: string`
- `observed_generation: uint64`
- `status: enum { PENDING, RECONCILING, READY, DEGRADED, ERROR, DISABLED }`
- `status_reason: string`
- `runtime_backend_name: string`
- `endpoint: string`
- `available: bool`
- `operations: repeated string`
- `features: repeated string`
- `transports: repeated string`
- `last_heartbeat_at_ms`

Key points:

- `backend_id + node_uuid` is unique
- Node status is bound to generation, making stale status detection straightforward

### AiModelView

This is a read-only aggregation view and does not carry control semantics.

Aggregation key:

- `provider`
- `model`
- `hosting`

Suggested fields:

- `provider`
- `model`
- `hosting`
- `backend_ids`
- `operations`
- `features`
- `transports`
- `ready_nodes`
- `total_nodes`
- `instances`

## How Remote and Local Become Unified

### Remote Backend

Flow:

1. SMS creates `AiBackend(hosting=REMOTE)`
2. SMS creates placements
3. Spearlet observes placements
4. `remote_driver` materializes the backend into a runtime backend
5. Spearlet reports `AiBackendNodeStatus`

Characteristics:

- No local model process is started
- It only injects the remote endpoint backend into runtime

### Local Backend

Flow:

1. SMS creates `AiBackend(hosting=LOCAL)`
2. SMS creates placements
3. Spearlet observes placements
4. The corresponding local driver starts the local model service
5. It produces the endpoint/base_url
6. It injects the runtime backend
7. It reports `AiBackendNodeStatus`

Characteristics:

- The driver owns the actual process/model lifecycle

### Key Conclusion

The difference between `remote` and `local` only exists at the driver layer:

- `remote_driver`
- `llamacpp_driver`
- `vllm_driver`

The upper-layer control-plane resource model remains exactly the same.

## Proto Draft

### `proto/sms/ai_backend.proto`

```proto
syntax = "proto3";

package sms;

import "google/protobuf/struct.proto";
import "sms/backend_spec.proto";

enum AiBackendHosting {
  AI_BACKEND_HOSTING_UNSPECIFIED = 0;
  AI_BACKEND_HOSTING_REMOTE = 1;
  AI_BACKEND_HOSTING_LOCAL = 2;
}

enum AiBackendDesiredState {
  AI_BACKEND_DESIRED_STATE_UNSPECIFIED = 0;
  AI_BACKEND_DESIRED_STATE_ENABLED = 1;
  AI_BACKEND_DESIRED_STATE_DISABLED = 2;
}

enum AiBackendManagementMode {
  AI_BACKEND_MANAGEMENT_MODE_UNSPECIFIED = 0;
  AI_BACKEND_MANAGEMENT_MODE_SMS_REMOTE = 1;
  AI_BACKEND_MANAGEMENT_MODE_SMS_LOCAL = 2;
}

message AiBackendRecord {
  string backend_id = 1;
  string display_name = 2;
  string provider = 3;
  string model = 4;
  AiBackendHosting hosting = 5;
  string backend_kind = 6;
  AiBackendDesiredState desired_state = 7;
  AiBackendManagementMode management_mode = 8;
  string credential_ref = 9;
  BackendSpec spec = 10;
  map<string, string> labels = 11;
  google.protobuf.Struct metadata = 12;
  uint64 generation = 13;
  int64 created_at_ms = 14;
  int64 updated_at_ms = 15;
}
```

### `proto/sms/ai_backend_placement.proto`

```proto
syntax = "proto3";

package sms;

import "sms/ai_backend.proto";

message AiBackendPlacementRecord {
  string placement_id = 1;
  string backend_id = 2;
  string node_uuid = 3;
  AiBackendDesiredState desired_state = 4;
  optional int32 weight_override = 5;
  optional int32 priority_override = 6;
  uint64 generation = 7;
  int64 created_at_ms = 8;
  int64 updated_at_ms = 9;
}
```

### `proto/sms/ai_backend_status.proto`

```proto
syntax = "proto3";

package sms;

enum AiBackendNodeStatus {
  AI_BACKEND_NODE_STATUS_UNSPECIFIED = 0;
  AI_BACKEND_NODE_STATUS_PENDING = 1;
  AI_BACKEND_NODE_STATUS_RECONCILING = 2;
  AI_BACKEND_NODE_STATUS_READY = 3;
  AI_BACKEND_NODE_STATUS_DEGRADED = 4;
  AI_BACKEND_NODE_STATUS_ERROR = 5;
  AI_BACKEND_NODE_STATUS_DISABLED = 6;
}

message AiBackendNodeStatusRecord {
  string backend_id = 1;
  string node_uuid = 2;
  uint64 observed_generation = 3;
  AiBackendNodeStatus status = 4;
  string status_reason = 5;
  string runtime_backend_name = 6;
  string endpoint = 7;
  bool available = 8;
  repeated string operations = 9;
  repeated string features = 10;
  repeated string transports = 11;
  int64 last_heartbeat_at_ms = 12;
}
```

## Module Split

### SMS

Suggested new directory:

```text
src/sms/ai_backends/
  mod.rs
  model.rs
  proto_conv.rs
  repository.rs
  repository_kv.rs
  service.rs
  placement_service.rs
  status_service.rs
  read_model.rs
  validator.rs
```

Responsibilities:

- `model.rs`
  - domain structs
- `proto_conv.rs`
  - proto/domain conversion
- `repository.rs`
  - trait definitions
- `repository_kv.rs`
  - KV-backed implementation
- `service.rs`
  - backend CRUD and enable/disable
- `placement_service.rs`
  - placement CRUD
- `status_service.rs`
  - node status reporting and cleanup
- `read_model.rs`
  - `AiModelView` aggregation
- `validator.rs`
  - input validation and constraints

### Spearlet

Suggested new directory:

```text
src/spearlet/ai/backends/
  mod.rs
  assignment_controller.rs
  driver.rs
  remote_driver.rs
  llamacpp_driver.rs
  vllm_driver.rs
  runtime_registry.rs
  status_reporter.rs
  materializer.rs
```

Responsibilities:

- `assignment_controller.rs`
  - watch placements for the current node
  - load backend records
  - produce node-local desired assignments
- `driver.rs`
  - shared driver trait
- `remote_driver.rs`
  - remote backend materialization
- `llamacpp_driver.rs`
  - llama.cpp process management
- `vllm_driver.rs`
  - vLLM process management
- `runtime_registry.rs`
  - manage runtime backends keyed by `backend_id`
- `status_reporter.rs`
  - report status keyed by `backend_id`

### Web Admin

Suggested new directory:

```text
web-admin/src/features/ai-backends/
  AiBackendsPage.tsx
  AiBackendDetailPage.tsx
  AiBackendEditorDialog.tsx
  PlacementPanel.tsx
  PlacementEditorDialog.tsx
  NodeStatusPanel.tsx
  queries.ts
  types.ts
```

The information architecture should be split into three layers:

- `AI Backends`
  - control-plane resource page
- `Placements`
  - node enablement relationship page
- `AI Models`
  - read-only operational aggregation page

## KV Storage Design

In phase 1, all control-plane resources can initially live inside the admin KV store.

### Primary Records

- `ai:backend:{backend_id}`
- `ai:placement:{placement_id}`
- `ai:status:{backend_id}:{node_uuid}`

### Secondary Indexes

- `ai:index:placements_by_backend:{backend_id}:{placement_id}`
- `ai:index:placements_by_node:{node_uuid}:{placement_id}`
- `ai:index:statuses_by_backend:{backend_id}:{node_uuid}`

### Principles

- Writes must be idempotent
- Deletion must clean up indexes
- Prefer typed record serialization
- Do not optimize for a complex transaction model in phase 1

## Core Service Interface Sketch

### Repository

```rust
pub trait AiBackendRepository {
    async fn insert_backend(&self, record: AiBackendRecord) -> Result<AiBackendRecord, SmsError>;
    async fn update_backend(&self, record: AiBackendRecord) -> Result<AiBackendRecord, SmsError>;
    async fn get_backend(&self, backend_id: &str) -> Result<Option<AiBackendRecord>, SmsError>;
    async fn delete_backend(&self, backend_id: &str) -> Result<bool, SmsError>;

    async fn upsert_placement(
        &self,
        record: AiBackendPlacementRecord,
    ) -> Result<AiBackendPlacementRecord, SmsError>;

    async fn upsert_node_status(
        &self,
        record: AiBackendNodeStatusRecord,
    ) -> Result<AiBackendNodeStatusRecord, SmsError>;
}
```

### Backend Service

```rust
pub trait AiBackendService {
    async fn create_backend(&self, input: CreateAiBackendInput) -> Result<AiBackendRecord, SmsError>;
    async fn update_backend(&self, input: UpdateAiBackendInput) -> Result<AiBackendRecord, SmsError>;
    async fn delete_backend(&self, backend_id: &str) -> Result<bool, SmsError>;
    async fn enable_backend(&self, backend_id: &str) -> Result<AiBackendRecord, SmsError>;
    async fn disable_backend(&self, backend_id: &str) -> Result<AiBackendRecord, SmsError>;
}
```

### Placement Service

```rust
pub trait AiBackendPlacementService {
    async fn upsert_placement(
        &self,
        input: UpsertPlacementInput,
    ) -> Result<AiBackendPlacementRecord, SmsError>;

    async fn delete_placement(&self, placement_id: &str) -> Result<bool, SmsError>;

    async fn list_node_assignments(
        &self,
        node_uuid: &str,
    ) -> Result<Vec<ResolvedBackendAssignment>, SmsError>;
}
```

### Status Service

```rust
pub trait AiBackendStatusService {
    async fn report_statuses(
        &self,
        node_uuid: &str,
        records: Vec<AiBackendNodeStatusRecord>,
    ) -> Result<(), SmsError>;

    async fn cleanup_stale_statuses(&self, older_than_ms: i64) -> Result<u64, SmsError>;
}
```

## Node-Side Reconcile Flow

The node side should no longer keep separate parallel flows for "remote sync" and "local controller". Instead, it should unify them into one assignment controller.

### Inputs

- placements for the current node
- backend records referenced by those placements

### Intermediate State

```rust
pub struct ResolvedBackendAssignment {
    pub backend: AiBackendRecord,
    pub placement: AiBackendPlacementRecord,
}
```

### Outputs

- the set of runtime backends that should be materialized on the current node

### Reconcile Flow

1. Load all placements for the current node
2. Filter `placement.desired_state == ENABLED`
3. Load the referenced backend records
4. Filter `backend.desired_state == ENABLED`
5. Build the desired assignment set
6. Compare it against the current runtime set
7. Send new assignments to driver `reconcile`
8. Send removed or disabled assignments to driver `disable`
9. Generate and report node status

### Driver Selection

- `hosting = REMOTE` -> `remote_driver`
- `hosting = LOCAL && backend_kind = llamacpp_*` -> `llamacpp_driver`
- `hosting = LOCAL && backend_kind = vllm_*` -> `vllm_driver`

## UI Information Architecture

### AI Backends

This becomes the new primary control-plane resource page, with one row per `backend_id`.

Displayed fields:

- Name
- Provider
- Model
- Hosting
- Kind
- Desired State
- Enabled Nodes
- Ready Nodes

Actions:

- Create
- Edit
- Enable/Disable
- Delete
- Manage Placements

### AI Backend Detail

Recommended tabs:

- `Overview`
- `Placements`
- `Node Status`
- `Spec`

### AI Models

Keep the page, but downgrade it into a pure read model:

- no delete
- no enable/disable
- responsible only for:
  - provider/model aggregation visibility
  - backend coverage
  - node readiness

## Phased Implementation Plan

### Phase 0

Define the new world without replacing the old world.

Deliverables:

- new proto
- new domain model
- new repository/service skeleton
- new read-model skeleton
- new unit-test skeleton

Do not:

- connect old UI
- migrate old data
- delete old logic

### Phase 1

Wire up the SMS control plane.

Deliverables:

- backend CRUD
- placement CRUD
- status reporting
- node deletion -> status cleanup
- `AiModelView` aggregation

Acceptance:

- `CreateAiBackend` works
- `Placement` works
- `Status` can be reported
- read-model `total_count` and pagination are correct

### Phase 2

Implement the Spearlet assignment controller and remote driver.

Deliverables:

- `assignment_controller`
- `remote_driver`
- `status_reporter`

Acceptance:

- remote backends can become ready on specific nodes through placements

### Phase 3

Implement local drivers.

Deliverables:

- `llamacpp_driver`
- `vllm_driver`

Acceptance:

- local backends can start and become ready on specific nodes through placements

### Phase 4

Launch the new Web Admin pages.

Deliverables:

- `AI Backends`
- `Placements`
- `Node Status`
- read-only `AI Models`

### Phase 5

Remove the old model.

Delete:

- `admin_backends.rs`
- `model_deployments.rs`
- `backend_assignment_controller.rs`
- the old local controller main path
- `AI Models` control-semantics logic

## Current Implementation Status

At the current implementation stage, the unified AI backend control plane has reached its first end-to-end usable cut, including:

- New proto files:
  - `proto/sms/ai_backend.proto`
  - `proto/sms/ai_backend_placement.proto`
  - `proto/sms/ai_backend_status.proto`
- New SMS modules:
  - `src/sms/ai_backends/model.rs`
  - `src/sms/ai_backends/proto_conv.rs`
  - `src/sms/ai_backends/repository.rs`
  - `src/sms/ai_backends/repository_kv.rs`
  - `src/sms/ai_backends/service.rs`
  - `src/sms/ai_backends/placement_service.rs`
  - `src/sms/ai_backends/status_service.rs`
  - `src/sms/ai_backends/read_model.rs`
  - `src/sms/ai_backends/validator.rs`
- `build.rs` integration for the new proto generation
- `src/sms/mod.rs` export for the `ai_backends` module
- New SMS-side gRPC service integration:
  - `proto/sms/ai_backend_control_plane.proto`
  - `src/sms/ai_backend_rpc.rs`
  - `src/sms/grpc_server.rs`
- New HTTP/Web Admin API integration:
  - `src/sms/web_admin/router.rs`
  - `src/sms/web_admin.rs`
  - `src/sms/gateway.rs`
- New Web Admin frontend integration:
  - `web-admin/src/api/ai-backends.ts`
  - `web-admin/src/features/ai-backends/*`
  - `web-admin/src/app/AppShell.tsx`
- New Spearlet assignment-controller integration:
  - `src/spearlet/ai/backend_assignment_controller.rs`
  - `src/apps/spearlet/main.rs`
  - `src/spearlet/execution/ai/router/mod.rs`
  - `src/spearlet/backend_reporter.rs`
  - The controller now starts whenever Spearlet has an SMS connection and directly owns remote backend reconciliation

The current implementation boundary is:

- Completed:
  - UUID-based typed models for backend / placement / status
  - KV primary-record and secondary-index repository
  - service support for backend CRUD, placement upsert, and status reporting
  - `AiModelView` aggregation and read model
  - proto/domain conversion
  - SMS-side gRPC control-plane service
  - HTTP/Web Admin control-plane API
  - Web Admin frontend list/detail pages, server-side pagination/filtering, and placement/status queries
  - Spearlet-side assignment-driven remote backend materialization and node-status reporting
  - Initial `llamacpp` local-assignment integration on the Spearlet side
  - Explicit local-provider routing on the Spearlet side, including `vllm` placeholder status semantics
  - `vllm` now supports an `external_endpoint` integration mode, so an existing node-local vLLM service can participate in the unified control plane
  - `llamacpp` intermediate lifecycle status reporting on the Spearlet side (`reconciling -> ready/error`)
  - The legacy `remote_backend_sync` and legacy `LocalModelController` runtime implementations have been removed from the node side
  - Node-side AI backend lifecycle is now owned by the assignment controller
  - Spearlet now always starts in unified-only AI control-plane mode
  - monitoring now exposes dynamic-backend source snapshots and duplicate-name conflict detection for the unified controller
  - node-side AI backend lifecycle is now fully driven by the unified assignment controller
  - Web Admin `AI Models` now reads from the unified read model (`ai-model-views`) and is read-only
  - legacy Web Admin endpoints `/admin/api/ai-models`, `/admin/api/nodes/{uuid}/ai-models*`, and `/admin/api/ai/remote-backends*` are now removed
  - legacy SMS gRPC services `AdminAiConfigService` and `ModelDeploymentRegistryService` are now removed
  - a breaking cut-over is now accepted: legacy `admin_backends` / `model_deployments` data is not migrated, and upgrades only recognize unified `ai_backends`
  - legacy `admin_backends.rs`, `model_deployments` helpers, and their proto files are removed from active code
  - `vllm` now has a dedicated skeleton module at `src/spearlet/local_models/vllm.rs`
  - focused unit tests
- Not completed yet:
  - full convergence of `vllm` and other local-provider assignments into provider drivers

This means the codebase is now ready to continue into `Phase 1`, but it does not replace the current production control path yet.

## Testing Strategy

### SMS

- backend CRUD unit tests
- placement CRUD unit tests
- status reporting unit tests
- stale status cleanup unit tests
- node deletion cascade cleanup unit tests
- `AiModelView` aggregation and pagination unit tests

### Spearlet

- assignment controller idempotency tests
- remote driver reconcile/disable tests
- local driver reconcile/disable tests
- status reporter generation alignment tests

### E2E

- create remote backend + placement -> ready
- disable backend -> disabled
- delete placement -> node no longer serves the backend
- create local backend + placement -> process up + ready
- multiple `backend_id` values with the same provider/model -> `AI Models` aggregates correctly while `AI Backends` remains split by identity

## Risks and Open Questions

### 1. Is `BackendSpec` Sufficient as the Unified Execution Description?

It appears sufficient right now, but if local drivers require more provider-specific parameters, we may need to:

- extend `metadata`
- or add `driver_config` outside `spec`

### 2. Does Placement Need Node Selectors?

The first phase should use `node_uuid`-level placement to avoid premature complexity.

### 3. Does Node Status Need Event Sourcing?

The first phase should persist the latest status directly.

If auditing and replay become necessary later, add an event log at that point.

### 4. Should the `AI Models` Page Remain?

Yes, but it should stay strictly read-only and must no longer carry destructive control semantics.

## Recommended Decisions

The following two hard rules should be confirmed immediately:

1. `backend_id` is the only resource identity, and `provider/model/name` must never again act as a primary key
2. `AI Models` must remain a read model only, and must no longer carry delete / enable / disable semantics

If these two decisions are accepted, the implementation path becomes very clear.

## Related Implementation References

- Current unified backend persistence: `src/sms/ai_backends/repository_kv.rs`
- Current `AI Models` read-model aggregation: `src/sms/ai_backends/read_model.rs`
- Current AI backend gRPC entrypoint: `src/sms/ai_backend_rpc.rs`
- Current remote and local backend controller: `src/spearlet/ai/backend_assignment_controller.rs`
- Current local-provider support: `src/spearlet/local_models/provider.rs`
- Existing backend reporting: `src/spearlet/backend_reporter.rs`
