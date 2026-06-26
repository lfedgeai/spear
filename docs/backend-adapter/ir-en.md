# Spear Canonical IR (Multi-Operation / Multi-Modal)

This document specifies the recommended “canonical intermediate representation (IR)” used by the Spear adapter layer. It is the stable contract between hostcall/session inputs and backend-specific requests.

## 1. Goals

- Support multiple operations: Chat, Embeddings, Image Generation, ASR, TTS, Realtime Voice.
- Align with the OpenAI ecosystem: maximize pass-through compatibility to OpenAI-compatible backends.
- Be extensible: allow backend-specific non-standard fields without freezing the IR too early.
- Be routable: explicitly carry capability requirements and routing hints.
- Be evolvable: versioned IR with incremental schema coverage.

## 2. Shape: Unified Envelope + Per-Operation Payload

All AI-related hostcalls should normalize into a unified request envelope:

```text
CanonicalRequestEnvelope
  - meta
  - routing
  - requirements
  - policy
  - payload (oneof)
```

### 2.1 CanonicalRequestEnvelope (recommended fields)

- `version: u32`
- `request_id: String` (or u64)
- `task_id: Option<String>`
- `operation: Operation`
- `meta: RequestMeta`
  - `trace_id/span_id` (optional)
  - `created_at_ms` (optional)
- `routing: RoutingHints`
  - `backend: Option<String>`
  - `backend_allowlist/denylist: Option<Vec<String>>`
  - optional `region_hint/cost_hint/latency_hint`
- `requirements: Requirements`
  - `required_ops: Set<Operation>` (often equals `operation`, but can be composed)
  - `required_features: Set<Feature>`
  - `required_transports: Set<Transport>`
  - optional `preferred_backends: Vec<String>`
- `policy: Option<SelectionPolicySpec>` (request override, constrained by host config)
- `timeout_ms: Option<u64>`
- `payload: oneof { ChatCompletionsPayload | EmbeddingsPayload | ImageGenerationPayload | SpeechToTextPayload | TextToSpeechPayload | ... }`
- `extra: Map<String, Value>` (unknown/experimental passthrough)

Notes:

- `payload` lives inside the envelope and should align with `operation` (typically 1:1). Payload skeletons are defined in `operations-en.md`.
- Large binaries should not be embedded directly into JSON fields; use `MediaRef` (e.g., `sms_file`) to reference data locations (see Section 3).

### 2.2 CanonicalResponseEnvelope (recommended fields)

Responses should also be wrapped for uniform consumption, observability, and error handling:

- `version: u32`
- `request_id: String`
- `operation: Operation`
- optional `backend: BackendInfo`
  - `name/instance`
  - `latency_ms`
  - `attempts` (retries/hedging)
- `result: oneof { payload, error }`
- `raw: Option<Value>` (optional; gated for debug/replay)

### 2.3 Error shape (recommended)

- `code: String` (e.g., `BackendNotEnabled`/`NoCandidateBackend`/`MissingCapabilities`/`Timeout`)
- `message: String`
- `retryable: bool`
- `operation: Operation`
- optional `required: RequirementsSummary`
- optional `candidates_checked: Vec<String>`
- optional `rejected_reasons: Map<String, String>`

## 3. MediaRef: Unified references for image/audio payloads

Multimodal operations frequently carry large binary data. Use `MediaRef` to indicate where data lives, instead of embedding bytes in JSON.

### 3.1 Recommended MediaRef variants

- `inline_base64`: good for small data and debugging
  - `{ "kind": "inline_base64", "mime": "audio/wav", "data": "..." }`
- `sms_file`: recommended production default for large data
  - `{ "kind": "sms_file", "uri": "smsfile://<id>", "mime": "image/png" }`
- `http_url`: only if explicitly allowed by host security policy
  - `{ "kind": "http_url", "url": "https://...", "mime": "image/jpeg" }`

### 3.2 Where MediaRef is used in the IR (recommended)

`MediaRef` should be the unified type for “media fields” in the Canonical IR. It commonly appears in:

- Request-side (input media):
  - `speech_to_text`: `SpeechToTextPayload.audio: MediaRef`
  - `image_generation`: `ImageGenerationPayload.image: Option<MediaRef>`, `mask: Option<MediaRef>`
  - Multimodal `chat_completions`: `Message.content_parts[]` image/audio parts use `MediaRef`
- Response-side (output media):
  - `image_generation`: return `images: Vec<MediaRef>`
  - `text_to_speech`: return `audio: MediaRef`

Avoid embedding large binaries directly in response JSON; prefer returning `sms_file` and let callers fetch on demand.

### 3.3 Normalization guidance

- If hostcall input provides raw bytes, store them in object storage and emit `sms_file` to avoid memory spikes.
- If a backend requires inline content, the adapter can read and encode on demand; the IR should not force inline.

### 3.4 Examples

#### 3.4.1 ASR (`speech_to_text`) request: audio as `sms_file`

```json
{
  "version": 1,
  "request_id": "req-123",
  "operation": "speech_to_text",
  "requirements": {
    "required_ops": ["speech_to_text"],
    "required_features": ["supports_audio_input"],
    "required_transports": ["http"]
  },
  "payload": {
    "model": "whisper-1",
    "audio": {
      "kind": "sms_file",
      "uri": "smsfile://01JFK...ABCD",
      "mime": "audio/wav"
    },
    "language": "zh",
    "timestamps": true
  }
}
```

#### 3.4.2 Image generation (`image_generation`) response: image as `sms_file`

```json
{
  "version": 1,
  "request_id": "req-456",
  "operation": "image_generation",
  "backend": { "name": "openai-us", "instance": "openai-us-1", "latency_ms": 842 },
  "result": {
    "payload": {
      "images": [
        { "kind": "sms_file", "uri": "smsfile://01JFK...WXYZ", "mime": "image/png" }
      ]
    }
  }
}
```

#### 3.4.3 Multimodal chat: message content references an image

```json
{
  "operation": "chat_completions",
  "payload": {
    "model": "gpt-4o",
    "messages": [
      {
        "role": "user",
        "content_parts": [
          { "type": "text", "text": "Describe this image" },
          {
            "type": "image",
            "image": { "kind": "sms_file", "uri": "smsfile://01J...", "mime": "image/jpeg" }
          }
        ]
      }
    ]
  }
}
```

### 3.5 Normalization and resolution responsibilities (recommended)

- Normalize layer (host-side):
  - If hostcall input is raw bytes, store it in object storage and emit a `MediaRef` of kind `sms_file`.
  - Enforce security policy for `http_url` (reject or re-host into `sms_file` if disallowed).
- Backend adapter (before sending):
  - Resolve `MediaRef` into bytes (fetch `sms_file`, decode `inline_base64`) and build multipart/base64/binary payloads as required.
  - If a backend returns base64 image/audio, prefer storing into `sms_file` before producing the canonical response (unless inline is explicitly requested and size-limited).
- Router layer:
  - Does not parse media bytes, but can enforce capability/limit checks using metadata (e.g., `mime`, size/duration) such as `max_audio_seconds`.

## 4. Payload evolution strategy

To avoid freezing all fields too early:

- Strongly type the backbone fields of each `*Payload`.
- Put backend-specific parameters into `extra`.
- Handle capability gaps and downgrades explicitly in router/adapters (no silent drops inside IR).

See `operations-en.md` for per-operation payload field recommendations and capability mappings.
