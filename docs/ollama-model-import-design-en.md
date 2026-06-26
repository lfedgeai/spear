# Ollama Serving Models Auto-Import (Config & Detailed Design)

## Background

Today, Spearlet AI backends are sourced from static config `spearlet.ai.backends[]` and are materialized into an in-process `BackendRegistry` once at startup by [builder.rs](../src/spearlet/execution/ai/router/builder.rs). This has two limitations:

- Spearlet cannot dynamically generate routable backend instances based on **models currently being served by Ollama** on a node.
- Web Admin “Backends” can only reflect static configuration (or fixed local entries) and does not show Ollama runtime availability.

You want a config knob to decide whether Spearlet should “import” the models Ollama is currently serving and expose them as available backends.

Related docs:

- Backend registry/discovery and config: [backend-adapter/backends-en.md](./backend-adapter/backends-en.md)
- LLM `credential_ref` rules: [implementation/llm-credentials-implementation-en.md](./implementation/llm-credentials-implementation-en.md)
- Backend availability aggregation (node push): [backend-registry-api-design-en.md](./backend-registry-api-design-en.md)

## Goals

- Provide an industry-standard config option (default off) to enable Ollama serving-model import.
- Represent each imported model as a separate backend instance (predictable naming).
- Ensure robustness: rate limiting, caching, backoff, failure isolation, observability.
- Ensure security: no secret leakage; prevent SSRF by safe defaults.

## Non-goals

- Importing all installed models by default (can be a future extension).
- Editing/pushing Ollama config from Web Admin.
- Cross-node model scheduling and migration.

## Best Practices (Summary)

- **Explicit opt-in**: auto-import/discovery must be disabled by default.
- **Bounded scope**: require allow/deny filters and a `max_models` limit.
- **Stable naming**: deterministic naming that avoids collisions with user-defined backends.
- **Separate static vs dynamic sources**: merge for routing, but preserve provenance.
- **Failure isolation**: discovery errors must not break existing static routing.
- **Secure defaults**: only localhost by default; remote requires explicit config.

## Configuration Design

Add a new `discovery` section under `SpearletConfig.ai` (the current config uses `deny_unknown_fields`, so the schema must be extended explicitly):

```toml
[spearlet.ai.discovery.ollama]
enabled = false

# Ollama HTTP endpoint
base_url = "http://127.0.0.1:11434"

# Import scope:
# - serving: only models currently being served (recommended, matches requirement)
# - installed: all installed models (future extension)
scope = "serving"

# Refresh cadence (seconds). 0 means import once at startup.
refresh_interval_secs = 15

# Security: allow non-localhost base_url (default false)
allow_remote = false

# Upper bound on imported models
max_models = 20

# Naming
name_prefix = "ollama/"

# Conflict policy:
# - skip: if name already exists in static config, skip import (recommended)
# - override: override static config (not recommended)
name_conflict = "skip"

# allow/deny support glob patterns
allow_models = ["*"]
deny_models = []

# Default capabilities for imported backends
ops = ["chat_completions"]
features = ["supports_stream"]
transports = ["http"]

# Default routing weights
weight = 10
priority = -10

# Model binding strategy
# - backend_name: backend is just a label; the request's model decides
# - fixed_default_model: each imported backend is bound to one model (recommended)
binding_mode = "fixed_default_model"
```

## Semantics

### Data sources

For “models currently being served”, query Ollama `GET /api/ps`.

- `scope=serving`: import models from `/api/ps`.
- `scope=installed` (future): import models from `GET /api/tags`.

### Mapping to backend instances

Each imported model becomes one backend instance:

- `name = name_prefix + sanitize(model_name)`
  - `sanitize` must produce URL-safe, stable identifiers.
- `kind = "ollama_chat"` (recommended: introduce a new backend kind that requires **no API key**)
- `base_url = discovery.ollama.base_url`
- `ops/features/transports/weight/priority` come from discovery defaults

With `binding_mode=fixed_default_model`:

- bind a `default_model = model_name` to the backend instance
- the adapter always calls Ollama using that model (does not rely on session model)

This is recommended because it makes “imported model == routable backend” true.

### Conflict handling

When an imported backend name collides with a statically configured backend name:

- default `name_conflict=skip`: skip import and log a structured warning.
- avoid `override`: it can cause surprising routing changes.

### Refresh & stability

When `refresh_interval_secs > 0`:

- periodically refresh `/api/ps`.
- if a model disappears from serving list:
  - remove it from the imported set
  - trigger a registry rebuild (see implementation)

Optionally add a future `min_stable_cycles` to reduce flapping.

### Failure isolation

- Ollama unreachable/timeout/5xx:
  - keep last successful imported snapshot (stale) and emit metrics
  - do not affect static backends
- Parse errors:
  - treat as a failed refresh, keep last snapshot

## Implementation (Architecturally Clean)

### 1) OllamaDiscoveryService

Add a dedicated periodic discovery service, similar to other background services:

- queries Ollama HTTP endpoints
- produces a discovered backend set (as backend specs)
- stores it in `Arc<RwLock<DiscoveredBackends>>`

### 2) Registry merge and hot update

Currently [builder.rs](../src/spearlet/execution/ai/router/builder.rs) builds a registry once. To support dynamic serving-model changes, introduce a `RegistryHandle`:

- `ArcSwap<BackendRegistry>` or `RwLock<BackendRegistry>`
- the router reads the current registry from the handle for each selection

Update path:

1. Build static instances from config.
2. Build dynamic instances from discovery.
3. Merge (dedupe by name + conflict policy).
4. Atomically replace the registry handle.

### 3) Web Admin integration (node push)

If node push reporting is enabled, report the merged registry view so Web Admin naturally shows:

- static backends
- imported Ollama backends

Use `status_reason` for diagnostics such as:

- `ollama: unreachable`
- `ollama: filtered by denylist`
- `ollama: conflict name, skipped`

### 4) Security (SSRF prevention)

Defaults:

- `allow_remote=false`
- only allow `localhost/127.0.0.1/[::1]` hosts

When `allow_remote=true`:

- still recommend CIDR deny for link-local/metadata ranges
- restrict schemes to `http/https`

## Observability

Recommended metrics:

- `ollama_discovery_refresh_total{result=success|error}`
- `ollama_discovery_models_imported` (gauge)
- `ollama_discovery_last_success_timestamp`
- `ollama_discovery_snapshot_hash`

Structured logs:

- refresh start/end + duration
- error classification (connect/timeout/parse)
- imported/removed counts

## Test Plan

- Config parsing tests for `spearlet.ai.discovery.ollama`.
- Unit tests for sanitize/filter/conflict behavior.
- Integration test with a mocked Ollama `/api/ps` to validate dynamic updates.
- Web Admin: `/admin/api/backends` should reflect imported backends via node snapshots.

## Rollout Plan

- Phase 0: import once at startup (`refresh_interval_secs=0`) to validate the full loop.
- Phase 1: periodic refresh + registry hot updates.
- Phase 2: installed models support (`/api/tags`) and richer capabilities (embeddings, etc.).
