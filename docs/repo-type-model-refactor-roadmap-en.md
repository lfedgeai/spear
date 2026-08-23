# Repository Type Model Refactor Roadmap

## Purpose

This document defines a repository-wide refactor roadmap for a recurring design smell found across `sms`, `spearlet`, and `web-admin`:

- sparse "bag of options" structs
- magic-string based metadata or config keys
- flat DTOs that mix multiple semantic variants
- repeated string normalization for `provider`, `hosting`, `origin`, and similar fields

The goal is not to rewrite everything at once. The goal is to establish a phased, low-risk migration path toward stronger type boundaries, smaller orchestration functions, and easier long-term maintenance.

## Why This Matters

The same pattern now appears in multiple layers:

- config loading
- Web Admin write models
- SMS admin DTOs
- runtime backend assembly
- local / remote preflight request and response models
- task configuration mapping in Web Admin

When the same weak model shape appears in several places, each layer compensates by adding:

- extra validation branches
- duplicate string parsing
- repeated field mapping
- more `Option<T>` or `undefined` handling
- hand-written synchronization between flat forms and metadata JSON

This increases maintenance cost and makes it easy for one layer to drift from another.

## Problem Statement

### 1. One Struct Carries Multiple Variants

Several files still use one large struct or interface to represent multiple variants that should be modeled explicitly.

Representative examples:

- `src/spearlet/config.rs`
- `src/spearlet/ai/backend_assembly.rs`
- `src/sms/web_admin/ai_backend_admin.rs`
- `src/spearlet/http_gateway/backend_preflight_handlers.rs`
- `web-admin/src/api/ai-backends.ts`
- `web-admin/src/api/types.ts`

### 2. Magic Strings Leak Across Layers

Some domain concepts exist only as string keys inside `metadata` or `config` maps.

Representative examples:

- `model_url`
- `model_path`
- `skip_download`
- `download_timeout_s`
- `mcp.enabled`
- `mcp.default_server_ids`
- `mcp.tool_allowlist`

This forces UI, SMS, and runtime code to manually keep the same implicit schema in sync.

### 3. String Fields Encode Domain Variants

Fields such as these are still normalized or interpreted repeatedly:

- `provider`
- `hosting`
- `origin`
- `desired_state`
- `backend_kind`

This creates scattered logic and prevents the compiler from helping with invalid states.

### 4. API Envelopes Encode Status More Than Once

Some admin responses still combine:

- `success`
- `message`
- optional payload
- ad hoc shape differences

This makes clients check several fields to understand one outcome.

## Refactor Principles

- Model variants with tagged enums or discriminated unions.
- Keep string parsing at serialization boundaries only.
- Prefer explicit typed metadata objects over free-form JSON maps.
- Split identity, desired spec, observed status, and transport DTOs into separate layers.
- Keep orchestration code short and move field mapping into dedicated mappers.
- Require focused tests for every new typed mapper or enum branch.

## Priority Areas

### Priority 1. Backend Config and Runtime Assembly

Primary files:

- `src/spearlet/config.rs`
- `src/spearlet/ai/backend_assembly.rs`

Reason:

- these files are upstream sources for several later weak-model patterns
- improvements here reduce downstream normalization and special cases

### Priority 2. Web Admin Write Models and SMS Entry DTOs

Primary files:

- `src/sms/web_admin/ai_backend_admin.rs`
- `web-admin/src/features/ai-backends/AiBackendEditorForm.ts`
- `web-admin/src/features/ai-backends/AiBackendEditorDialog.tsx`

Reason:

- these files currently amplify metadata magic strings
- these files directly shape the API contract used by users

### Priority 3. Shared API Types and Admin Envelopes

Primary files:

- `src/sms/ai_admin_api.rs`
- `web-admin/src/api/ai-backends.ts`
- `web-admin/src/api/types.ts`

Reason:

- these files determine how complexity propagates into page components and tests

### Priority 4. Task Config Mapping

Primary files:

- `web-admin/src/features/tasks/TasksPage.tsx`

Reason:

- the MCP configuration path repeats the same magic-string pattern as AI backend metadata

## Phase Plan

## Phase 1. Introduce Canonical Enums and Typed Variant Boundaries

### Goal

Stop re-parsing the same domain strings in multiple layers.

### Planned Work

- Introduce canonical enums or typed wrappers for:
  - `provider`
  - `hosting`
  - `origin`
  - `desired_state`
  - `backend_kind`
- Keep string conversion at API / config serialization boundaries.
- Make internal mappers consume typed values instead of raw strings.

