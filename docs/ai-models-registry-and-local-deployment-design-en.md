# AI Models (UI Naming) and Local Model Deployment (Local Providers) Design

## 1. Background and Goals

The system currently uses “Backends” as the primary runtime and Web Admin abstraction for routing and observability: Spearlet builds an in-process registry from `spearlet.ai.backends[]` (plus optional discovery, e.g. Ollama import), and periodically reports per-node backend snapshots to SMS (see [backend_registry.proto](../proto/sms/backend_registry.proto) and the Spearlet reporter).

You want to:

- Rename “Backends” to “AI Models” in the UI, and present a hierarchy under “AI Models”:
  - Remote: OpenAI, etc.
  - Local: Ollama, llama.cpp, vLLM, etc.
- Clearly distinguish whether a backend is remote or local (and decide if a type field is needed).
- Under “Local”, allow Web Admin to select a node and create a provider-specific model (ollama / llamacpp / vllm), making it observable and routable.

This document proposes an industry-aligned, best-practice architecture, including protocol (proto) sketches and function-level boundaries for implementation review.

Non-goals (you don’t have to do everything at once):

- Not forcing all local providers to support “auto install + fully managed lifecycle” immediately.
- Not requiring Kubernetes CRDs (but we align with the same controller patterns).

## 2. Key Takeaways (TL;DR)

- Renaming the UI tab to “AI Models” is correct because users think in terms of “which model” rather than “which backend connector”.
- Internally, keep “Backend” as the execution/routing unit: a backend is an endpoint instance; a model is a semantic dimension and/or request parameter. “AI Models” is a model-centric aggregated view of backends.
- Should backends have a remote/local type?
  - Recommended: add an explicit field (e.g. `hosting` / `location`) to avoid brittle inference from `kind` or `base_url`.
  - Maintain backward compatibility: when missing, the aggregation/UI can fall back to inference rules.
- For “create local provider model on a node”: use a **desired state registry + node-side reconcile loop + status reporting** controller design (Kubernetes-style), not “Web Admin executes commands on nodes”.

## 3. Terminology and Layers (recommended)

To avoid ambiguity between “backend / provider / model”, fix the following layers:

### 3.1 Provider (engine/vendor)

Provider describes the “model serving implementation form”, e.g.:

- Remote providers: OpenAI, Azure OpenAI, OpenAI-compatible SaaS
- Local providers: Ollama, llama.cpp server, vLLM server

It answers: **who serves inference and via what interface**.

### 3.2 Model (model identifier)

Model is the user-facing model name/version, e.g.:

- `gpt-4o-mini`
- `llama3:8b`
- `Qwen2.5-7B-Instruct`

It answers: **what model the inference targets**.

### 3.3 Backend (smallest routing/execution unit)

Backend is a concrete callable endpoint instance, typically including:

- `kind` (adapter kind, e.g. `openai_chat_completion`, `ollama_chat`)
- `base_url` (where to send requests)
- `credential_ref` (auth reference, if needed)
- `capabilities` (ops/features/transports/weight/priority, etc.)
- `model` (optional: fixed model binding; if empty, model is dynamic)

It answers: **which endpoint we call, what it can do, and how it is ranked**.

### 3.4 AI Model (UI object)

The UI “AI Models” should be a model-centric aggregated view:

- An AI Model is uniquely identified by `(provider, model, hosting)` (recommended).
- One AI Model can map to multiple backends (same model deployed on multiple nodes, or multiple remote endpoints).

## 4. UI Information Architecture (Backends → AI Models)

### 4.1 Top-level naming

Rename the “Backends” tab to “AI Models”. Internally, keep an “Instances / Backends” sub-view for ops/debug.

Recommended hierarchy:

- AI Models
  - Remote
    - OpenAI
    - OpenAI-compatible
    - …
  - Local
    - Ollama
    - llama.cpp
    - vLLM
    - …

### 4.2 Main list view (AI Models)

Each row is an aggregated AI Model:

- Provider (OpenAI/Ollama/vLLM/…)
- Model (model name; if backend is not fixed-bound, display “(dynamic)” or aggregate at provider level)
- Hosting (Remote/Local)
- Operations (chat_completions, speech_to_text, embeddings, ...; only show currently implemented or registered capabilities)
- Available nodes / Total nodes (mainly relevant for Local or multi-endpoint Remote)
- Status (aggregate: e.g. available/partial/unavailable)

