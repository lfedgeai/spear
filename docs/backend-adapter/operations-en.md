# Spear Multi-Operation and Multimodal Design (Operations & Capabilities)

This document complements the main proposal by specifying per-`Operation` guidance: payload backbone fields, key capabilities (Feature/Limits/Transport), and typical downgrade strategies.

## 1. Operation Matrix (recommended)

| Operation | Input modalities | Output modalities | Typical transport | Common features |
|---|---|---|---|---|
| `chat_completions` | text (extendable to image/audio) | text/tool_calls | http | `supports_stream` `supports_tools` `supports_json_schema` |
| `embeddings` | text | vector | http | `supports_batch` `supports_dimensions` |
| `image_generation` | text (extendable to image/mask) | image | http | `supports_image_input` `supports_mask` `supports_seed` |
| `speech_to_text` (ASR) | audio | text | http (extendable to ws) | `supports_timestamps` `supports_language` `supports_diarization` |
| `text_to_speech` (TTS) | text | audio | http | `supports_voices` `supports_formats` `supports_ssml` |
| `realtime_voice` (planned) | audio/text events | audio/text events | websocket/grpc | `supports_bidi_stream` `supports_audio_in/out` |

This table is the minimal capability modeling core; it does not require implementing all fields at once. `realtime_voice` is kept here as historical design / future planning only and is not supported by the current runtime.

## 2. Payload backbone fields (skeletons)

This section lists strongly-typed “backbone” fields only. Any missing fields or backend-specific parameters should go into `extra`.

### 2.1 `ChatCompletionsPayload`

- `model: String`
- `messages: Vec<Message>` (role + content parts)
- `tools: Option<Vec<ToolSpec>>`
- `tool_choice: Option<ToolChoice>`
- `generation: GenerationParams` (temperature/top_p/max_tokens/stop/seed/stream...)
- `response_format: Option<ResponseFormat>`

Key capabilities:

- `supports_tools`
- `supports_json_schema`
- `supports_stream`

Typical downgrades:

- missing `supports_tools`: either “tool prompt injection” downgrade or hard error (controlled by `degradation_policy`)
- missing `supports_system_role`: merge system into the first user message (explicitly marked)

### 2.2 `EmbeddingsPayload`

- `model: String`
- `input: Vec<String> | String` (recommend normalizing to a list)
- `dimensions: Option<u32>`
- `encoding_format: Option<String>` (e.g., `float`/`base64`)

Key capabilities:

- `supports_batch`
- `supports_dimensions`

Typical downgrades:

- if batch is not supported: split into multiple single-item requests (mind rate limits and cost)

### 2.3 `ImageGenerationPayload`

- `model: String`
- `prompt: String`
- `image: Option<MediaRef>` (img2img/edit)
- `mask: Option<MediaRef>` (edit)
- `n: Option<u32>`
- `size: Option<String>` (e.g., `1024x1024`)
- optional `quality/style/seed`
- optional `response_format` (recommend returning `MediaRef`, not forcing base64)

Key capabilities:

- `supports_image_input` (img2img)
- `supports_mask`
- `max_image_size`, `max_images_per_request`

Typical downgrades:

- if mask/edit is unsupported: downgrade to text-to-image or hard error

### 2.4 `SpeechToTextPayload` (ASR)

- `model: String`
- `audio: MediaRef`
- `language: Option<String>`
- `prompt: Option<String>`
- `timestamps: Option<bool>`
- `diarization: Option<bool>`
- `temperature: Option<f32>`

Key capabilities:

- `supports_timestamps`
- `supports_language`
- `supports_diarization`
- `max_audio_seconds`

Typical downgrades:

- if timestamps are unsupported: disable timestamps
- if audio is too long: chunking (requires explicit boundaries/merge strategy) or reject

### 2.5 `TextToSpeechPayload` (TTS)

- `model: String`
- `input: String`
- `voice: Option<String>`
- `format: Option<String>` (wav/mp3/pcm)
- `speed: Option<f32>`
- `ssml: Option<bool>`

Key capabilities:

- `supports_voices`
- `supported_formats`
- `supports_ssml`
- `max_chars`

Typical downgrades:

- unsupported voice: fallback to default
- SSML unsupported: reject or strip tags (must be explicit)

### 2.6 `RealtimeVoicePayload` (historical design, not implemented in the current runtime)

Realtime is a streaming session and should not be modeled as a single HTTP request/response. The payload should describe:

- `session_config` (model, voice, turn-taking, VAD, language, ...)
- `input_stream: StreamRef` (abstract)
- `output_events: EventSinkRef` (abstract)

Key capabilities:

- `supports_bidi_stream`
- `transport=websocket|grpc`
- `max_session_seconds`, `max_frame_bytes`

Typical downgrades:

- if realtime is unavailable: downgrade to `speech_to_text` (non-realtime) or reject

## 3. Capability and routing defaults (by operation)

### 3.1 Suggested `default_policy_by_operation`

- `chat_completions`: `ewma_latency` or `least_inflight`
- `embeddings`: `weighted_round_robin`
- `image_generation`: `priority + fallback` (high cost; no hedging by default)
- `speech_to_text`: `least_inflight` or `weighted_rr`
- `text_to_speech`: `weighted_rr`
- Planned only: `realtime_voice` would likely prefer `least_inflight` (not implemented in the current runtime)

### 3.2 Hedging/mirroring guidance

- Image: no hedging by default; mirroring is suitable for offline evaluation
- Realtime/ASR: tail-latency sensitive but costly; only enable light hedging with explicit budget

## 4. Hostcall family expansion

Today you have `cchat_*`. When adding other capabilities, there are two common paths:

Naming guidance:

- The family prefix should reflect the operation semantics; there is no need to force a single-letter prefix across all components.
- `cchat` means “completion chat”, so keeping `cchat_*` is reasonable; for other capabilities use semantics-aligned prefixes.

### Option A: per-operation hostcall families (clear semantics)

- `cchat_*`: completion chat
- `emb_*`: embeddings
- `img_*`: image (generation/edit/variation, etc.)
- `asr_*`: speech-to-text (ASR)
- `tts_*`: text-to-speech (TTS)
- `rt_*`: realtime (or reuse a stream subsystem)

### Option B: a unified `ai_call/transform` hostcall (faster extension)

One hostcall carries `operation + payload` and the host dispatches by `operation`.

Recommendation:

- Prefer A for ABI clarity and debuggability.
- Prefer B if you want a legacy-like `TransformRegistry` style and fastest iteration.

In both options, secrets and URLs must remain host-managed.
