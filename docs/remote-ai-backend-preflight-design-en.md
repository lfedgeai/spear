# Remote AI Backend Preflight Design

## Overview

This document defines how SPEAR should verify `remote` AI backends before creation, with special attention to credential validation, model access checks, and `all_nodes` placement behavior.

The design extends the current node-side preflight pattern already used by local `llamacpp` backends instead of introducing a second ad hoc validation flow.

Current related implementation:

- Web Admin preflight entry: [`../src/sms/web_admin/ai_backend_admin.rs`](../src/sms/web_admin/ai_backend_admin.rs)
- Current local-only preflight route: `POST /admin/api/ai-backends/preflight`
- Current node-side local preflight route: `POST /internal/ai/local-models/preflight`

## Problem Statement

Today a remote backend can usually be created even when one of the following is already wrong:

- the target node cannot reach the provider endpoint
- TLS, DNS, or egress settings are broken on the actual runtime node
- the selected credential is invalid
- the credential cannot access the target model or deployment
- the provider configuration is syntactically valid but operationally unusable

This delays failures until reconciliation time or the first live request, which makes the user workflow slower and harder to diagnose.

## Goals

- Fail fast for clearly invalid remote backend definitions
- Verify from the real execution node instead of from the browser or SMS
- Return structured, user-readable failure reasons
- Reuse one unified preflight framework for local and remote backends
- Keep backend creation responsive even when placement targets many nodes
- Preserve clear ownership:
  - Web Admin handles interaction
  - SMS handles orchestration
  - SPEARlet handles node-side truth

## Non-Goals

- Full capability certification for every provider feature in v1
- Background periodic health checks for all backends
- Long-term persistence of historical preflight runs
- Automatic credential repair or fallback credential selection
- Synchronous full-cluster blocking validation for large `all_nodes` placements

## Design Principles

### Node-Side Truth

Validation must run from the final execution node because only that node sees the real network, proxy, TLS, DNS, and credential resolution environment.

### Lightweight Probe

Preflight should prove usability with the smallest reliable request. It must not perform expensive model inference unless no cheaper provider-specific probe exists.

### Structured Results

The system should return stable error codes and phases rather than only free-form text.

### Provider-Aware Validation

Standard providers such as OpenAI-compatible backends and remote Ollama can support stronger semantic checks. Generic HTTP backends should use weaker or configurable checks.

### Two-Stage Validation for Large Placements

For `all_nodes` placement, preflight should not default to synchronous validation on every node before creation. The system should use sampled strict validation before create, then asynchronous full verification after create.

## High-Level Flow

### Single Node Placement

1. User submits a remote backend draft in Web Admin
2. Web Admin calls `POST /admin/api/ai-backends/preflight`
3. SMS validates the request shape and resolves the target node
4. SMS forwards a node-side preflight request to the selected SPEARlet
5. SPEARlet resolves credentials, probes the provider, and returns a structured result
6. Web Admin creates the backend only if preflight succeeds

### All Nodes Placement

1. User submits a remote backend draft
2. Web Admin calls the same preflight API
3. SMS selects a representative sample of nodes
4. SMS runs synchronous strict validation only on the sample
5. If the sample fails, creation is blocked
6. If the sample passes, Web Admin proceeds with creation
7. After creation, SMS schedules asynchronous full-node verification
8. Node-level verification results flow into backend node status and aggregated backend summary

## Why Not Validate Every Node Before Create

Validating every node synchronously before creation is operationally correct but not a good default because it:

- increases creation latency linearly with cluster size
- makes the workflow fragile under transient provider or network instability
- can trigger provider rate limits
- produces a worse first-run experience for large clusters
- couples a control-plane create action to full-cluster availability at a single instant

For that reason, the default policy for `all_nodes` should be:

- sampled strict preflight before create
- asynchronous full verification after create

## Placement Verification Policies

The framework should support explicit policies even if v1 exposes only a subset in UI.

### `single_node_strict`

- used for single-node placement
- the selected node must pass before create
- any failure blocks creation

### `sampled_strict`

- default for `all_nodes`
- a representative node sample must pass before create
- creation succeeds only if the sample passes
- all nodes are still verified asynchronously after create

### `strict_all_nodes`

- optional advanced mode
- every target node must pass before create
- any failure blocks creation
- recommended only for small clusters or explicitly strict environments

### `best_effort`

- optional non-blocking mode
- preflight returns warnings but does not block create
- useful for exploratory environments, not recommended as default

## Representative Sampling Strategy

For `sampled_strict`, SMS should select a small but meaningful sample instead of a purely random subset.

Preferred strategy:

1. Choose at least one node per topology domain if topology labels are available
2. Prefer routing-default or scheduler-preferred nodes
3. Otherwise choose `min(3, total_nodes)` nodes

Future topology keys may include:

- region
- availability zone
- rack or subnet group
- node pool or hardware class

If topology metadata is not yet available, v1 should still implement deterministic bounded sampling.

## Provider Scope for v1

Recommended initial provider scope:

- `openai`
- `openai_compatible`
- `ollama`

Deferred to later iterations:

- rich verification for generic `http_json`
- realtime or audio capability verification
- tool-calling capability verification
- provider-specific deployment topologies beyond the common path

## Validation Semantics by Provider

### OpenAI-Compatible

Checks:

- endpoint reachability
- TLS and HTTP success
- credential validity
- target model visibility or accessibility

Preferred probe order:

1. model metadata or model listing endpoint
2. direct target model lookup if supported
3. minimal low-cost request only if metadata inspection is insufficient

### Remote Ollama

Checks:

- base URL reachability
- basic API health
- target model existence through tags or model metadata

### Generic HTTP JSON

v1 should only do weak validation:

- endpoint reachable
- configured headers applied
- response is not an obvious auth failure

Stronger validation for generic adapters should be added later through explicit healthcheck configuration.

## API Design

### Web Admin / SMS Public API

Keep the existing route:

- `POST /admin/api/ai-backends/preflight`

Current request shape:

```json
{
  "backend": { "...": "..." },
  "node_uuids": ["node-a", "node-b"]
}
```

Recommended v2-compatible request shape:

```json
{
  "backend": { "...": "..." },
  "node_uuids": ["node-a", "node-b"],
  "verification_policy": "sampled_strict",
  "requested_checks": ["connectivity", "auth", "model_access"]
}
```

### SMS to SPEARlet Internal API

Introduce a generic node-side route:

- `POST /internal/ai/backends/preflight`

Suggested request body:

```json
{
  "backend": {
    "provider": "openai_compatible",
    "hosting": "remote",
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1",
    "credential_ref": "cred-openai-prod",
    "spec": {
      "operations": ["chat"],
      "features": ["streaming"],
      "transports": ["https"]
    },
    "metadata": {
      "api_style": "openai"
    }
  },
  "requested_checks": ["connectivity", "auth", "model_access"]
}
```

Suggested response body:

```json
{
  "success": true,
  "provider": "openai_compatible",
  "phase": "model_access",
  "error_code": null,
  "message": "authentication and model access verified",
  "resolved_endpoint": "https://api.openai.com/v1",
  "http_status": 200,
  "provider_code": null,
  "latency_ms": 183,
  "checks": [
    { "name": "connectivity", "ok": true },
    { "name": "auth", "ok": true },
    { "name": "model_access", "ok": true }
  ],
  "auth_valid": true,
  "model_accessible": true
}
```

## Result Model

SMS should normalize node-side results into a shared response type for Web Admin.

Recommended node result fields:

- `node_uuid`
- `provider`
- `success`
- `phase`
- `error_code`
- `message`
- `resolved_endpoint`
- `http_status`
- `provider_code`
- `latency_ms`
- `auth_valid`
- `model_accessible`
- `checks`

## Error Model

Recommended stable error codes:

- `invalid_input`
- `unsupported_provider`
- `unsupported_feature`
- `node_unreachable`
- `dns_resolve_failed`
- `connect_timeout`
- `tls_handshake_failed`
- `http_404`
- `http_5xx`
- `auth_missing`
- `auth_invalid`
- `permission_denied`
- `model_not_found`
- `model_not_accessible`
- `provider_rate_limited`
- `provider_unavailable`

Recommended phases:

- `input`
- `connectivity`
- `auth`
- `model_access`
- `capability`

## Status Model After Creation

The system should separate backend-level sampled verification from node-level full verification.

### Backend-Level Summary

Suggested states:

- `pending`
- `sampled_verified`
- `sampled_failed`
- `fully_verified`
- `partially_verified`
- `failed`

### Node-Level Verification State

Suggested states:

- `pending`
- `ready`
- `auth_failed`
- `network_failed`
- `model_not_accessible`
- `timeout`

These states should be surfaced through the existing backend node status views and backend summary views.

## UI Behavior

### Create Remote Backend

Default behavior:

1. User clicks `Create`
2. Web Admin automatically calls preflight
3. On failure, creation stops and the dialog shows the structured error
4. On success, creation continues automatically

### All Nodes Placement

Before create:

- show sampled verification result
- explain that full verification continues after creation

After create:

- show backend summary such as `18/20 nodes verified`
- show node-level failure reasons in the backend detail page

## Security Considerations

- Web Admin must never receive raw secrets back from preflight
- SMS should avoid handling more secret material than needed
- SPEARlet should resolve credentials through the same path used by runtime execution
- preflight logging must redact sensitive headers and tokens
- failure payloads must not echo secrets

## Module Layout Recommendation

Suggested implementation layout:

- SMS orchestration:
  - [`../src/sms/web_admin/ai_backend_admin.rs`](../src/sms/web_admin/ai_backend_admin.rs)
  - extracted helpers such as `backend_preflight.rs`
- SPEARlet node-side handlers:
  - new internal route under the HTTP gateway
- provider adapters:
  - `remote_preflight/mod.rs`
  - `remote_preflight/common.rs`
  - `remote_preflight/openai_compatible.rs`
  - `remote_preflight/ollama.rs`
  - `remote_preflight/http_json.rs`

## Rollout Plan

### Phase 1

- unify local and remote preflight orchestration
- add node-side remote preflight route
- support `single_node_strict`
- support OpenAI-compatible and Ollama

### Phase 2

- add `sampled_strict` for `all_nodes`
- add asynchronous full verification after create
- expose aggregated verification progress in Web Admin

### Phase 3

- add optional `strict_all_nodes`
- add richer provider-specific capability checks
- add configurable generic HTTP health checks

## Testing Plan

### Unit Tests

- provider request builders
- credential resolution failures
- error code mapping
- sampling policy selection

### Integration Tests

- Web Admin preflight orchestration
- mock OpenAI-compatible provider success and failure
- mock Ollama provider success and failure
- `all_nodes` sampled strict behavior

### UI Tests

- create blocks on preflight failure
- create continues on success
- sampled and full-verification messaging renders correctly

## Open Questions

- Which topology labels should drive representative sampling in the first production rollout
- Whether `generic http_json` should support a custom verification contract in v1 or later
- Whether backend verification summary should live inside existing node status records or in a dedicated verification sub-status

## Final Recommendation

The recommended default design is:

- use a unified backend preflight framework
- validate from the target SPEARlet node
- use strict validation for single-node remote backends
- use sampled strict validation before create for `all_nodes`
- run asynchronous full-node verification after create
- surface both sampled and full verification status in Web Admin
