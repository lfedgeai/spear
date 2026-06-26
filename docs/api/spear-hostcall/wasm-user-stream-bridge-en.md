# Spear Hostcall Engineering Design: Bidirectional User Stream Bridge for WASM (v1)

## 0. Purpose

This document proposes a best-practice design to support **bidirectional streaming input/output** between an external client and a Spear WASM instance, with:

- A **network-facing transport** (WebSocket first; gRPC streaming optional) suitable for browsers and realtime media.
- A **future-proof binary frame format** that can carry raw bytes plus typed metadata (audio/video/text/etc).
- A **WASM-facing I/O model** that is deterministic, non-reentrant, and scalable: **fd + non-blocking read/write + epoll readiness**.

This design intentionally builds on Spear’s existing fd/epoll subsystem and conventions:

- General fd/epoll subsystem: [fd-epoll-subsystem-en.md](./fd-epoll-subsystem-en.md)
- Realtime ASR as a streaming fd example: [realtime-asr-epoll-en.md](./realtime-asr-epoll-en.md)

---

## 1. Goals / Non-goals

### 1.1 Goals

- **Bidirectional streaming** between client ⇄ spearlet ⇄ WASM guest.
- **Protocol compatibility**: versioned frames, extensible headers, typed metadata for audio/video and future modalities.
- **Backpressure**: bounded queues and explicit flow control; no unbounded buffering.
- **WASM-friendly semantics**: non-blocking `read/write` with `-EAGAIN`, and epoll to avoid busy loops.
- **Security boundaries**: WASM never directly sees network credentials; host enforces authz, quotas, limits.

Note: `EAGAIN/EBADF/...` refers to **SPEAR virtual errno constants** (aligned with the C/Rust WASM SDK, e.g. `SPEAR_EAGAIN=11`), not the host OS `libc` errno values. This avoids macOS vs Linux errno number mismatches that can break guest-side error handling.

### 1.2 Non-goals (v1)

- Full WASI sockets compatibility.
- WebRTC (datachannel/media) transport.
- Multi-tenant cross-execution stream fanout (v1 is per execution/session).

---

## 2. High-level Architecture

### 2.1 Data flow

Client (browser/service) connects to the stream gateway and binds to an execution’s stream session:

1. Client → SMS (HTTP): creates a short-lived stream session token via `POST /api/v1/executions/{execution_id}/streams/session`.
2. Client → SMS (WebSocket): connects to `GET /api/v1/executions/{execution_id}/streams/ws?token=...` and sends SSF frames.
3. SMS: validates token and proxies the WebSocket to the target spearlet.
4. Spearlet: routes inbound SSF frames into the execution-local stream hub and exposes them to WASM via fd/epoll hostcalls.
3. WASM guest: uses `user_stream_*` hostcalls (fd-based) to `read/write` messages and `spear_epoll_*` to wait for readiness.
5. Spearlet → SMS → Client (WebSocket): forwards outbound frames written by WASM.

### 2.2 Why “fd + epoll” is the recommended WASM side model

### 2.3 Rust SDK and Boa JS usage

- Rust: `sdk/rust/crates/spear-wasm` provides safe wrappers for `user_stream_*`, `user_stream_ctl_*`, and `spear_epoll_*`.
- Rust helper layer: `sdk/rust/crates/spear-wasm-helper` provides reusable `ManagedStream`, SSF/user-stream protocol parsing, and text-output buffering for Rust-first guest apps.
- Boa JS: `sdk/rust/crates/spear-boa` exposes `Spear.userStream` so JS code can open/read/write streams without manually dealing with raw pointers.
- Boa JS helper modules: `spear/time_format`, `spear/rtasr_event`, and `spear/stream_gate` provide reusable timestamp, RTASR-event, and user-stream gating helpers for JS-first guest apps.

Two options exist for delivering inbound data to a WASM instance:

- **(A) Host calls into guest (“callback”)** whenever a message arrives.
- **(B) Guest pulls via `read/write`**, using epoll to wait.

Best practice recommendation: **(B)**.

Reasons:

