# CChat Model Selection and Routing (Implementation Notes)

This document describes how the current CChat (`cchat_*` hostcalls) implementation routes requests using `model` and optional backend constraints, and how to observe which backend was selected.

## 1. The request model

In CChat, the request `model` comes from session params:

- The WASM guest sets `{"key":"model","value":"..."}` via `cchat_ctl(SET_PARAM)`
- Or the WASM-C sample (`samples/wasm-c/chat_completion.c`) selects a model at build time and calls `sp_cchat_set_param_string(fd, "model", model)`

Normalization writes this model into `CanonicalRequestEnvelope.payload` and it participates in routing and backend invocation.

## 2. backend.model binding (model-bound backends)

`[[spearlet.ai.backends]]` supports an optional `model` field:

```toml
[[spearlet.ai.backends]]
name = "openai-chat"
kind = "openai_chat_completion"
base_url = "https://api.openai.com/v1"
hosting = "remote"
model = "gpt-4o-mini"
...
```

Semantics: if a backend has `model` set, it is considered “bound” to that model.

Routing behavior:

- First filter candidates by op/features/transports and routing allowlist/denylist.
- If the request carries a non-empty `model` and any candidate has `backend.model != None`:
  - further retain only candidates where `backend.model == request.model`
  - if that yields zero candidates, return `no_candidate_backend` and include `available_models`

This lets guests select a backend indirectly by setting only `model`, without explicitly setting `backend`.

## 3. Routing to Ollama gemma3

Once Ollama discovery imports `gemma3:1b`:

- set `model = "gemma3:1b"`
- routing will match the candidate backend with `backend.model = "gemma3:1b"`

No explicit backend name is required on the guest side.

## 4. How to tell which backend was selected

Two options are available:

### 4.1 Inspect `_spear.backend` in response JSON

The JSON returned by `cchat_recv` includes a top-level:

```json
"_spear": {"backend": "...", "model": "..."}
```

The WASM-C sample prints:

- `debug_model=...`
- `debug_backend=...`

### 4.2 Check Router debug logs

With debug logging enabled, the Router emits a `router selected backend` debug line after selection, including:

- `selected_backend` / `selected_model`
- operation/model/routing constraints and candidate summary

## 5. Recommendations

- If one backend should support multiple models (e.g. OpenAI), do not bind it via `backend.model`; use explicit `backend`/allowlist/denylist or other selection mechanisms.
- If you want strict “model → backend” routing (e.g. pin some models to local Ollama), set `backend.model` and have guests only set `model`.
