# AI Runtime Refactor Roadmap

## Purpose

This document records the refactor roadmap for the AI backend control-plane/data-plane path in `spearlet` and `sms`.

The goals are:

- Make the code more modular and easier to extend
- Reduce cross-module duplication
- Shorten overly long functions
- Clarify ownership boundaries between config, dynamic registry, routing, reporting, and Web Admin
- Keep each refactor step small enough to verify with tests

This roadmap is intended to be executed incrementally. Each phase should remain independently reviewable and revertible.

## Current Problems

### 1. Backend assembly logic is duplicated

Backend normalization and runtime assembly are currently spread across multiple modules:

- `src/spearlet/execution/ai/router/builder.rs`
- `src/spearlet/execution/ai/router/mod.rs`
- `src/spearlet/backend_reporter.rs`
- `src/spearlet/local_models/llamacpp.rs`
- `src/spearlet/ai/backend_assignment_controller.rs`

This duplication includes:

- provider inference
- hosting normalization
- `credential_ref` validation
- `BackendSpec` to runtime adapter conversion
- status/report snapshot mapping

As a result, adding a new backend kind requires editing several places and keeping them manually aligned.

### 2. Two dynamic-backend mental models still coexist

The codebase originally contained both:

- effective-config style remote-backend merge
- dynamic registry based live injection

The runtime path already relied mainly on the dynamic registry, while the older effective-config path increased conceptual load.

### 3. Routing logic is too concentrated

`Router::route()` currently mixes:

- static/dynamic instance merge
- request constraints
- candidate filtering
- external gRPC filter interaction
- model binding restrictions
- selection policy
- no-candidate error explanation

This is difficult to read and risky to extend.

### 4. gRPC filter stream code is over-coupled

`grpc_filter_stream.rs` currently combines:

- singleton lifecycle
- worker startup
- protocol mapping
- inflight request correlation
- blocking wait bridge

These concerns should be separated.

### 5. SMS service and Web Admin contain God modules

The following files have become too large and multi-purpose:

- `src/sms/service.rs`
- `src/sms/web_admin.rs`

They should be decomposed by capability and subdomain.

### 6. HTTP backend adapters repeat infrastructure logic

`openai_chat_completion` and `ollama_chat` share very similar request execution, timeout, error mapping, and response envelope logic. This should be extracted into a small reusable base layer.

## Refactor Principles

- Prefer one canonical source of truth per concern
- Prefer composition over cross-module special-casing
- Prefer typed helpers over repeated ad-hoc JSON and map transformations
- Keep orchestration functions short and explicit
- Each phase must preserve behavior unless the phase explicitly changes the contract
- Each phase must add or update focused tests

## Phase Plan

## Phase 1. Unify backend description and assembly

### Goal

Introduce a shared backend assembly layer so static config, SMS-managed backends, and local-controller backends go through the same normalization path.

### Planned work

- Introduce a shared mapper/factory layer, for example:
  - `BackendDescriptor`
  - `BackendAssembler`
  - `AdapterFactory`
- Move shared logic out of:
  - `router/builder.rs`
  - `router/mod.rs`
  - `backend_reporter.rs`
- Centralize:
  - provider inference
  - hosting mapping
  - credential resolution policy
  - `BackendSpec` to `BackendInstance` conversion

### Current progress

The first shared module is now `src/spearlet/ai/backend_assembly.rs`.

At the current stage, it already centralizes:

- `AiBackendConfig -> BackendSpec`
- typed parts -> `BackendSpec`
- hosting/origin/provider normalization
- operation parsing
- `BackendSpec -> Capabilities`
- `BackendSpec -> Adapter`
- `AiBackendConfig -> BackendInstance`
- `BackendSpec -> BackendInstance`
- `BackendSpec -> AiBackendConfig`

The module is already wired into:

- router registry builder
- dynamic backend instance conversion
- backend reporter static snapshot mapping
- local `llamacpp` managed backend construction

### Acceptance criteria

- Adding a new backend kind no longer requires editing multiple unrelated assembly paths
- Static and dynamic backends use the same adapter-construction rules
- Existing routing and host API tests remain green

## Phase 2. Choose one dynamic-backend lifecycle model

