# Realtime/Streaming Subsystem Design Notes

Realtime voice/streaming differs from standard HTTP requests: it requires dedicated transport (WebSocket/gRPC), session lifecycle, and event-driven incremental outputs.

Note: the current runtime does not implement `realtime_voice`; the only shipped realtime capability today is `speech_to_text`. The following content is kept as historical design / future planning reference.

## 1. Why it needs a separate model

The legacy Go realtime ASR uses a WebSocket session and appends audio buffers; delta events are forwarded back to the task (`legacy/spearlet/stream/rt_asr.go:46-140`). This cannot be represented as a single request/response.

Therefore model realtime explicitly as:

- planned `Operation::realtime_voice`
- `Feature::supports_bidi_stream`
- `Transport::websocket|grpc`

Implement it as a dedicated stream subsystem, while sharing the same registry/capabilities and routing constraints.

## 2. Suggested lifecycle

At minimum:

- `create_session`
- `append_audio` (or `send_input_event`)
- `commit` (end of an input turn)
- `close_session`

Outputs should be an event stream:

- `delta`
- `completed`
- `error`

## 3. Routing and capability constraints

Routing must require:

- if reintroduced, the backend instance must support `realtime_voice`
- transport supports `websocket|grpc`
- session concurrency and duration constraints (`max_session_seconds`)

If no candidate exists:

- return `MissingCapabilities`
- if allowed, downgrade to `speech_to_text` (non-realtime) or reject

## 4. Relationship to hostcalls

Two common shapes:

- reuse a “stream ctrl” hostcall style (closer to legacy `MethodStreamCtrl`)
- design a dedicated `rt_*` hostcall family

In both cases, secrets and URLs remain host-managed.