- Avoids **re-entrancy** and “host-in-the-middle-of-guest” call stacks (hard to make safe/deterministic).
- Matches existing Spear hostcall conventions (`rtasr_*`, `mic_*`, `spear_fd_ctl`, `spear_epoll_*`).
- Enables **single-threaded guest event loops** to multiplex multiple streams predictably.

---

## 3. Transport: WebSocket (Primary) + gRPC Streaming (Optional)

### 3.1 WebSocket as primary

Why WebSocket:

- Browser-native bidirectional binary transport.
- Works well for realtime audio/video chunks and incremental outputs.
- Simpler deployment than custom HTTP streaming for full duplex.

Recommended WebSocket properties:

- Path (gateway, recommended): `GET /api/v1/executions/{execution_id}/streams/ws?token=...`
- Stream session creation (gateway, recommended): `POST /api/v1/executions/{execution_id}/streams/session`
- Path (spearlet, internal): `GET /api/v1/executions/{execution_id}/streams/ws`
- Subprotocol: `Sec-WebSocket-Protocol: spear.stream.v1`
- Frames: **binary only** for data plane; optional text frames for debugging (not required).

### 3.2 Authentication / authorization

Best practice:

- Use the same auth mechanism as the existing HTTP/gRPC gateway (token/cookie/mtls), but enforce **execution-level authorization**.
- Require a short-lived **stream session token** for the public-facing gateway endpoint.

Handshake options:

- `Authorization: Bearer ...` HTTP header during upgrade.
- Or `?token=...` query parameter when headers are not feasible (less preferred; must be short-lived).

### 3.3 gRPC streaming (optional alternative)

For service-to-service use cases (non-browser), gRPC bidi stream may be preferable.

If added, mirror the same logical messages and flow control used by the WS format. The on-wire frame format can remain identical by carrying `bytes frame`.

---

## 4. On-wire Framing Protocol: Spear Stream Frame (SSF)

### 4.1 Why not “raw bytes only”

Raw bytes are insufficient for long-term evolution:

- No versioning → breaking changes are hard.
- No message types → cannot represent OPEN/CLOSE/ACK/ERROR cleanly.
- No typed metadata → audio/video codecs, timestamps, or content-type require ad-hoc conventions.

Therefore v1 adopts a compact binary envelope: **Spear Stream Frame (SSF)**.

### 4.2 SSF v1 frame layout (little-endian)

Each WebSocket binary message contains exactly one SSF frame.

Header (fixed 32 bytes):

| Offset | Size | Field | Meaning |
|---:|---:|---|---|
| 0 | 4 | `magic` | ASCII `"SPST"` (`0x53505354`) |
| 4 | 2 | `version` | `1` |
| 6 | 2 | `header_len` | `32` (future extensible) |
| 8 | 2 | `msg_type` | see below |
| 10 | 2 | `flags` | bitset |
| 12 | 4 | `stream_id` | logical stream within the session |
| 16 | 8 | `seq` | sender sequence number (per direction + stream_id) |
| 24 | 4 | `meta_len` | bytes of metadata section |
| 28 | 4 | `data_len` | bytes of data section |

Body:

- `meta` bytes (length = `meta_len`)
- `data` bytes (length = `data_len`)

Constraints:

- `header_len` MUST be at least 32; receivers MUST ignore unknown header extension bytes when `header_len > 32`.
- `meta_len + data_len` MUST equal WS payload size minus `header_len`.

### 4.3 Message types (v1)

Current implementation requires only a valid SSF v1 header (magic/version/header_len) and uses `stream_id` to route frames. `msg_type` is parsed and preserved for the guest but not interpreted by the host.

Recommended v1 conventions:

- `1 = CTRL`:
  - Control frames (e.g. OPEN/keepalive).
  - `meta`: JSON UTF-8 (optional).
  - `data`: usually empty.
- `2 = DATA`:
  - Data frames (text/binary payload).
  - `meta`: JSON UTF-8 (optional).
  - `data`: raw bytes (audio/video/text or any binary).
- `3 = COMMIT`:
  - Marks the end of a user input segment (application-level).
  - `meta`: JSON UTF-8 (optional).
  - `data`: usually empty.

