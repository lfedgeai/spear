# SPEAR Console Voice Input (Press-and-Hold) Design

## 1. Goals

- Support a WeChat-like “press-and-hold to talk” UX in SPEAR Console: press to start capturing audio, release to commit.
- Send audio over the existing WebSocket (binary SSF frames) to the execution, and perform ASR (Speech-to-Text) inside the execution-side Agent, then feed the transcript into the chat flow.
- Keep a modular architecture: Console only does capture + transport; ASR and conversation logic live in the Agent.

## 2. Non-Goals

- No WebRTC.
- No “voice-specific protocol parsing/business logic” in SMS/spearlet gateways (keep binary passthrough + auth as-is).
- No backward compatibility: for each stream, the client must send CTRL(OPEN) before sending DATA/COMMIT.

## 3. Key Conventions

### 3.1 Stream ID Allocation

- `stream_id=1`: text chat input/output (bound to the existing Console chat window).
- `stream_id=2`: voice input uplink (Console → Agent).

Notes:
- `stream_id=2` carries voice input only. The transcript (text) and model answers are written back to `stream_id=1` to avoid adding a new rendering path in the UI.

### 3.2 SSF Frames and meta Encoding

- On-wire: each WebSocket binary message contains exactly one SSF v1 frame (see `docs/api/spear-hostcall/wasm-user-stream-bridge-en.md`).
- `meta`: UTF-8 JSON object, must not be an empty string; the minimal value is `{}`.
- `data`:
  - Text: UTF-8 bytes (TextEncoder/encodeUtf8).
  - Audio: PCM16LE bytes (little-endian), 16000Hz, mono in the current Console implementation.

## 4. Protocol: meta JSON Spec (Ready to Implement)

### 4.1 Common Top-Level Fields

Every `meta` JSON (CTRL/DATA/COMMIT) must be an object and may include:

- `v` (number, required): meta version, fixed to `1`.
- `kind` (string, required): `"open"` / `"utterance_begin"` / `"data"` / `"commit"`.
- `modality` (string, required): `"text"` / `"audio"`.
- `ts_ms` (number, optional): client-side timestamp in milliseconds.
- `trace_id` (string, optional): request identifier for end-to-end troubleshooting.

Receivers must ignore unknown fields.

### 4.2 Text Stream (`stream_id=1`)

#### 4.2.1 CTRL(OPEN)

- `msg_type=CTRL`
- `stream_id=1`
- `meta`:

```json
{
  "v": 1,
  "kind": "open",
  "modality": "text",
  "encoding": "utf-8",
  "ts_ms": 1710000000000
}
```

Constraint:
- Do not send DATA/COMMIT on `stream_id=1` before this CTRL.

#### 4.2.2 DATA (Text Chunks)

Notes:
- DATA/COMMIT semantics are defined by SSF `msg_type`. meta is only for correlation and debugging (e.g. `message_id`/`utterance_id`/`ts_ms`) and no longer requires `kind/modality`.

```json
{
  "v": 1,
  "message_id": "m_01",
  "seq_in_message": 1,
  "ts_ms": 1710000000100
}
```

Where:
- `data` is UTF-8 text bytes.
- A single message may be split into multiple DATA frames.

#### 4.2.3 COMMIT (Text Commit)

```json
{
  "v": 1,
  "message_id": "m_01",
  "ts_ms": 1710000000200
}
```

Semantics:
- Marks the end of `message_id`. The Agent can treat the accumulated text as one user input.

### 4.3 Voice Stream (`stream_id=2`)

#### 4.3.1 CTRL(OPEN)

- `msg_type=CTRL`
- `stream_id=2`
- `meta`:

```json
{
  "v": 1,
  "kind": "open",
  "modality": "audio",
  "audio": {
    "format": "pcm_s16le",
    "sample_rate_hz": 16000,
    "channels": 1
  },
  "ts_ms": 1710000000000
}
```

Constraint:
- Do not send DATA/COMMIT on `stream_id=2` before this CTRL.

#### 4.3.2 CTRL(UTTERANCE_BEGIN) (Press Start)

Send immediately when the user presses the button:

```json
{
  "v": 1,
  "kind": "utterance_begin",
  "modality": "audio",
  "utterance_id": "u_01",
  "ts_ms": 1710000001000
}
```

Semantics:
- Declares the beginning of a new utterance.
- The Agent uses it to create/reuse an ASR session and reset previous buffers.

#### 4.3.3 DATA (Audio Chunks)