Details drawer/page:

- Show backend instances grouped by node: `base_url`, `weight/priority`, `status_reason`, etc.

### 4.3 Create entry under Local

Local → Provider (e.g. Ollama) → “Create model on node”

The correct semantics is: **create/update a Model Deployment desired state record**, and let the node reconcile it.

## 5. Data Model Design

### 5.1 Minimal fields to extend backend snapshots (recommended)

The current backend snapshot model (see [backend_registry.proto](../proto/sms/backend_registry.proto)) lacks two critical fields for model-centric UI: `model` and “remote/local”.

Extend it in a **backward compatible** way (proto3 new fields are compatible):

#### New enum: BackendHosting (sketch)

```proto
enum BackendHosting {
  BACKEND_HOSTING_UNSPECIFIED = 0;
  BACKEND_HOSTING_REMOTE = 1;     // SaaS / remote cluster
  BACKEND_HOSTING_NODE_LOCAL = 2; // node-local / loopback
}
```

#### Extend BackendInfo (sketch)

```proto
message BackendInfo {
  // existing fields...

  // provider identifier for UI grouping
  string provider = 11;

  // fixed bound model name if any
  string model = 12;

  // explicit hosting type (remote vs local)
  BackendHosting hosting = 13;
}
```

Compatibility fallback (when fields are missing):

- Infer hosting by `kind` and/or loopback `base_url`, otherwise show as “Unknown”.

Why not rely on inference only:

- An Ollama endpoint may be `http://10.x.x.x:11434` (not loopback), which may be “cluster-local” or “remote” depending on topology.
- `kind` is adapter type, not a deployment signal.

### 5.2 UI aggregation object: AiModelInfo (served by SMS)

To keep the frontend simple, expose a model-centric API from the SMS Web Admin BFF:

```json
{
  "provider": "ollama",
  "model": "llama3:8b",
  "hosting": "local",
  "operations": ["chat_completions"],
  "features": ["supports_stream"],
  "transports": ["http"],
  "available_nodes": 3,
  "total_nodes": 5,
  "instances": [
    {
      "node_uuid": "…",
      "backend_name": "ollama/llama3-8b",
      "kind": "ollama_chat",
      "base_url": "http://127.0.0.1:11434",
      "status": "available",
      "status_reason": ""
    }
  ]
}
```

Implementation source:

- Reuse `BackendRegistryService.ListNodeBackendSnapshots()` and aggregate in SMS:
  - key = `(provider, model, hosting)`; if `model` is missing, downgrade to `(provider, "(dynamic)", hosting)`.

## 6. Best-Practice Control Plane for “Create Local Model”

### 6.1 Why “desired state + reconcile”

If Web Admin directly triggers on-node commands (SSH/remote exec), you immediately get:

- Security risk: a broad remote execution surface.
- Poor auditability: hard to trace who changed what.
- Poor resilience: node reboot/process crash won’t converge back to desired state.

So the recommended industry pattern is: **SMS stores desired state; Spearlet (like kubelet) runs a reconcile controller**.

### 6.2 New concept: ModelDeployment (local model deployment record)

A ModelDeployment means “ensure provider X with model Y is available on target nodes”, including:

- `deployment_id` (stable id)
- `target` (node_uuid or node selector)
- `provider` (ollama/llamacpp/vllm)
- `model` (model identifier)
- `serving` (port/concurrency/GPU/ctx, provider-specific)
- `lifecycle` (create/update/delete semantics)

### 6.3 Protocol: ModelDeploymentRegistryService (new proto suggested)

Mirror the “revision + watch” pattern used in [mcp_registry.proto](../proto/sms/mcp_registry.proto).

#### Proto sketch