Other message types (CLOSE/ACK/ERROR/PING/PONG/etc.) are reserved for future extensions.

### 4.3.1 SDK constants

- C: `sdk/c/include/spear_ssf.h` provides `SPEAR_SSF_MSG_TYPE_CTRL/DATA/COMMIT`.
- Rust (guest): `spear_wasm::ssf::SsfMsgType`.
- JS (web-console): `web-console/src/ssf/index.ts` exports `SsfMsgType` and `encodeSsfV1Frame/decodeSsfV1Frame`.
- JS (Boa runtime): `import * as ssf from "spear/ssf"` exports `MsgType`.

### 4.4 Metadata conventions

Metadata encoding in v1: **JSON UTF-8** (for debuggability and interop).

Recommended keys (examples):

- For `OPEN`:
  - `session_id` (string)
  - `content_type` (string, e.g. `audio/pcm;rate=16000;channels=1`)
  - `codec` (string, e.g. `pcm_s16le`, `opus`, `h264`)
  - `timebase` (object, e.g. `{ "unit": "ms" }`)
  - `limits` (object, server-provided effective limits)
- For `DATA`:
  - `timestamp_ms` (number)
  - `duration_ms` (number)
  - `is_keyframe` (bool, for video)

Receivers MUST ignore unknown keys.

---

## 5. Backpressure and Flow Control

### 5.1 Host-side bounded queues (required)

For each `(execution_id, stream_id, direction)`:

- `recv_queue` for inbound client→guest messages
- `send_queue` for guest→client messages

Each queue is bounded by:

- `max_queue_bytes`
- `max_frame_bytes`
- `max_frames`

Overflow policy (recommended defaults):

- Inbound (client→guest): reject with `ERROR` and close (protects execution determinism).
- Outbound (guest→client): apply `-EAGAIN` backpressure to guest writes.

### 5.2 Credit-based flow control (recommended)

To prevent a fast sender from overrunning the receiver even when TCP buffers absorb bursts, SSF v1 includes an explicit credit mechanism via `ACK`:

- Each direction maintains `credit_bytes`.
- Sender MUST NOT send `DATA` whose total outstanding bytes exceed `credit_bytes`.
- Receiver sends `ACK` to grant more credit:
  - `meta`: `{ "grant_bytes": <u64>, "ack_seq": <u64> }`

A minimal deployment may:

- Use bounded queues + `-EAGAIN` backpressure (implemented).
- Add `ACK`-based credit flow control later if needed (not implemented in current codebase).

### 5.3 Guest-visible backpressure

Guest observes backpressure via:

- `user_stream_write(...) -> -EAGAIN` when send queue is full (and fd is non-blocking).
- `EPOLLOUT` readiness toggling when capacity becomes available.

---

## 6. WASM-facing ABI (Hostcalls)

This section defines a new hostcall family **`user_stream_*`** that integrates with the unified fd/epoll subsystem.

### 6.1 Handle model

- Guest-visible handle is `i32 fd`.
- `fd` entries are stored in the unified `FdTable`.
- Readiness is exposed through `spear_epoll_*`.
- Generic controls use `spear_fd_ctl` (flags/status/metrics). See [fd-epoll-subsystem-en.md](./fd-epoll-subsystem-en.md).

### 6.2 Signatures (v1)

Create/open:

- `user_stream_open(stream_id: i32, direction: i32) -> i32`
- `user_stream_ctl_open() -> i32`

I/O:

- `user_stream_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32`
- `user_stream_write(fd: i32, buf_ptr: i32, buf_len: i32) -> i32`
- `user_stream_ctl_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32`

Close:

- `user_stream_close(fd: i32) -> i32`

Constants:

- `direction`:
  - `USER_STREAM_DIR_IN = 1` (client → guest)
  - `USER_STREAM_DIR_OUT = 2` (guest → client)
  - `USER_STREAM_DIR_BIDI = 3`

Return values:

- `>= 0`: success (bytes written for write; bytes returned for read; or `0` for close/open)
- `< 0`: `-errno` (e.g. `-EAGAIN`, `-EBADF`, `-EINVAL`, `-ENOTCONN`, `-EPIPE`)

