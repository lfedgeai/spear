# gRPC-based Candidate Filtering for Router (Design, Function/Interface Level)

This document specifies **Option C**: an **external gRPC decision service** that the Router calls **right before selecting** a backend instance, to perform request inspection and candidate filtering/scoring for operations like Chat Completions and ASR.

It is designed to fit the current layered path:

`Normalize -> CanonicalRequestEnvelope -> Router -> BackendAdapter`

See:
- `routing-en.md` for capability filtering and selection policies
- `implementation-plan-en.md` for file/function-level integration points

## 1. Goals / Non-goals

### Goals

1. **Pre-dispatch inspection**: detect/annotate request properties before forwarding (policy, safety, compliance, cost).
2. **Candidate filtering**: drop ineligible backends from a candidate set.
3. **Candidate scoring**: adjust preference (weights / priorities / scores) among remaining candidates.
4. **Strict reliability**: Router must remain available if the external service is down (fail-open by default).
5. **Deterministic contract**: stable proto API, predictable time budget, bounded payload size.

### Non-goals

- Performing the actual upstream invocation (still done by `BackendAdapter`).
- Long-running analysis or streaming inspection (must fit routing latency budget).
- Exposing secrets to the external decision service.

## 2. Placement in the Current Router Flow

Current routing is implemented in:
- `src/spearlet/execution/ai/router/mod.rs` (`Router::route`)
- `src/spearlet/execution/ai/router/registry.rs` (`BackendRegistry::candidates`)

The new gRPC step is inserted **after** hard capability filtering and request routing hints, **before** selection policy:

1. `candidates = registry.candidates(req)` (operation + required_features + transports)
2. Apply routing hints (backend/allowlist/denylist/model binding)
3. **Call external filter/scorer** (this design)
4. `selected = policy.select(req, candidates)`

## 3. gRPC Service Contract (proto, bidirectional stream)

### 3.1 Proto file and package

Proposed new proto file:

- `proto/spearlet/router_filter.proto`
- `package spearlet;`

This keeps the contract close to the Spearlet router use case (even if the server is deployed externally).

### 3.2 Service definition (Spearlet dials the filter server)

The current implementation treats Router Filter as a **server** (provided by SMS by default). Spearlet calls it as a gRPC client:

```proto
service RouterFilterService {
  rpc Filter(FilterRequest) returns (FilterResponse);
}
```

For the full schema, refer to `proto/spearlet/router_filter.proto`.

### 3.3 Optional payload forwarding

By default, Spearlet sends only size-bounded `RequestSignals` + `meta` and does not transmit raw request content.

When a policy must inspect content, Spearlet can optionally populate `FilterRequest.request_payload` (bounded by config):

- `content_fetch_enabled = true`
- `content_fetch_max_bytes` caps forwarded payload size (if exceeded, payload is not forwarded)

This keeps the connection direction one-way (Spearlet → Filter) while making raw-content access explicit and bounded.

## 4. Router-side Implementation Plan (function / variable level)

### 4.1 New module and main entry points (Rust)

Proposed new module (Router-side gRPC client hub integration):

- `src/spearlet/execution/ai/router/grpc_filter_stream.rs`

Key public types and functions:

```rust
pub struct RouterFilterStreamConfig {
    pub enabled: bool,
    pub addr: String,
    pub decision_timeout_ms: u64,
    pub fail_open: bool,
    pub max_candidates_sent: usize,
    pub max_debug_kv: usize,
    pub max_inflight_total: usize,
}

pub struct RouterFilterStreamHub {
    config: RouterFilterStreamConfig,
    // Background async worker for gRPC calls.
}

pub struct FilterTrace {
    pub decision_id: Option<String>,
    pub dropped: Vec<String>,
    pub weight_overrides: Vec<(String, u32)>,
    pub priority_overrides: Vec<(String, i32)>,
    pub reason_codes_by_candidate: std::collections::HashMap<String, Vec<String>>,
    pub final_action: Option<FinalActionTrace>,
}

pub struct FinalActionTrace {
    pub reject_request: bool,
    pub reject_code: Option<String>,
    pub force_backend: Option<String>,
}

impl RouterFilterStreamHub {
    pub fn try_filter_candidates_blocking(
        &self,
        req: &CanonicalRequestEnvelope,
        candidates: &mut Vec<&BackendInstance>,
        decision_timeout_ms: u64,
    ) -> Result<(FilterResponse, FilterTrace), CanonicalError>;
}
```

### 4.1.1 Transport: TCP (Filter exposes a gRPC port)

This design uses TCP (host:port):

- The filter server (provided by SMS by default) runs a gRPC server on a TCP port (typically reusing `sms.grpc.addr`).
- Spearlet dials that address as a gRPC client (defaults to `spearlet.sms_grpc_addr`, can be overridden by `router_grpc_filter_stream.addr`).

Address examples:

- `127.0.0.1:50051` (same-host loopback)
- `sms.internal:50051` (cross-host)

### 4.2 Router::route integration details

Minimal changes to `Router` (no new “big” abstraction required):

```rust
pub struct Router {
    registry: BackendRegistry,
    policy: SelectionPolicy,
    grpc_filter_stream: Option<std::sync::Arc<RouterFilterStreamHub>>,
}
```

New helper function in `ai/router/mod.rs`:

