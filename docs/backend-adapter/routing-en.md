# Capability Routing and Selection Policies

This document specifies capability modeling, candidate filtering, and selection policies when multiple backends can serve the same capability (LB/Fallback/Hedge/Mirror).

## 1. Capability model

Model capabilities across four dimensions:

- `Operation`: high-level, cross-hostcall semantic operations
- `Feature`: fine-grained capabilities (tools, json schema, timestamps, mask, ...)
- `Limits`: constraints (token limits, audio length, concurrency)
- `Transport`: http/websocket/grpc

Key rule: capabilities belong to a **backend instance** (instances of the same backend kind can differ by model/limits).

Legacy references:

- `OpenAIFunctionType` categorization and the multi-endpoint `APIEndpointMap` (`legacy/spearlet/core/models.go`)
- subset transform selection (`legacy/spearlet/hostcalls/transform.go`)

## 2. Candidate filtering (hard constraints)

Step 1 is filtering:

- must satisfy `required_ops` (often equals the request `operation`)
- must satisfy `required_features` and `required_transports`
- must pass allowlist/denylist
- must be healthy (or pass soft-health rules)
- must pass concurrency/rate limits/budgets

The output is `candidates[]`.

## 3. Scoring and selection (soft preferences)

After filtering, score candidates using preferences:

- boost `preferred_backends`
- optionally incorporate region/cost/latency hints
- optionally incorporate model matching signals

## 4. Selection policies

### 4.1 Single selection (most common)

- `round_robin`
- `weighted_round_robin`
- `weighted_random`
- `least_inflight`
- `ewma_latency`
- `consistent_hash(key=session_id|task_id)`

### 4.2 Fallback and downgrade

- `priority + fallback`: try higher-priority pools first; fallback on error/timeout/429
- integrate with circuit-breaking/outlier ejection: temporarily downweight or remove failing instances

### 4.3 Hedged / fan-out

For tail-latency sensitive workloads with explicit budget:

- `hedged(k=2, delay_ms=50)`
- `mirror_to`: return primary result, mirror to secondary for offline evaluation

## 5. Default policy recommendations (by operation)

- `chat_completions`: `ewma_latency` or `least_inflight`
- `embeddings`: `weighted_round_robin`
- `image_generation`: `priority + fallback` (no hedging by default)
- `speech_to_text`: `least_inflight` or `weighted_rr`
- `text_to_speech`: `weighted_rr`
- Planned: if `realtime_voice` is reintroduced in the future, prefer `least_inflight` (not implemented in the current runtime)

## 6. Explainability

The router should be able to produce structured explanations for diagnostics:

- `candidates_checked`
- `rejected_reasons`
- `selected_instance(s)`
- `policy` and key scoring signals