### 6.3 Read/write payload contract

To minimize cross-boundary translation and preserve forward compatibility:

- `user_stream_read` returns **one SSF frame** (binary) per call.
- `user_stream_write` expects **one SSF frame** (binary) per call.

Host responsibilities:

- Validate `magic/version/header_len`.
- Enforce `max_frame_bytes`.
- Enforce directionality (IN fds reject write; OUT fds reject read).
- Update fd readiness and notify epoll watchers on queue state changes.

Guest responsibilities:

- Provide an output buffer and use the “buffer-too-small” convention:
  - if `*out_len_ptr < need`, host writes back `need` and returns `-ENOSPC`.

This matches the conventions already used by other hostcalls and `spear_epoll_wait` output sizing.

### 6.4 Epoll readiness semantics for UserStreamFd

Readiness bits:

- `EPOLLIN`: inbound queue non-empty (`user_stream_read` will succeed)
- `EPOLLOUT`: outbound queue has capacity (`user_stream_write` will succeed)
- `EPOLLERR`: session/transport error
- `EPOLLHUP`: peer closed or fd closed

Level-triggered rule:

- As long as the condition holds, `spear_epoll_wait` reports it.

### 6.5 Guest usage pattern (single-threaded event loop)

Recommended pattern:

1. (Optional) Discover available streams:
   - `ctl_fd = user_stream_ctl_open()`
   - register `ctl_fd` with epoll for `EPOLLIN`
   - `user_stream_ctl_read` returns an 8-byte event: `(u32 stream_id, u32 kind)`
2. Create data fds:
   - `in_fd = user_stream_open(stream_id, USER_STREAM_DIR_IN)`
   - `out_fd = user_stream_open(stream_id, USER_STREAM_DIR_OUT)` (or BIDI)
2. Register with epoll:
   - `epfd = spear_epoll_create()`
   - `spear_epoll_ctl(epfd, ADD, in_fd, EPOLLIN)`
   - `spear_epoll_ctl(epfd, ADD, out_fd, EPOLLOUT)`
3. Loop:
   - `spear_epoll_wait(epfd, ...)`
   - drain readable fds until `-EAGAIN`
   - write as capacity allows; stop on `-EAGAIN`

---

## 7. Host-side Implementation Design (Rust, function-level)

This section describes the intended placement and function-level responsibilities. Names are indicative and should follow existing module patterns under `src/spearlet/execution/`.

### 7.1 Data structures

Add a new fd kind:

```rust
pub enum FdKind {
    // ...
    UserStream,
}
```

And a new fd inner state:

```rust
pub struct UserStreamState {
    pub stream_id: u32,
    pub direction: UserStreamDirection,
    pub conn_state: UserStreamConnState,

    pub recv_queue: std::collections::VecDeque<Vec<u8>>,
    pub recv_queue_bytes: usize,

    pub send_queue: std::collections::VecDeque<Vec<u8>>,
    pub send_queue_bytes: usize,

    pub limits: UserStreamLimits,
    pub metrics: UserStreamMetrics,
    pub last_error: Option<String>,
}
```

### 7.2 WebSocket session manager

Implementation location:

- `src/spearlet/http_gateway.rs` (`user_stream_ws_loop`)

Key responsibilities:

- Upgrade HTTP to WebSocket; enforce subprotocol `spear.stream.v1`.
- Parse inbound WS binary messages as SSF frames.
- Route frames to `(execution_id, stream_id)` inbound queues.
- Drain outbound queues and send WS frames to the client.
- Handle close/error and propagate `EPOLLHUP/EPOLLERR`.

Function-level design (indicative):

```rust
pub async fn handle_user_stream_ws(
    state: AppState,
    execution_id: String,
    ws: WebSocketUpgrade,
) -> impl IntoResponse;

async fn ws_loop(
    hub: Arc<ExecutionUserStreamHub>,
    ws: axum::extract::ws::WebSocket,
) -> Result<(), UserStreamWsError>;
```

### 7.3 Execution-local stream hub