```proto
syntax = "proto3";
package sms;

enum ModelDeploymentPhase {
  MODEL_DEPLOYMENT_PHASE_UNSPECIFIED = 0;
  MODEL_DEPLOYMENT_PHASE_PENDING = 1;
  MODEL_DEPLOYMENT_PHASE_PULLING = 2;
  MODEL_DEPLOYMENT_PHASE_STARTING = 3;
  MODEL_DEPLOYMENT_PHASE_READY = 4;
  MODEL_DEPLOYMENT_PHASE_FAILED = 5;
  MODEL_DEPLOYMENT_PHASE_DELETING = 6;
}

message ModelDeploymentSpec {
  string target_node_uuid = 1; // MVP: single-node binding
  string provider = 2;         // ollama | llamacpp | vllm
  string model = 3;            // model identifier
  map<string, string> params = 4; // provider-specific; must be validated/allowlisted
}

message ModelDeploymentStatus {
  ModelDeploymentPhase phase = 1;
  string message = 2;
  int64 updated_at_ms = 3;
}

message ModelDeploymentRecord {
  string deployment_id = 1;
  uint64 revision = 2;
  int64 created_at_ms = 3;
  int64 updated_at_ms = 4;
  ModelDeploymentSpec spec = 5;
  ModelDeploymentStatus status = 6;
}

message ListModelDeploymentsRequest {
  uint32 limit = 1;
  uint32 offset = 2;
  string target_node_uuid = 3; // optional
  string provider = 4;         // optional
}

message ListModelDeploymentsResponse {
  uint64 revision = 1;
  repeated ModelDeploymentRecord records = 2;
  uint32 total_count = 3;
}

message WatchModelDeploymentsRequest {
  uint64 since_revision = 1;
  string target_node_uuid = 2; // spearlet watches only itself
}

message ModelDeploymentEvent {
  uint64 revision = 1;
  repeated string upserts = 2;
  repeated string deletes = 3;
}

message WatchModelDeploymentsResponse { ModelDeploymentEvent event = 1; }

message UpsertModelDeploymentRequest { ModelDeploymentRecord record = 1; }
message UpsertModelDeploymentResponse { uint64 revision = 1; }
message DeleteModelDeploymentRequest { string deployment_id = 1; }
message DeleteModelDeploymentResponse { uint64 revision = 1; }

message ReportModelDeploymentStatusRequest {
  string deployment_id = 1;
  string node_uuid = 2;
  uint64 observed_revision = 3;
  ModelDeploymentStatus status = 4;
}
message ReportModelDeploymentStatusResponse { bool success = 1; }

service ModelDeploymentRegistryService {
  rpc ListModelDeployments(ListModelDeploymentsRequest) returns (ListModelDeploymentsResponse);
  rpc WatchModelDeployments(WatchModelDeploymentsRequest) returns (stream WatchModelDeploymentsResponse);
  rpc UpsertModelDeployment(UpsertModelDeploymentRequest) returns (UpsertModelDeploymentResponse);
  rpc DeleteModelDeployment(DeleteModelDeploymentRequest) returns (DeleteModelDeploymentResponse);
  rpc ReportModelDeploymentStatus(ReportModelDeploymentStatusRequest) returns (ReportModelDeploymentStatusResponse);
}
```

Notes:

- Start with `target_node_uuid` (single-node) to avoid scheduling complexity; extend later to selectors.
- `params` must be strictly validated in code (allowlist keys + formats) to avoid turning this into a remote-exec channel.
- `ReportModelDeploymentStatus` is for UX (“pulling/starting/failed”), while **routing truth** remains backend snapshots (avoid dual source of truth).

### 6.4 Spearlet node: LocalModelController (reconcile loop, historical design)

Note: this section documents a historical design. The node-side legacy
`LocalModelController` runtime has since been removed, and the current
implementation is centered on the unified assignment controller.

Add a long-running controller to Spearlet:

- watch model deployments relevant to the node (gRPC stream)
- reconcile each deployment into actual state
- register callable endpoints as backends and rely on existing backend reporting to SMS
- report deployment status (phase/message)

#### Core Rust trait boundaries (sketch)

```rust
pub trait LocalModelDriver: Send + Sync {
    fn provider(&self) -> &'static str;

    async fn ensure_model_present(
        &self,
        model: &str,
        params: &std::collections::HashMap<String, String>,
    ) -> Result<(), LocalModelError>;

    async fn ensure_serving(
        &self,
        model: &str,
        params: &std::collections::HashMap<String, String>,
    ) -> Result<ServingEndpoint, LocalModelError>;

    async fn stop_serving(
        &self,
        model: &str,
        params: &std::collections::HashMap<String, String>,
    ) -> Result<(), LocalModelError>;
}

pub struct ServingEndpoint {
    pub base_url: String,
    pub kind: String,
    pub fixed_model: Option<String>,
    pub transports: Vec<String>,
}
```

Driver implementation guidance:

- OllamaDriver (recommended MVP):
  - assumes Ollama daemon already runs on the node
  - `ensure_model_present`: trigger pull via Ollama API (or `ollama pull` only with stricter security)
  - `ensure_serving`: return `base_url=http://127.0.0.1:11434`
- llama.cpp / vLLM (phase-based):
  - Phase 1: node-managed process (start a local server and probe readiness)
  - For llama.cpp (llamacpp), prefer direct GGUF download:
    - params: `model_url` (http/https, direct `.gguf` URL), or `model_path` (an existing local file path)
    - local cache: if `model_path` already exists, skip downloading
    - do not rely on `llama-cli --hf-repo/--hf-file` for downloading
  - Phase 2: managed containers with strict image allowlists and resource isolation

#### Suggested controller phases

- Pending → Pulling → Starting → Ready
- Failed (retryable) and Deleting

### 6.5 Relationship to existing Ollama discovery

Existing Ollama discovery (see [ollama-discovery-zh.md](./ollama-discovery-zh.md)) is “runtime discovery (read-only)”: it imports models already present/serving into backends.

ModelDeployment is “control-plane desired state (write/control)”: it ensures a model is present/usable on a node.

They can coexist:

- discovery: auto-expose what already exists
- deployment: declaratively create/manage what you want

Naming best practice:

- Use a distinct prefix for managed deployments (e.g. `managed/ollama/<model>`) to avoid name conflicts with discovery backends (e.g. `ollama/<model>`).

### 6.6 Concrete code boundaries (recommended)

This section maps the protocol/controller design to the current repository layout to reduce ambiguity during implementation.

#### SMS: ModelDeployment registry (storage + gRPC)

Follow the same “revision + watch” shape as the MCP registry (see [mcp_registry.proto](../proto/sms/mcp_registry.proto)). Suggested additions:

- Proto: `proto/sms/model_deployment_registry.proto` (new service + records)
- Watch infrastructure: reuse the shared watch hub in SMS, `RegistryWatchHub` in [registry_watch.rs](../src/sms/registry_watch.rs) (the MCP registry has been migrated to this implementation; see [service.rs](../src/sms/service.rs)).
  - Semantics: if `since_revision` is too old, return `FAILED_PRECONDITION("since_revision too old; resync required")`; if the consumer lags and the broadcast buffer overflows, return `ABORTED("watch lagged; resync required")`.
  - Client contract: on these errors, `List` a full snapshot and then resume `Watch` from the new revision.
- Rust store trait (sketch):

```rust
pub trait ModelDeploymentStore: Send + Sync {
    fn revision(&self) -> u64;

    fn list(
        &self,
        limit: u32,
        offset: u32,
        target_node_uuid: Option<&str>,
        provider: Option<&str>,
    ) -> Result<(Vec<ModelDeploymentRecord>, u32), StoreError>;

    fn get(&self, deployment_id: &str) -> Result<Option<ModelDeploymentRecord>, StoreError>;

    fn upsert(&self, record: ModelDeploymentRecord) -> Result<u64, StoreError>;

    fn delete(&self, deployment_id: &str) -> Result<u64, StoreError>;

    fn update_status(
        &self,
        deployment_id: &str,
        node_uuid: &str,
        observed_revision: u64,
        status: ModelDeploymentStatus,
    ) -> Result<u64, StoreError>;
}
```

Recommended implementation path:

- Start with in-memory for rapid iteration/tests, then persist via the existing KV abstraction (similar to other registries/stores).

gRPC service implementation boundaries (suggested):

- `ModelDeploymentRegistryServiceImpl::list_model_deployments(...)`
- `ModelDeploymentRegistryServiceImpl::watch_model_deployments(...)`
- `ModelDeploymentRegistryServiceImpl::upsert_model_deployment(...)`
- `ModelDeploymentRegistryServiceImpl::delete_model_deployment(...)`
- `ModelDeploymentRegistryServiceImpl::report_model_deployment_status(...)`

#### SMS: Web Admin BFF (`/admin/api`)

Add handlers in `src/sms/web_admin.rs` in the same style as existing backends/mcp endpoints:

- `list_ai_models(...)`: aggregate `BackendRegistryService.ListNodeBackendSnapshots` into `AiModelInfo[]`
- `get_ai_model_detail(...)`: return instances for a `(provider, model)`
- `create_node_ai_model_deployment(...)`: HTTP body → `UpsertModelDeploymentRequest`
- `delete_node_ai_model_deployment(...)`: delete a deployment
- `list_node_ai_model_deployments(...)`: list deployments + status for a node

