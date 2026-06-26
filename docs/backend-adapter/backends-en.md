# Backend Feature Pruning, Registry/Discovery, and Configuration

This document focuses on how “compiled and enabled” backends are discovered and participate in routing.

## 1. Compile-time pruning (Cargo features)

Use one Cargo feature per backend and mark heavy dependencies as `optional`:

- `backend-openai` (OpenAI-compatible HTTP)
- `backend-azure-openai`
- `backend-vllm`
- `backend-openai-realtime` (WebSocket)
- `backend-stub`

The registry builder registers backends behind `#[cfg(feature = "backend-xxx")]`; disabled backends do not compile/link.

## 2. BackendKind vs BackendInstance

Split into two layers:

- `BackendKind`: implementation type (openai_chat_completion/azure/vllm/realtime...)
- `BackendInstance`: concrete endpoint (base_url, region, weight, priority, capabilities, limits)

The router selects instances.

## 3. Registry and CapabilityIndex

- `BackendRegistry`: holds enabled instances, their capabilities, weights, and health handles
- `CapabilityIndex`: derived index (e.g., `Operation -> candidates[]`)

Legacy alignment: `GetAPIEndpointInfo` filters endpoints by env keys (`legacy/spearlet/core/models.go`); the new design centralizes this in registry construction and discovery.

## 4. Discovery surfaces

“Discovery” here means exposing an observable view of the in-process registry. It does not mean backends must discover each other via network calls.

- In-process: router/adapters read `BackendRegistry` via normal function calls (default path; no HTTP/gRPC required).
- External: optionally expose HTTP/gRPC endpoints for ops/debug/UI/automation to inspect “compiled + configured + currently healthy” backends and capabilities.

Two recommended surfaces:

1) Control-plane (HTTP/gRPC)
- `GET /api/v1/backends`
- `GET /api/v1/capabilities`

2) Task-side adaptation (optional)
- a hostcall control command like `GET_CAPABILITIES` returning JSON

## 5. Configuration model (example)

TOML (matches current `spearlet` config schema):

```toml
[spearlet.ai]
default_policy = "weighted_random"

[[spearlet.ai.credentials]]
name = "openai_chat"
kind = "env"
api_key_env = "OPENAI_CHAT_API_KEY"

[[spearlet.ai.credentials]]
name = "openai_realtime"
kind = "env"
api_key_env = "OPENAI_REALTIME_API_KEY"

[[spearlet.ai.backends]]
name = "openai-us"
kind = "openai_chat_completion"
base_url = "https://api.openai.com/v1"
hosting = "remote"
model = "gpt-4o-mini"
credential_ref = "openai_chat"
weight = 80
priority = 10
ops = ["chat_completions", "text_to_speech"]
features = ["supports_stream", "supports_tools", "supports_json_schema"]
transports = ["http"]

[[spearlet.ai.backends]]
name = "openai-realtime"
kind = "openai_realtime_ws"
base_url = "https://api.openai.com/v1"
hosting = "remote"
credential_ref = "openai_realtime"
weight = 100
priority = 20
ops = ["speech_to_text"]
features = ["supports_bidi_stream", "supports_audio_input", "supports_audio_output"]
transports = ["websocket"]
```

### 5.1 Model binding and “route by model”

If some backend instances declare `model` (for example, one Ollama backend per imported model), routing can select instances using only the request `model`:

- when the candidate set contains any `backend.model != None`, further filter by `backend.model == request.model`
- guests don’t need to set an explicit `backend` name; setting `model` is sufficient

Recommended: manage API keys centrally in `spearlet.ai.credentials[]` and reference them from backends via `credential_ref`.

Hosting:

- `hosting` is required for every configured backend and must be `local` or `remote`.

Detailed design and implementation notes: [llm-credentials-implementation-en.md](../implementation/llm-credentials-implementation-en.md)

## 6. Secrets and network policy

- `ai.credentials[].api_key_env`, `ai.backends[].credential_ref`, and `base_url` must be host-configured; WASM cannot inject them.
- Host-config allowlist/denylist is authoritative; request-side hints can only restrict further.

### 6.1 API key storage (recommended)

Store only the “environment variable name” in config; do not store plaintext keys in config files.

- In `[[spearlet.ai.credentials]]`, set `api_key_env = "OPENAI_API_KEY"`
- In `[[spearlet.ai.backends]]`, set `credential_ref = "<credential_name>"`
- Inject `OPENAI_API_KEY=...` into the spearlet process environment at startup