### Current Progress

The first batch of Phase 1 has landed.

Completed so far:

- added a shared `src/ai_backend_types.rs` helper module
- introduced canonical typed parsing for:
  - `provider`
  - `hosting`
  - `backend_kind`
- extended the same shared typed helper approach to:
  - `origin`
  - `desired_state`
  - `management_mode`
- updated `backend_assembly` to infer provider and normalize kind/hosting through shared typed helpers
- updated SMS admin input mapping to normalize provider/kind and to gate preflight paths through typed selectors
- updated backend validation to reuse typed kind-to-hosting rules instead of repeating raw string match lists
- updated admin desired-state / management-mode parsing and proto-to-domain enum conversion to reuse the same shared typed helpers

### Acceptance Criteria

- New internal code branches on enums, not repeated string comparisons.
- Invalid combinations are rejected earlier.
- Existing behavior remains unchanged at external boundaries.

## Phase 2. Replace Sparse Backend Config Bags with Typed Variants

### Goal

Split local and remote backend configuration into explicit variants.

### Planned Work

- Replace weak configuration bags in `src/spearlet/config.rs` with typed variants such as:
  - `LocalBackendConfig`
  - `RemoteBackendConfig`
- Add provider-specific typed sub-config where needed.
- Move validation rules into constructors or typed conversion helpers.

### Current Progress

The first Phase 2 slice has landed without changing the external config file shape yet.

Completed so far:

- introduced `AiBackendTypedView` in `src/spearlet/config.rs`
- added `AiBackendConfig::typed_view()` to derive a typed internal view from the sparse config bag
- updated `validate_spearlet_config()` to validate backend config through the typed view instead of raw string checks
- updated `backend_assembly` to prefer the typed config view when converting config into `BackendSpec`
- added focused tests for alias normalization and the valid `local + ollama_chat` configuration path
- evolved the typed config view from one flat object into explicit `Local` / `Remote` variants backed by shared common fields
- updated `backend_assembly` to consume the new config variants through dedicated local/remote conversion helpers
- moved minimal variant constraints into typed conversion itself, including:
  - remote backends require `base_url`
  - local endpoint-style backends require `base_url`
  - managed `local + llamacpp` remains allowed without `base_url`
  - invalid kind/hosting combinations are rejected by the typed config layer
- added provider-specific subviews on top of the `Local` / `Remote` variants:
  - `LocalBackendProviderView`
  - `RemoteBackendProviderView`
  - `AiBackendProviderView`
- updated typed config construction to reject `provider` / `kind` mismatches explicitly instead of letting one backend drift across two raw string meanings
- updated local variant validation to express endpoint-style local-provider requirements through the provider-specific subview layer
- updated `backend_assembly` to enter finer-grained local/remote conversion paths through provider-specific classification helpers, creating a stable boundary for later provider-specific assembly rules
- updated `remote_preflight` to classify `openai_compatible` / `ollama` through a typed provider-family selector instead of returning raw magic strings directly
- updated runtime adapter / capability selection in `backend_assembly` to route through a typed runtime-family selector instead of scattering backend-kind string constants
- updated `local_models/provider` and `backend_assignment_controller` to resolve local assignment drivers through shared canonical helpers instead of maintaining handwritten local alias tables
- updated `backend_assignment_controller` to funnel local assignment reconcile entry through `LocalAssignmentRuntimeFamily`, so controller branching now consumes a typed local runtime family directly
- updated provider-specific local metadata reads in `backend_assignment_controller` to first flow through `LlamaCppAssignmentParams` / `VllmAssignmentParams`, reducing direct raw `HashMap<String, String>` consumption inside reconcile branches
- updated `local_models/llamacpp` to funnel `server_mode / server_cmd / server_cmd_args / threads / ctx_size / ready_probe / skip_download / model_url` through `LlamaCppLaunchOptions` and `LlamaCppModelSource`
- updated `local_models/vllm` to resolve external endpoints into a typed `base_url/source` result, and wired `backend_assignment_controller` to consume that result directly
- updated node-side `local_model_handlers` to funnel local-model preflight provider selection and llama.cpp param assembly through a typed provider family and typed params builder
- updated node-side `local_model_handlers` and `sms/web_admin/ai_backend_admin` local-model preflight DTOs to replace flat `source_kind/final_url/...` payloads with a nested typed source variant
- updated node-side `backend_preflight_handlers` and `sms/web_admin/ai_backend_admin` remote-preflight DTOs to replace flat `phase/error_code/checks/...` payloads with a nested typed result variant
- updated remote-preflight DTOs again to refine the generic result into a provider-specific family (`open_ai_compatible / ollama / unknown`) plus typed outcome structure
- further refined success outcomes inside known remote provider families:
  - `OpenAI-compatible` now uses `ConnectivityVerified / AuthVerified / ModelAccessVerified`
  - `Ollama` now uses `ConnectivityVerified / ModelAccessVerified`
  - `Unknown` still keeps generic `Success / Failure` to avoid premature pseudo-precision for unsupported providers