#### Spearlet: LocalModelController (node-side reconcile loop, historical design)

Note: the module boundaries and functions below are kept only as a record of the
historical design and no longer match the current code layout.

Startup:

- Start only when Spearlet is connected to SMS (same lifecycle point as existing task subscriber / backend reporter).

Suggested modules:

- `src/spearlet/local_models/mod.rs`
- `src/spearlet/local_models/controller.rs`: watch + reconcile + status reporting
- `src/spearlet/local_models/driver.rs`: `LocalModelDriver` trait + shared error types
- `src/spearlet/local_models/drivers/ollama.rs`: OllamaDriver

Core controller functions (sketch):

```rust
impl LocalModelController {
    pub fn start(&self);

    async fn watch_loop(&self) -> Result<(), ControllerError>;

    async fn reconcile_one(&self, record: ModelDeploymentRecord) -> Result<(), ControllerError>;

    async fn apply_ready_backend(&self, endpoint: ServingEndpoint) -> Result<(), ControllerError>;

    async fn report_status(&self, deployment_id: &str, status: ModelDeploymentStatus);
}
```

Integration with backend registry:

- After a deployment becomes ready, produce an `AiBackendConfig` (or equivalent runtime backend spec) and inject it into the runtime registry. For long-term correctness, use a dynamic registry handle approach (see the registry hot-update notes in [ollama-model-import-design-zh.md](./ollama-model-import-design-zh.md)).

## 7. Web Admin API Design (BFF)

### 7.1 AI Models aggregation endpoints

- `GET /admin/api/ai-models`
  - returns `AiModelInfo[]` aggregated from backend snapshots
  - query: `hosting`, `provider`, `q`, `limit/offset`

- `GET /admin/api/ai-models/{provider}/{model}`
  - single model aggregated details (instances by node)

Compatibility:

- Keep existing `/admin/api/backends` as the low-level instances view/debug surface.

### 7.2 Local create/delete model deployments

- `POST /admin/api/nodes/{node_uuid}/ai-models`
  - body: `{ provider, model, params }`
  - behavior: upsert a ModelDeploymentRecord in SMS

- `DELETE /admin/api/nodes/{node_uuid}/ai-models/deployments/{deployment_id}`
  - behavior: delete deployment record; node reconciles stop/cleanup

- `GET /admin/api/nodes/{node_uuid}/ai-models/deployments`
  - behavior: list deployments and their status for the node

## 8. Security and Compliance (must-have)

### 8.1 SSRF prevention

Local providers often talk to node-local HTTP endpoints. Default must be safe:

- allow loopback only by default
- if remote is allowed, apply CIDR deny rules appropriate to your threat model

If you support downloading model files via URL (e.g. llamacpp `params.model_url`), apply SSRF protections there as well:

- allow `http/https` only and restrict to allowed domains/ranges (prefer an allowlist)
- block `localhost/127.0.0.1/[::1]`, link-local, private CIDRs, and cloud metadata addresses

### 8.2 Secrets must never appear in protocols/UI

Continue current best practice:

- only transmit `credential_ref` / env var names, never values
- never include secret values in `status_reason`

### 8.3 Minimize command execution surface

If you later support “pull/start processes automatically”, require:

- strict param validation and allowlists
- executable/image allowlists
- least-privilege runtime users
- audit logs for Web Admin actions

## 9. Practical Phased Rollout

### Phase A: UI semantics upgrade only (fastest loop)

- UI: Backends → AI Models (Remote/Local grouping; display only)
- SMS: add `/admin/api/ai-models` aggregation endpoint
- No Spearlet protocol changes yet: infer hosting/provider/model where possible

### Phase B: Fill protocol gaps (reduce inference)

- extend `BackendInfo` with `provider/model/hosting`
- populate these in Spearlet reporting based on `kind/model/base_url`

### Phase C: Local (Ollama) “create model” MVP (historical design)

- SMS: implement ModelDeploymentRegistryService + Web Admin entry
- Spearlet: LocalModelController + OllamaDriver (historical design; current node-side runtime has moved to the unified assignment controller path)
- Routing: after readiness, generate `managed/ollama/<model>` backend and report

### Phase D: Managed llama.cpp / vLLM (optional)

- endpoint registration first, managed process/container later
- strict allowlists and resource isolation to avoid general-purpose remote exec