Benefits:

- prevents keys from entering the repo, config distribution, or logs
- enables per-instance/per-node keys and straightforward rotation

### 6.2 Reading and using the key (host-side)

In the current Rust codebase:

- resolve `api_key_env` via `credential_ref`
- read the env var value from the host process environment into `RuntimeConfig.global_environment`
- the registry uses that value to construct backend adapters
- adapters attach it as an HTTP header (e.g., `Authorization: Bearer <key>`)
- never log or return the key (including in error messages and `raw` payloads)

### 6.3 Missing key behavior (current behavior)

- If `credential_ref` is set (non-empty) and the credential cannot be found or the env var is not set:
  - treat the backend instance as unavailable (filter from candidates)
- If `credential_ref` is not set:
  - treat the backend instance as “no-auth” (no API key header). This is useful for OpenAI-compatible proxies that do not require keys.
- For external discovery:
  - only expose `credential_ref` (and optionally the env var name), never the value

### 6.4 Rotation and multiple keys

- Rotation: update the process env and do a rolling restart (MVP); add hot reload later if needed.
- Multiple keys: allow different `credential_ref` per backend instance.

### 6.5 Best practices for organizing multiple API keys

#### 6.5.1 Naming and mapping

- Name env vars by provider/region/purpose/instance to avoid accidental reuse:
  - e.g., `OPENAI_API_KEY_US_PRIMARY`, `OPENAI_API_KEY_US_FALLBACK`, `AZURE_OPENAI_KEY_EASTUS`, `VLLM_TOKEN_CLUSTER_A`
- Config references env var names only: bind one `credential_ref` per `BackendInstance` for traceability, rotation, and auditing.

#### 6.5.2 Multiple keys for the same backend instance (key pool)

If a single endpoint needs multiple keys (quota split, rate-limit sharding, gradual rollout/AB), introduce a “key pool”:

- Config (suggested): `credential_refs = ["openai_key_us_primary", "openai_key_us_2", ...]`
- Selection policies:
  - `round_robin`: spread QPS evenly
  - `random`: simplest
  - `least_errors`: more robust against bans/invalid keys (requires error counters)
- Failure fallback: on `401/403/429`, switch keys and apply short-term backoff/circuiting per key.

MVP can start with “one instance one key”; key pools are a Phase 4+ enhancement.

#### 6.5.3 Separation of duties and least privilege

- Do not share keys across providers/projects/permission domains; different backends should not reuse the same key.
- Bind key usage to operations where possible (e.g., a key only for `embeddings`) and restrict routing accordingly.

#### 6.5.4 Performance and operability

- Avoid expensive secret resolution per request (e.g., calling an external secret manager); cache resolved keys in-process.
- Optionally read and cache keys at adapter initialization time (if you accept “rotation requires restart”).

#### 6.5.5 Deployment notes (Kubernetes)

- Inject env vars via Kubernetes Secrets (`envFrom` / `valueFrom.secretKeyRef`) and constrain RBAC.
- External discovery/APIs must only expose `credential_ref` (or env var names), never values.

### 6.6 Working with SMS Web Admin (recommended)

SMS Web Admin can support an “API key configuration” UI, but best practice is to make it “secret reference management”, not plaintext key entry/storage.

Recommended split:

- Web Admin manages:
  - backend instance configuration (`base_url`, weights, capabilities, `credential_ref`, etc.)
  - secret references (credential names + env var names, or external secret-manager reference IDs)
- Web Admin does not manage:
  - plaintext key values (never stored in SMS DB, never logged, never returned via APIs)

How it works with spearlet:

- Inject env vars at spearlet startup via your deployment system (K8s Secrets, Vault Agent, systemd drop-in, etc.)
- Spearlet reads env vars at startup, builds a registry, and uses resolved keys to sign requests
- SMS Web Admin can provide validation/observability:
  - only report “present/usable” (e.g., spearlet heartbeat `health_info` reports `HAS_ENV:OPENAI_API_KEY_US_PRIMARY=true`)
  - UI can flag missing secrets on certain nodes without revealing values

Is this a good organization pattern:

- Yes (recommended): UI manages “mapping/references”, deployment manages “secret values”. This keeps clear security boundaries and supports auditing/rotation.
- Not recommended (unless you have a full security program): storing plaintext keys in Web Admin/SMS. Without KMS encryption, auditing, fine-grained RBAC, rotation, and incident response, SMS becomes a high-risk secret vault.