- further refined failure outcomes inside known remote provider families:
  - `OpenAI-compatible` now uses `ConnectivityFailed / AuthFailed / ModelAccessFailed`
  - `Ollama` now uses `ConnectivityFailed / ModelAccessFailed`
  - failure variants now carry stable provider-specific booleans directly instead of reconstructing semantics through `phase + Option<bool>`
- updated the AI backend admin write boundary in `sms/web_admin/ai_backend_admin`:
  - known `llamacpp / vllm` metadata fields are parsed into provider-specific typed input models first
  - added `AiBackendWriteTypedView + provider_view` in the same style as the repo's existing config typed-view layer so `create/update/preflight` share one local/remote and provider-specific parsing boundary
  - added an explicit local request discriminated union, `local.provider_family + config`, and made `llamacpp / vllm` consume that structured input before falling back to raw metadata
  - added an explicit remote request discriminated union, `remote.provider_family + config`, and made `OpenAI-compatible / Ollama` consume that structured input first when resolving effective `base_url / credential_ref / operations / features / transports`
  - once structured `local/remote` input is present, duplicate legacy sources in `metadata / spec / credential_ref` are rejected so dual-source configuration stops spreading
  - for known providers that already have structured models, the boundary is now further tightened to required structured input: `llamacpp / vllm` must provide `local`, and `OpenAI-compatible / Ollama` must provide `remote`
  - create/update/preflight now render one shared normalized metadata JSON shape instead of hand-reading magic strings in multiple places
  - admin input can use natural JSON `bool/number` values, which are normalized at the boundary into the stable string form consumed by the current runtime path
- aligned the Web Admin frontend serializer to the same request contract:
  - `web-admin/src/api/ai-backends.ts` now exposes typed `local / remote` write inputs for known providers
  - `AiBackendEditorForm.buildPayload()` now emits structured known-provider payloads instead of keeping them in legacy top-level `metadata / spec / credential_ref`
  - focused Vitest coverage now locks the structured payload shape for local `llamacpp` and remote `OpenAI-compatible`, and rejects unsupported providers instead of preserving transition payloads
- added focused remote-preflight tests for `generic outcome -> provider-specific outcome` conversion and for admin end-to-end preflight JSON shape
- extended admin preflight integration coverage so the HTTP boundary now also rejects known-provider invalid writes before node proxying:
  - `llamacpp` legacy-only local input is rejected at `/admin/api/ai-backends/preflight`
  - `OpenAI-compatible` legacy-only remote input is rejected at `/admin/api/ai-backends/preflight`
  - `OpenAI-compatible` structured plus legacy duplicate remote sources are rejected at the same endpoint
- extended admin create/update integration coverage so write endpoints enforce the same typed boundary:
  - `/admin/api/ai-backends` rejects `llamacpp` legacy-only local input
  - `/admin/api/ai-backends` rejects `OpenAI-compatible` structured plus legacy duplicate remote sources
  - `/admin/api/ai-backends/:backend_id` rejects `OpenAI-compatible` legacy-only remote updates
- aligned the admin read path with the same typed provider contract:
  - `AdminAiBackendRecordResponse` now exposes structured `local / remote` read views for known providers
  - Web Admin `formFromBackend()` now prefers typed read fields when rehydrating editor state, instead of depending on legacy-only `metadata / spec / credential_ref`
  - focused Rust and Vitest coverage now lock both the response shape and the editor rehydration path
  - `AiBackendDetailPage` now resolves summary and provider-config display from typed read fields first, with focused helper coverage for known local and remote providers
  - admin mutation, list, and detail integration tests now all verify typed `local / remote` read responses for known providers
  - known-provider frontend read paths no longer silently mix typed and legacy fields
  - known-provider editor metadata textarea no longer back-propagates legacy keys into typed form fields; metadata edits now stay isolated to extra record data that is not modeled as typed config
- removed the remaining admin compatibility write path for unsupported providers:
  - admin create and preflight now reject unsupported providers such as `anthropic-compatible` instead of preserving legacy transition payloads
  - the Web Admin editor now validates and rejects unsupported providers before attempting submission
  - local `llamacpp` field edits no longer double-write into raw metadata JSON; typed config is the single source of truth and metadata is normalized only at save time
