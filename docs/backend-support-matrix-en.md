# Backend Support Matrix

This document summarizes the backend capability matrix that is already implemented in the current `spear` codebase.

It separates three concerns:

- what operations exist in IR / protocol enums
- what built-in adapters actually implement
- what Web Admin currently allows users to create

This matrix reflects current behavior only. Historical design notes, reserved fields, or future plans are not treated as shipped support.

## Design Principles

- `kind` identifies the backend adapter type, such as `openai_chat_completion`
- `operations` identify the externally declared capability, such as `chat_completions`
- a backend can participate in routing only if:
  - the declared operation is valid
  - the selected `kind` actually implements that operation
- the runtime assembly layer validates `kind` / `operations` consistency before registration

## Built-in Runtime Matrix

| Kind | Hosting | Transport | Currently supported operations | Notes |
|---|---|---|---|---|
| `openai_chat_completion` | remote | `http` | `chat_completions` | Standard OpenAI-style HTTP chat adapter |
| `ollama_chat` | remote or local endpoint-style | `http` | `chat_completions` | Ollama-compatible HTTP adapter |
| `openai_realtime_ws` | remote | `websocket` | `speech_to_text` | Currently implemented as streaming ASR, not a general realtime voice agent |
| `llamacpp` | local | `http` | `chat_completions` | Node-local managed `llama-server` adapter |
| `stub` | local / internal | in-process | `chat_completions` | Test / development only |

## Current Web Admin Create Flows

Web Admin now exposes two explicit create paths:

- `Create Remote Backend`
- `Create Local Backend`

### Remote

Current remote combinations available from the UI:

| Hosting | Provider | Backend Kind | Default / common operations | Notes |
|---|---|---|---|---|
| `remote` | `openai` | `openai_chat_completion` | `chat_completions` | Standard remote HTTP chat backend |
| `remote` | `openai` | `openai_realtime_ws` | `speech_to_text` | Remote websocket ASR backend |
| `remote` | `ollama` | `ollama_chat` | `chat_completions` | External Ollama-compatible endpoint |

### Local

Current local combinations available from the UI:

| Hosting | Provider | Backend Kind | Current behavior | Notes |
|---|---|---|---|---|
| `local` | `llamacpp` | `llamacpp` | managed local runtime | Starts / manages node-local `llama-server` |
| `local` | `vllm` | `vllm` | scaffolded / external-endpoint oriented | Not yet a fully managed local process path |

### Local llama.cpp surfaced fields

For `local + llamacpp`, Web Admin now exposes common runtime fields directly and persists them into backend metadata:

- `model_url`
- `model_path`
- `skip_download`
- `download_timeout_s`
- `threads`
- `ctx_size`

## Operations Present in Protocol But Not Fully Implemented

The following operations may still appear in parts of IR, filters, or future-facing abstractions, but there is no complete built-in adapter support today:

- `embeddings`
- `image_generation`
- `text_to_speech`
- `realtime_voice`

So when judging support, use the runtime adapter matrix plus `kind` / `operation` validation, not enum presence alone.

## Recommended Current Usage

- Standard chat backend:
  - `kind = openai_chat_completion`
  - `operations = ["chat_completions"]`
- Realtime ASR backend:
  - `kind = openai_realtime_ws`
  - `operations = ["speech_to_text"]`
- Local llama.cpp backend:
  - `kind = llamacpp`
  - `operations = ["chat_completions"]`
  - plus local metadata-backed runtime fields

## Discouraged Configurations

The following should not be treated as valid runtime-ready configurations in the current codebase:

- `openai_realtime_ws` + `realtime`
- `openai_realtime_ws` + `realtime_voice`
- `openai_chat_completion` + `speech_to_text`
- any `kind` declaring an operation that its adapter does not implement

## Recommended Extension Order

If a new operation needs to become first-class support:

1. implement the adapter behavior first
2. register the `kind` / `operation` mapping in runtime assembly
3. expose it in Web Admin only after runtime support exists

Avoid exposing operations in UI before the runtime can actually execute them.

## Related Current Docs

- [web-admin-overview-en.md](./web-admin-overview-en.md) for the current Web Admin page and API surface
- [web-admin-ui-guide-en.md](./web-admin-ui-guide-en.md) for actual operator workflows
- [ai-backend-unified-control-plane-design-en.md](./ai-backend-unified-control-plane-design-en.md) for the control-plane model behind backend identity, placement, and status
