# AI Credentials / credential_ref (Detailed design and implementation)

This document describes the implemented approach in this repository: introduce centralized AI credentials under `spearlet.ai.credentials[]`, and let each backend reference a credential via `credential_ref`.

This enables:

- Different backends using different API keys (e.g., chat vs realtime ASR)
- Multiple backends sharing the same credential without duplicating config
- No plaintext keys in config files (only env-var names)

## 0. Current state

- `AiConfig` includes `credentials` and `backends`: see [config.rs](../../src/spearlet/config.rs)
- `AiBackendConfig` does not support `api_key_env`; API keys are referenced via `credential_ref` (optional)
- `AiBackendConfig.hosting` is required and must be `local` or `remote`
- `deny_unknown_fields` is enabled: a config containing `api_key_env` under `[[spearlet.ai.backends]]` fails to parse

Registry behavior:

- `credential_ref` is optional:
  - if it is set (non-empty): resolve the env-var name via the referenced credential and filter the backend if the env var is missing in the runtime environment (`RuntimeConfig.global_environment` first, then the OS process env)
  - if it is not set: treat the backend as “no-auth” (no API key header)

## 1. Schema (TOML)

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
name = "openai-chat"
kind = "openai_chat_completion"
base_url = "https://api.openai.com/v1"
hosting = "remote"
credential_ref = "openai_chat"
ops = ["chat_completions"]
features = ["supports_tools", "supports_json_schema"]
transports = ["http"]
weight = 100
priority = 0

[[spearlet.ai.backends]]
name = "openai-realtime-asr"
kind = "openai_realtime_ws"
base_url = "https://api.openai.com/v1"
hosting = "remote"
credential_ref = "openai_realtime"
ops = ["speech_to_text"]
transports = ["websocket"]
weight = 100
priority = 0
```

## 2. Rust structs (src/spearlet/config.rs)

```rust
pub struct AiConfig {
    pub default_policy: Option<String>,
    pub credentials: Vec<AiCredentialConfig>,
    pub backends: Vec<AiBackendConfig>,
}

pub struct AiCredentialConfig {
    pub name: String,
    pub kind: String,      // v1: "env"
    pub api_key_env: String,
}

pub struct AiBackendConfig {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub hosting: Option<String>,
    pub model: Option<String>,
    pub credential_ref: Option<String>,
    pub weight: u32,
    pub priority: i32,
    pub ops: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
}
```

## 3. RuntimeConfig.global_environment injection (best practice)

Problem: in sandboxed runtimes (e.g. WASM hostcall), if `RuntimeConfig.global_environment` is empty, backends that reference API keys via `credential_ref` may be filtered.

Implemented approach:

- Collect only the env vars referenced by non-empty `credential_ref`
- Read those env vars from the OS process environment and inject them into each runtime's `RuntimeConfig.global_environment`

Lookup order (current behavior):

- Prefer `RuntimeConfig.global_environment` first (for sandboxed runtimes)
- Fall back to the OS process environment (for non-sandboxed runtimes)

Implementation: [function_service.rs](../../src/spearlet/function_service.rs#L57-L92)

Security note:

- Do not inject `std::env::vars()` wholesale; only inject env vars referenced by configuration

## 4. Backend registry logic

Implementation: [builder.rs](../../src/spearlet/execution/ai/router/builder.rs)

Behavior (current):

- Build a credential index from `ai.credentials[]`
- If a backend has a non-empty `credential_ref`, resolve `api_key_env` via the referenced credential
- Filter a backend if:
  - `credential_ref` is set but the referenced credential does not exist
  - the resolved env var is missing in the runtime environment (`RuntimeConfig.global_environment` first, then the OS process env) or empty

## 5. Migration

- `[[spearlet.ai.backends]] api_key_env = ...` is removed and rejected by parsing (deny_unknown_fields)
- Migrate by moving env-var names into `[[spearlet.ai.credentials]]` and referencing them from backends via `credential_ref`