### Goal

Remove conceptual overlap between effective-config merge and dynamic-registry injection.

### Planned work

- Make dynamic registry the explicit runtime source of truth
- Reduce or remove the old effective-config merge path if it is no longer needed
- Remove dead or misleading parameters and helpers
- Document the final lifecycle clearly

### Current progress

The old runtime-facing `effective_config` path has now been removed from the active code path.

Completed changes:

- removed the obsolete `apply_sms_remote_backends` / `EffectiveSpearletConfig` runtime model
- remote merge policy was once extracted into a separate file; that file has now been removed together with the legacy remote-sync path
- updated router builder documentation to describe dynamic registry injection as the only runtime path
- removed the unused `EngineHolder` constructor dependency from the old remote-sync controller

### Acceptance criteria

- Runtime backend mutation has one primary code path
- No stale helper remains that suggests a second runtime model
- Documentation matches actual runtime behavior

## Phase 3. Split routing orchestration into smaller steps

### Goal

Make routing logic easy to read, reason about, and test.

### Planned work

- Split `Router::route()` into smaller internal helpers, for example:
  - `collect_instances`
  - `collect_candidates`
  - `apply_request_constraints`
  - `apply_external_filter`
  - `restrict_by_model_binding`
  - `select_candidate`
  - `explain_no_candidate`

### Current progress

The first Phase 3 refactor step is now in place.

Completed changes:

- split `Router::route()` into smaller orchestration helpers
- separated instance collection, candidate collection, routing constraints, external filter application, model binding, final selection, and no-candidate explanation
- added a focused test for explicit backend / allowlist / denylist routing constraints
- extracted router-filter decision interpretation into a dedicated `filter_decision` helper module
- reused the same filter-decision helper in `http_gateway` to reduce duplicate candidate-decision handling
- extracted no-candidate explanation into `candidate_explainer`
- extracted final selection and candidate log snapshot helpers into `selection`

### Acceptance criteria

- `Router::route()` becomes short and orchestration-only
- Candidate filtering behavior remains unchanged
- Error reporting remains understandable

## Phase 4. Decouple gRPC filter stream layers

### Goal

Separate transport, lifecycle, protocol mapping, and blocking-bridge concerns in router filter streaming.

### Planned work

- Extract protocol mapping helpers
- Extract request/response correlation management
- Isolate worker lifecycle
- Hide implementation behind a thin trait or facade used by the router

### Current progress

The first Phase 4 split is now in place.

Completed changes:

- extracted protocol mapping, request construction, requested-model lookup, and trace construction into `router/filter_protocol.rs`
- simplified `grpc_filter_stream.rs` so the hub focuses more on worker lifecycle and the sync blocking bridge
- extracted inflight correlation registration/completion/blocking wait logic into `router/filter_inflight.rs`
- extracted worker/client lifecycle into `router/filter_worker.rs`

### Acceptance criteria

- Router depends on a small decision interface
- gRPC streaming details are local to dedicated modules
- The code is easier to unit test without background worker coupling

## Phase 5. Refactor SMS service and Web Admin by subdomain

### Goal

Reduce the size and responsibility overlap in `sms/service.rs` and `sms/web_admin.rs`.

### Planned work

- Split `sms/service.rs` by capability:
  - admin AI config
  - model deployment
  - placement
  - execution index / projectors
  - registry operations
- Split `sms/web_admin.rs` by handler group:
  - backends
  - models
  - tasks
  - executions
  - node RPC helpers
  - typed request/response DTOs

### Current progress

The first Phase 5 split is now in place.

Completed changes:

- extracted repeated node lookup + lazy channel construction logic from `sms/web_admin.rs` into `sms/web_admin/node_rpc.rs`
- updated execution termination, instance destruction, direct-node invocation, and placement spillback paths to reuse the same node RPC helper
- extracted shared invocation request building, result summarization, and placement outcome classification into `sms/web_admin/invocation_flow.rs`
- extracted backend/model snapshot aggregation into `sms/web_admin/backend_catalog.rs`
- extracted instance/execution index projector lifecycle into `sms/projectors.rs`
- extracted placement penalty state into `sms/placement/state.rs`
- extracted placement candidate filtering/scoring/selection helpers into `sms/placement/policy.rs`
- extracted placement outcome parsing/classification/request-building helpers into `sms/placement/outcome.rs`
- extracted registry state containers into `sms/registry/state.rs`
- extracted MCP registry record validation/upsert/delete/list helpers into `sms/registry/mcp.rs`
- extracted model deployment registry list/upsert/delete/status helpers into `sms/registry/model_deployments.rs`
- extracted model deployment watch/filter stream helper into `sms/registry/model_deployments.rs`