```rust
fn apply_grpc_filter(
    req: &CanonicalRequestEnvelope,
    candidates: &mut Vec<&BackendInstance>,
    hub: &RouterFilterStreamHub,
) -> Result<FilterTrace, CanonicalError>;
```

Key local variables inside `Router::route`:

- `let mut candidates: Vec<&BackendInstance> = self.registry.candidates(req);`
- `let hard_filtered_count: usize = candidates.len();`
- `let decision_budget_ms: u64 = clamp(req.timeout_ms, cfg.decision_timeout_ms);`
- `let filter_res: Result<FilterTrace, CanonicalError> = apply_grpc_filter(...);`
- `let candidate_count_after_filter: usize = candidates.len();`
- `let selected: &BackendInstance = self.policy.select(req, candidates)?;`

Error and fallback policy:

- If `hub.config.fail_open == true`:
  - No available connected agent / stream reset / local wait timeout => **no change** to `candidates`.
  - Router logs `filter_failed=true` and continues to `policy.select`.
- If `hub.config.fail_open == false`:
  - No agent / timeout / protocol error => `CanonicalError { code: "router_filter_unavailable", retryable: true/false }`.

FinalAction handling:

- `reject_request=true` => Router returns `CanonicalError { code = reject_code or "router_filter_rejected", message = reject_message, retryable=false }`
- `force_backend` => Router rewrites candidates to just that backend name, then continues with policy selection.

### 4.3 Response validation rules (must be enforced by Router)

To prevent the external service from expanding power beyond allowed constraints:

1. Any `CandidateDecision.name` not in the input candidates is ignored.
2. `force_backend` must match the current candidate set after hard constraints (backend/allowlist/denylist/model binding).
3. `weight_override` and `priority_override` are clamped to safe bounds:
   - `weight_override` in `[0, 10_000]`
   - `priority_override` in `[-1000, 1000]`
4. If all candidates are dropped:
   - If `final_action.reject_request=true`, Router returns rejection.
   - Else Router returns `no_candidate_backend` (same as current behavior).

## 5. Configuration (TOML)

Proposed config fields under `spearlet.ai`:

```toml
[spearlet.ai.router_grpc_filter_stream]
enabled = true
addr = "127.0.0.1:50052"
decision_timeout_ms = 5
fail_open = true
max_candidates_sent = 64
max_debug_kv = 32
max_inflight_total = 4096
per_agent_max_inflight = 512
```

Mapping to Rust:

- `SpearletConfig.ai.router_grpc_filter_stream: Option<RouterFilterStreamConfig>`

## 6. Operational Guidance (best practices)

### 6.1 Time budget

- For non-streaming requests, recommend `decision_timeout_ms <= 5ms` (local network) or `<= 10ms` (cross-host).
- For streaming (first token / first audio frame), enforce even tighter budgets (1–3ms) and prefer fail-open.

### 6.2 Idempotency and caching

- `FilterCandidatesRequest.request_id` is the idempotency key for retries.
- Router may keep a short-lived cache:
  - key: `(request_id, operation, requested_model, candidate_names_hash)`
  - value: `FilterCandidatesResponse`
  - ttl: `min(500ms, decision_timeout_ms * 100)`

### 6.3 Observability fields

Router logs should include (structured):

- `request_id`, `operation`, `candidate_count_before`, `candidate_count_after`
- `filter_decision_id`, `filter_elapsed_ms`, `filter_failed`
- `dropped_candidates[]` (capped), `selected_backend`

## 7. Security Boundary

- Router must not send secrets (API keys, bearer tokens) to the external service.
- `base_url` is optional and should be omitted by default if it reveals internal topology.
- External service output must be constrained and validated (Section 4.3).

## 8. Throughput and Sharing (multiple WASM instances)

### 8.1 Do WASM instances need a shared client?

With the streaming design:

- Spearlet is the gRPC server (listening on TCP), and the filter process is the gRPC client (dialing in).
- The Router should not create a per-request gRPC connection. It should reuse the already-established stream(s) via a process-level `RouterFilterStreamHub`.

Conclusion:

- **Share** a single `RouterFilterStreamHub` across all WASM instances / Host API entry points in the same Spearlet process.
- Avoid per-instance streams: it increases connection count, context switching, and memory pressure without improving tail latency reliably.

### 8.2 How to increase throughput

Recommended layering:

1. **Multiplex concurrency on a single stream (required)**
   - Allow multiple in-flight `FilterRequest` messages on the same bidirectional stream.
   - Use `correlation_id` to map responses back to awaiting callers (`inflight` map).

2. **Multiple agents / multiple streams (recommended)**
   - Support multiple filter agents connecting to the same `addr`.
   - Dispatch with least-inflight or round-robin across `AgentHandle`s.

3. **Backpressure and limits (required)**
   - `max_inflight_total` prevents Router overload when the filter is slow.
   - `per_agent_max_inflight` avoids saturating a single agent and protects P99.

4. **Payload slimming (strongly recommended)**
   - Cap `max_candidates_sent` (e.g. 32–128).
   - Send only summarized `signals`, never large payloads (ASR audio, long prompts).
   - Cap debug kv count/size.

5. **Timeout and late-response handling (required)**
   - Router enforces `decision_timeout_ms` locally and removes inflight entries after timeout.
   - Late `FilterResponse` messages are discarded and counted for observability.