Purpose: decouple the WS transport from fd table entries and allow controlled attachment.

Implementation location:

- `src/spearlet/execution/host_api/user_stream.rs` (`ExecutionUserStreamHub`)

Core API:

```rust
pub struct ExecutionUserStreamHub {
    pub execution_id: String,
    pub streams: dashmap::DashMap<u32, StreamChannel>,
    pub authz: UserStreamAuthz,
    pub limits: UserStreamLimits,
}

pub struct StreamChannel {
    pub inbound: StreamQueue,
    pub outbound: StreamQueue,
    pub watchers: StreamWatchers,
    pub conn_state: UserStreamConnState,
}

impl ExecutionUserStreamHub {
    pub fn attach_ws(&self, stream_id: u32, ws_peer: WsPeerInfo) -> Result<(), UserStreamError>;
    pub fn detach_ws(&self, reason: UserStreamCloseReason);

    pub fn push_inbound_frame(&self, stream_id: u32, frame: Vec<u8>) -> Result<(), UserStreamError>;
    pub fn pop_outbound_frame(&self, stream_id: u32) -> Option<Vec<u8>>;
}
```

### 7.4 Hostcall glue (WASM imports)

Recommended integration points:

- `src/spearlet/execution/runtime/wasm_hostcalls.rs`:
  - add `user_stream_open/read/write/close`
  - reuse existing linear memory helper conventions (`mem_read`, `mem_write_with_len`)

Indicative hostcall implementations:

```rust
pub fn user_stream_open(host: &mut DefaultHostApi, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
pub fn user_stream_read(host: &mut DefaultHostApi, instance: &mut Instance, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
pub fn user_stream_write(host: &mut DefaultHostApi, instance: &mut Instance, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
pub fn user_stream_close(host: &mut DefaultHostApi, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
```

Within `DefaultHostApi` (indicative):

```rust
impl DefaultHostApi {
    pub fn user_stream_open(&self, stream_id: i32, direction: i32) -> i32;
    pub fn user_stream_read(&self, fd: i32) -> Result<Vec<u8>, i32>;
    pub fn user_stream_write(&self, fd: i32, bytes: &[u8]) -> i32;
    pub fn user_stream_close(&self, fd: i32) -> i32;
}
```

### 7.5 Readiness recomputation and notification

Follow the existing fd/epoll best practice:

- Any queue/state transition MUST:
  - recompute `poll_mask` for the fd entry
  - notify watchers (`fd_table.notify_watchers(fd)`)

---

## 8. Failure Handling

### 8.1 Disconnect behavior

- Client WS close:
  - inbound direction: set `EPOLLHUP` on IN fds; further reads drain remaining queued frames then return EOF policy (`0` or `-EPIPE`, choose consistently).
  - outbound direction: further writes return `-EPIPE`.

### 8.2 Error mapping

Recommended `-errno` mapping:

- Parse error / invalid frame: `-EINVAL` (and emit SSF `ERROR`)
- Auth failure: `-EACCES` (close)
- Not bound to an execution: `-ENOTCONN`
- Queue full: `-EAGAIN` for guest writes; for client inbound, close with `ERROR` to keep determinism.

---

## 9. Observability (Required)

Expose metrics per execution and per stream_id:

- `in_frames_total`, `out_frames_total`
- `in_bytes_total`, `out_bytes_total`
- `dropped_frames_total` (by reason)
- `queue_bytes`, `queue_len`
- `ws_disconnects_total`, `errors_total`

Expose `spear_fd_ctl(..., GET_STATUS/GET_METRICS, ...)` JSON for debugging.

---

## 10. Testing Plan (Recommended)

- Unit tests:
  - SSF parse/validate (magic/version/header_len; length checks)
  - Queue bounds and overflow policies
  - Readiness transitions (IN/OUT/ERR/HUP) with watcher notifications
- Integration tests:
  - WS client pushes DATA; guest reads via `user_stream_read` and epoll
  - Guest writes; WS client receives frames
  - Backpressure: guest write returns `-EAGAIN` when outbound queue full
  - Disconnect propagates `EPOLLHUP`

Document version: v1 (2026-03-18).
