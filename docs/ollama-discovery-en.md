# Ollama Model Discovery

This document explains how SPEARlet imports models from Ollama and exposes them in Web Admin.

## Background

SPEARlet builds its runtime backend registry from `spearlet.ai.backends`. To make local Ollama models routable and observable like other AI providers, Ollama discovery is introduced: on SPEARlet startup, it queries Ollama APIs and materializes per-model backends.

## Behavior

- Discovery runs during SPEARlet startup and appends backends with `kind = "ollama_chat"`.
- Imported backends live in the in-memory runtime config and participate in backend reporting and Web Admin.
- Backend “availability” is config/env based (e.g. OpenAI checks the presence of `api_key_env`); it does not probe Ollama network reachability.

## Configuration

The config section is: `[spearlet.ai.discovery.ollama]`.

Key fields:

- `enabled`: enable/disable import.
- `scope`:
  - `serving`: calls `/api/ps` and imports only models currently serving.
  - `installed`: calls `/api/tags` and imports installed models.
- `base_url`: Ollama base URL (default `http://127.0.0.1:11434`).
- `allow_remote`: allow non-loopback base URLs (SSRF risk). Default `false`.
- `allow_models` / `deny_models`: exact match allow/deny lists.
- `max_models`: import cap.
- `name_prefix`: imported backend name prefix (default `ollama/`).
- `name_conflict`: conflict policy: `skip|overwrite`.

Example:

```toml
[spearlet.ai.discovery.ollama]
enabled = true
scope = "installed"
base_url = "http://127.0.0.1:11434"
allow_remote = false
timeout_ms = 1500
max_models = 32
allow_models = []
deny_models = []
name_prefix = "ollama/"
name_conflict = "skip"
default_weight = 100
default_priority = 0
default_ops = ["chat_completions"]
default_features = []
default_transports = ["http"]
```

## Imported backend shape

Each model becomes one backend entry:

- `kind = "ollama_chat"`
- `base_url = <ollama base_url>`
- `hosting = "local"`
- `model = "<model_name>"` (fixed)
- `credential_ref = null` (no secrets)

With “route by model” enabled, guests can set `model = "<model_name>"` and do not need to specify an explicit `backend` name.

## Web Admin

- AI Models (Local) shows aggregated models (provider=ollama) across nodes.
- Click a row to navigate to the model detail page and inspect per-node instances and Raw JSON.

## Troubleshooting

### Enabled but no backends imported

Most commonly `scope` mismatch:

- If you only pulled models but nothing is actively serving, `/api/ps` may be empty. Use `scope = "installed"`.

### Shown as available but invocations fail

Availability does not probe network. Check:

- Ollama is listening on `base_url`
- SPEARlet can reach that address
