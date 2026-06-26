# Migration from legacy and Phased Delivery (MVP → Expansion)

## 1. Mapping legacy to the new design

### 1.1 TransformRegistry → Capability router

Legacy: selects a transform by subset matching on `input_types/output_types/operations` (`legacy/spearlet/hostcalls/transform.go`).

New design:

- map `operations` to `Operation`
- map `input/output types` to modalities/`Feature`
- replace “most specific transform” with “filter then select by policy”

### 1.2 APIEndpointMap → BackendRegistry

Legacy: hard-coded `APIEndpointMap` (`legacy/spearlet/core/models.go`) and filters endpoints by env keys.

New design:

- config-driven backend instances
- Cargo feature pruning + registry build
- discovery APIs expose “enabled instances + capabilities”

### 1.3 rt-asr → stream subsystem

Legacy: WebSocket session + append audio + delta events (`legacy/spearlet/stream/rt_asr.go`).

New design:

- Planned only: `Operation::realtime_voice` + `Transport::websocket|grpc` (not implemented in the current runtime)
- stream subsystem shares registry/capabilities with router

## 2. MVP phases

### Phase 1: Chat (minimum closed loop)

- Keep `cchat_*` ABI stable
- Normalize: session → `CanonicalRequestEnvelope(operation=chat_completions)`
- Router: implement a minimal policy (e.g., `weighted_random`)
- Backends: start with `backend-stub`, then add one OpenAI-compatible HTTP backend
- Discovery: provide `GET /capabilities`

### Phase 2: Embeddings / Image / ASR / TTS (expand by operation)

- Add payload skeletons and capabilities per operation
- Reuse the same router/registry/policies
- Introduce `MediaRef` for image/audio and unify object storage interaction

### Phase 3: Realtime voice / streaming (planned only, not implemented in the current runtime)

- Add streaming lifecycle and event model
- Enforce capabilities (bidi stream, transport, session constraints)
- Reliability: concurrency control and reconnect strategy when needed

### Phase 4: Advanced routing and governance

- circuit breaking and outlier ejection
- cost-aware routing
- hedging/mirroring gated by operation and budget