Notes:
- DATA/COMMIT semantics are defined by SSF `msg_type`. meta is only for correlation and debugging (e.g. `utterance_id`/`chunk_index`/`ts_ms`) and no longer requires `kind/modality`.

```json
{
  "v": 1,
  "utterance_id": "u_01",
  "chunk_index": 1,
  "ts_ms": 1710000001020
}
```

Where:
- `data` is raw PCM16LE bytes (the current Console default is `chunkMs=300`; exact size remains a front-end choice).

#### 4.3.4 COMMIT (Release Commit)

Send on button release:

```json
{
  "v": 1,
  "utterance_id": "u_01",
  "ts_ms": 1710000002200
}
```

Semantics:
- Marks the end of the utterance. The Agent should flush ASR and wait for the final transcript.

## 5. Frontend (Console) Module Split and Interfaces

### 5.1 Proposed Directory Layout

Add under `web-console/src/`:

- `voice/`
  - `types.ts`: voice config and event types
  - `pcm16.ts`: Float32 → PCM16LE encoding
  - `micCapture.ts`: microphone capture (WebAudio) producing PCM16 chunks
  - `voiceStreamSender.ts`: wrap PCM16 chunks + meta into SSF and send to `stream_id=2`
  - `PressToTalkButton.tsx`: press-and-hold button component
  - `index.ts`: exports

### 5.2 Core Interfaces (TS Pseudocode)

#### 5.2.1 `MicCapture`

```ts
export type MicConfig = {
  sampleRateHz: 16000
  channels: 1
  chunkMs: number
}

export type MicChunk = {
  tsMs: number
  pcm16le: Uint8Array
}

export interface MicCapture {
  start(): Promise<void>
  stop(): Promise<void>
  onChunk(cb: (c: MicChunk) => void): () => void
}
```

Constraints:
- After `start()`, `onChunk` must keep firing until `stop()`.
- Failures must throw and be surfaced to UI (permission denied, missing device, etc.).

#### 5.2.2 `VoiceStreamSender`

```ts
export type VoiceSession = {
  utteranceId: string
  chunkIndex: number
}

export interface VoiceStreamSender {
  ensureStreamOpen(): void
  beginUtterance(nowMs: number): VoiceSession
  sendChunk(session: VoiceSession, chunk: MicChunk): void
  commitUtterance(session: VoiceSession, nowMs: number): void
}
```

Constraints:
- `ensureStreamOpen()` must send CTRL(OPEN) for `stream_id=2` if not sent yet.
- `beginUtterance()` must send CTRL(UTTERANCE_BEGIN) and return a session.

#### 5.2.3 `PressToTalkButton`

```ts
export type PressToTalkButtonProps = {
  disabled: boolean
  onPressStart: () => void
  onPressEnd: () => void
}
```

Event policy:
- `pointerdown` → `onPressStart`
- `pointerup | pointercancel | pointerleave` → `onPressEnd`

### 5.3 Concrete Change List (Console)

#### 5.3.1 Update: `web-console/src/spearStream.ts`

- Extend `connect()` option from `autoOpenStreamId?: number` to `autoOpenStreamIds?: number[]`, and send CTRL(OPEN) for each stream after WS opens.
- By default, open `stream_id=1` and `stream_id=2` on successful connect.

#### 5.3.2 Update: `web-console/src/App.tsx`

- Add `PressToTalkButton` in the input area.
- After connecting, initialize `MicCapture` and `VoiceStreamSender`, and surface state into UI:
  - `micPermissionState`
  - `isRecording`
  - `lastVoiceError`

#### 5.3.3 Add: `web-console/src/voice/*`

- Add files described in 5.1.

## 6. Agent State Machine and Implementation Path

### 6.1 IO Contract

- The Agent must open and listen to:
  - `stream_id=1`: text input (DATA/COMMIT) and output (write DATA back).
  - `stream_id=2`: voice input (CTRL/DATA/COMMIT).
- The Agent may write both “ASR transcript” and “LLM answers” back to `stream_id=1` (to reuse the existing Console rendering).

### 6.2 Voice State Machine (Idle / Recording / Committing)

#### 6.2.1 States

- `Idle`: no active utterance; wait for `CTRL(kind=utterance_begin)`.
- `Recording`: an utterance is active; keep receiving audio DATA and write them into ASR.
- `Committing`: on COMMIT, trigger ASR flush, read final transcript, feed it into chat, then return to `Idle`.

#### 6.2.2 Transitions

- `Idle` + `utterance_begin` → `Recording`
- `Recording` + `DATA(audio)` → `Recording`
- `Recording` + `COMMIT(audio)` → `Committing`
- `Committing` + `asr_done` → `Idle`
- Any + `session_closed/error` → close fds and exit (MVP can exit directly)