- cleaned up the related SMS route-structure tests so the refactor guardrails stay fast and maintainable:
  - `src/sms/routes_test.rs` now uses shared probe helpers for route-existence assertions instead of repeating ad hoc request/status logic
  - POST and PUT route probes that only need routing coverage now use malformed JSON to fail at the Axum extractor layer, avoiding accidental dependency on lazy gRPC connection failures
  - the route test suite keeps the same route-matched semantics while running in milliseconds instead of waiting on downstream timeout paths

### Acceptance Criteria

- Runtime config does not rely on large groups of unrelated optional fields.
- Validation logic becomes shorter and more local.
- Tests cover valid and invalid config combinations.

## Phase 3. Introduce Typed Metadata Models for AI Backend Forms

### Goal

Remove hand-maintained metadata magic strings from Web Admin and SMS input paths.

### Planned Work

- Introduce explicit typed models for local backend metadata, for example:
  - `LlamaCppLocalModelSource`
  - `LlamaCppRuntimeOptions`
- Replace flat form mirroring logic with typed form state.
- Restrict raw JSON editing to a clear advanced mode if still needed.
- Centralize metadata-to-wire mapping in one mapper module.

### Acceptance Criteria

- `AiBackendEditorForm` no longer manually synchronizes several fields with a stringified JSON blob.
- SMS request mapping consumes typed metadata instead of scattered magic keys.
- Adding one new metadata field requires changes in one mapper path, not several components.

## Phase 4. Normalize Admin API Result Shapes

### Goal

Reduce response ambiguity and remove repeated status encoding.

### Planned Work

- Standardize response envelopes across admin endpoints.
- Keep typed payload variants for cases such as preflight details.
- Reduce combinations like `success + optional payload + extra flags` when one typed result model is enough.

### Acceptance Criteria

- Clients can understand result state from one consistent shape.
- Tests become simpler because fewer null combinations exist.
- API DTOs separate common headers from variant-specific details.

## Phase 5. Split Frontend API Types by Layer

### Goal

Prevent one TypeScript file from carrying summary, detail, write, runtime, and diagnostic models together.

### Planned Work

- Split `web-admin/src/api/ai-backends.ts` into layer-specific type modules.
- Split `web-admin/src/api/types.ts` into smaller domain-focused files.
- Keep discriminated unions where multiple variants are genuinely needed.

### Acceptance Criteria

- Page components import narrower types.
- Summary and detail models do not silently drift.
- The number of defensive null checks in feature pages decreases.

## Phase 6. Refactor Task Configuration Away from String-Key Maps

### Goal

Apply the same typed-config discipline to task-side MCP configuration.

### Planned Work

- Introduce explicit MCP form models in `TasksPage`.
- Move `mcp.*` wire mapping into a dedicated conversion helper.
- Split the large task creation form into smaller focused modules.

### Acceptance Criteria

- MCP config is represented by typed objects before serialization.
- New MCP settings do not require string-key updates scattered in the page component.
- The task creation UI becomes easier to test in isolation.

## Implementation Strategy

- Start at the highest fan-out sources first.
- Prefer additive typed wrappers before deleting legacy fields.
- Migrate one vertical slice at a time:
  - type
  - mapper
  - caller
  - tests
  - docs
- Avoid mixed partial migrations that leave two equal "canonical" paths alive for long.

## Recommended First Three Work Items

1. Introduce typed backend enums and use them in backend assembly and admin input mapping.
2. Extract typed `llamacpp` metadata models and remove raw-key synchronization from Web Admin forms.
3. Split `AiBackendConfig` into explicit local / remote variants and push validation into typed constructors.

## Non-Goals

- Rewriting every DTO in one change
- Removing all optional fields from the repository
- Changing external API compatibility without an explicit contract change decision
- Refactoring unrelated business logic only because it lives in the same file

## Testing Expectations

- Each phase must add or update focused tests.
- Mapper tests are required when one wire shape maps to a typed internal model.
- Integration tests must confirm that typed refactors preserve existing runtime behavior.
- Documentation must be updated in both English and Chinese after each completed phase.

## Success Criteria

This roadmap is successful when:

- new backend or provider features require fewer scattered file edits
- fewer magic strings are shared across UI, SMS, and runtime layers
- internal code branches on enums or variants instead of repeated string checks
- API and form models become easier to read without scanning large optional field bags
- test cases become more local and less defensive