### Current progress note

`ModelDeploymentRegistryServiceTrait` now keeps only gRPC-facing orchestration in `service.rs`.
The list/upsert/delete/status/watch data-path helpers are centralized in `sms/registry/model_deployments.rs`.

### Acceptance criteria

- File sizes and function sizes are materially reduced
- Handler and service ownership become obvious from module names
- Subdomain tests remain easy to locate

## Phase 6. Extract common HTTP backend execution base

### Goal

Remove repeated infrastructure logic from JSON-over-HTTP backend adapters.

### Planned work

- Introduce a common HTTP execution utility for:
  - request serialization
  - timeout handling
  - status/error mapping
  - response envelope conversion
- Keep provider-specific modules focused on:
  - request body construction
  - successful response interpretation
  - provider-specific error interpretation

### Acceptance criteria

- Backend adapters become shorter
- Shared transport logic is implemented once
- Provider modules are easier to review

## Suggested Order of Execution

The recommended order is:

1. Phase 1: backend assembly unification
2. Phase 2: single dynamic-backend lifecycle
3. Phase 3: router decomposition
4. Phase 4: gRPC filter stream decomposition
5. Phase 5: SMS service / Web Admin decomposition
6. Phase 6: HTTP backend base extraction

This order minimizes risk because the early phases clarify runtime boundaries before splitting larger service modules.

## Working Rules

- Keep each phase small enough for one focused review
- Avoid behavior changes and refactors in the same commit unless explicitly intended
- Update both English and Chinese docs after each phase
- Run focused tests during implementation and the full test suite after each completed phase

## Related Files

- `src/spearlet/execution/ai/router/builder.rs`
- `src/spearlet/execution/ai/router/mod.rs`
- `src/spearlet/ai/backend_assignment_controller.rs`
- `src/spearlet/ai/backend_assignment_controller.rs`
- `src/spearlet/backend_reporter.rs`
- `src/spearlet/execution/ai/router/grpc_filter_stream.rs`
- `src/spearlet/execution/ai/backends/openai_chat_completion.rs`
- `src/spearlet/execution/ai/backends/ollama_chat.rs`

### Current progress

Phase 5 is largely wrapped up, and Phase 6 has now reached a reasonable staged-completion point.

Completed changes:

- added `src/spearlet/execution/ai/backends/http_json.rs` as a small shared JSON-over-HTTP execution helper
- switched `openai_chat_completion` and `ollama_chat` to reuse the same blocking async/runtime + HTTP POST execution path
- extracted shared JSON response parsing, upstream-status error mapping, and canonical payload envelope building into the same HTTP helper layer
- extracted small request-front helpers such as URL joining, chat-operation guard, and non-empty field validation into the same HTTP helper layer
- extracted shared chat param filtering and optional tools injection into the same HTTP helper layer
- kept request body construction, backend-specific error extraction, and success-response interpretation separate to avoid over-abstracting too early

### Current boundary

Phase 6 currently stops at a healthy shared-layer boundary:

- The shared layer now owns:
  - request-front validation
  - JSON-over-HTTP execution
  - response JSON parsing
  - upstream status error mapping
  - canonical payload envelope building
- Provider-specific modules still own:
  - the concrete request body shape
  - provider-specific error extraction
  - provider-specific success-response interpretation

### Not Recommended Next

Unless a third similar HTTP backend is introduced later, it is not recommended to continue by:

- forcing OpenAI and Ollama into one fully unified request builder
- introducing a larger adapter trait or template-method framework
- splitting the shared layer into even smaller helper files just for symmetry

The current state already removes the important duplication while keeping provider differences explicit and easy to review, which makes it a good stopping point for Phase 6.