### 6.2.3 Current Sample Compatibility Notes

- `user_stream_live_caption` and `user_stream_voice_chat` accept both `input_audio_transcription.*` and `conversation.item.input_audio_transcription.*` RTASR event names.
- `user_stream_live_caption` also treats both `speech_started` and `input_audio_buffer.speech_started` as utterance-reset events.
- Press-and-hold release is implemented as a voice COMMIT on `stream_id=2`; the execution-side samples flush RTASR on that COMMIT boundary.

### 6.3 Recommended MVP Landing: WASM C Agent

Rationale:
- `rtasr_*` hostcalls already exist in the C SDK and the wasm ABI, so it can be used without additional language bindings.

Current sample landing:

- `samples/wasm-c/user_stream_voice_chat.c`: implements the state machine and rtasr/cchat calls
- `samples/wasm-c/user_stream_live_caption.c`: implements the voice-to-transcript path without the downstream chat step
- `samples/wasm-js/user_stream_voice_chat/src/entry.mjs`: JS-first version of the voice-chat flow
- `samples/wasm-js/user_stream_live_caption/src/entry.mjs`: JS-first version of the live-caption flow
- `samples/wasm-rust/user_stream_live_caption/src/main.rs`: Rust-first version of the live-caption flow, built around Rust SDK `epoll` wrappers and strongly typed modules
- `samples/wasm-rust/user_stream_voice_chat/src/main.rs`: Rust-first version of the voice-chat flow, built around Rust SDK `epoll` wrappers, RTASR helpers, and a downstream chat-completion adapter

MVP implementation notes:
- First, validate “voice → text” only: after voice COMMIT, write the final transcript back to `stream_id=1` (no LLM).
- Next, wire `cchat_*`: send the transcript as a user message, and write model output back to `stream_id=1`.

## 7. Minimal Working Path (MVP)

### Stage A: Transport + UI (No ASR)

- Frontend:
  - Send CTRL(OPEN), CTRL(UTTERANCE_BEGIN), DATA(audio), COMMIT(audio) on `stream_id=2`.
  - Show recording state and permission errors.
- Agent:
  - Only count/echo received audio DATA/COMMIT to confirm framing and segmentation correctness.

### Stage B: Add ASR (`rtasr_fd`)

- In `Recording`, write each audio chunk via `rtasr_write`.
- In `Committing`, flush and read the final transcript.
- For the JS live-caption sample, aggregate several PCM chunks before `rtasr.writeAudio()` to reduce JS/WASM bridge overhead.
- Write transcript back to `stream_id=1`.

### Stage C: Feed Transcript into Chat

- Treat the transcript as one text input and reuse the existing chat completion logic, writing the model answer back to `stream_id=1`.

## 8. Testing and Acceptance

- Manual acceptance (MVP):
  - After mic permission is granted, press and hold for 1–3 seconds; on release, see the transcript (Stage B) or model reply (Stage C) in the chat window.
  - If permission is denied, UI must show a clear error and must not send audio frames.
- Protocol strictness:
  - If DATA/COMMIT is sent before CTRL(OPEN), the Agent must reject and close the stream (no backward compatibility).

## 9. SDK Helpers (SSF + CTRL meta)

To make it easier for WASM guests to handle CTRL/DATA/COMMIT, this repo provides SDK helpers for “zero-copy frame split” and “meta JSON parsing”:

- C SDK:
  - SSF v1 frame parsing/splitting: `sdk/c/include/spear_ssf.h` (`sp_ssf_v1_parse_header` / `sp_ssf_v1_split`)
  - meta substring matching: `sp_ssf_meta_contains` (best-effort; prefer a real JSON parser when available)
- Rust SDK (WASM guest):
  - meta v1 parsing: `spear_wasm::ssf_meta::{parse_ctrl_meta_v1, parse_data_meta_v1}`
  - strict mode: `parse_data_meta_v1` rejects DATA/COMMIT meta that carries `kind/modality/audio`
- Boa JS runtime (WASM-JS sample):
  - meta v1 parsing: `import * as ssf from "spear/ssf"; ssf.parseCtrlMetaV1(...) / ssf.parseDataMetaV1(...)`

## 10. Security and Privacy

- Do not log raw audio bytes in Console.
- Stream session tokens are short-lived; do not display or copy them in the UI.
- Keep the same auth boundary as text: Console → SMS (auth) → spearlet (intra-network).
