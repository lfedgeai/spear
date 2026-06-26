# Rust SDK (Boa JS → WASM → Spear) Detailed Design (EN)

## 1. Background and Pain Points

Spear’s WASM-side capabilities are currently exposed primarily via hostcalls under `import_module("spear")`, and the repo already ships a C header SDK (`../sdk/c/include/spear.h`) to make WASM-C usage easier.

For JavaScript workloads, a common path is to embed a JS engine in Rust (Boa), compile the Rust program to a WASI target (today: `wasm32-wasip1`), and run it on Spear’s WASM runtime.

Without an SDK, users typically pay three costs:
- Runtime cost: setting up Boa, module loading, a minimal event loop, error/exit conventions.
- Hostcall cost: pointer/length/buffer management, errno semantics, FD lifecycle, epoll usage.
- JS ergonomics cost: hostcalls feel like system APIs, not JS patterns (Promise, options objects, AsyncIterator, Abort).

This document proposes adding a Rust-based SDK to the Spear repo that provides a “Boa JS runtime compiled to WASM + JS-friendly Spear APIs”, injecting Spear hostcalls into Boa in an idiomatic JavaScript style.

## 2. Goals and Non-goals

### 2.1 Goals

- Add `sdk/rust` so users can package JS into a Spear-runnable WASM with minimal steps.
- Expose Spear hostcalls inside Boa as JS-friendly APIs (Promise / options object / AsyncIterator), hiding pointer and errno details.
- Align with existing Spear WASM hostcall design (`spear` import module, FD/epoll model, chat/mic/rtasr).
- Support OpenAI-style chat completions and tool calling, aiming for “JS tool handlers just work”.
- Support MCP via existing chat params, providing a natural JS configuration surface.

### 2.2 Non-goals

- Full Node.js / Web API compatibility (complete `fs`, `net`, `fetch`, `ReadableStream`, etc.).
- Making new Spear hostcalls (http/objectstore/env/log) a hard dependency in v1; optional extensions are listed later.
- Shipping an npm package; the “module system” inside Boa is virtual/builtin rather than real npm installation.

## 3. Foundations to Reuse (Current Spear)

### 3.1 WASM hostcalls

Today’s hostcalls mostly include:
- `time_now_ms` / `wall_time_s` / `sleep_ms` / `random_i64`
- `cchat_*` (chat session, send, recv, AUTO_TOOL_CALL, metrics)
- `rtasr_*` (realtime ASR)
- `mic_*` (microphone frames)
- `spear_epoll_*`, `spear_fd_ctl` (fd/epoll abstraction)

Implementation: `../src/spearlet/execution/runtime/wasm_hostcalls.rs`
Declarations (C SDK): `../sdk/c/include/spear.h`

### 3.2 Patterns from the C SDK to internalize

- Fixed import module/name macros
- “cap + retry on ENOSPC” for variable-length outputs (`*_recv_alloc`)
- `AUTO_TOOL_CALL` to let the runtime iterate tool calls
- Tool arena (`tool_arena_ptr` / `tool_arena_len`) to reduce transient allocations

The Rust SDK should absorb these details so JS users never deal with them directly.

## 4. High-level Architecture

### 4.1 Layering

Add `sdk/rust/` and organize it as multiple crates (workspace style) to keep concerns separated:

1) `spear-wasm-sys`
- Low-level `extern "C"` hostcall bindings via `#[link(wasm_import_module = "spear")]`.
- ABI correctness only; no JSON, no allocation policy.

2) `spear-wasm`
- Safe wrappers:
  - errno/rc → `Result<T, SpearError>`
  - ENOSPC-capacity growth loops
  - `Fd` RAII wrappers (explicit close)
  - epoll payload decoding (`Vec<(fd, events)>`)
- Kept independent of Boa/JS so it can also serve “Rust-on-WASM” users.
- For downstream chat, the public wrapper is `ChatRequestContext`, which models a single-turn request context rather than a persistent conversation session.

3) `spear-wasm-helper`
- Higher-level Rust helpers built on top of `spear-wasm`
- Suitable for Rust-first WASM apps and samples that need:
  - reusable `user stream` state handling
  - shared SSF / stream-protocol parsing
  - shared RTASR transcript event parsing
  - a base RTASR session skeleton
- Keeps product-specific state machines out of the low-level SDK layer.

4) `spear-boa`
- Boa integration:
  - inject builtin/virtual modules: `spear`, `spear/chat`, `spear/audio`, `spear/poll`, `spear/errors`, ...
  - map `spear-wasm` to JS-friendly APIs (Promise, options objects)
  - maintain tool handler registry (JS tool handler table)
- Also exposes lightweight JS helper modules for reusable JS-first guest logic, such as:
  - `spear/time_format`
  - `spear/rtasr_event`
  - `spear/stream_gate`

5) Samples (`samples/wasm-js/*`) (JS-first; built as WASI executables via a Boa JS runner)
- A small runner binary that:
  - loads user JS (embedded or via WASI allowed dirs)
  - creates Boa `Context`
  - installs `spear-boa` modules / globals
  - executes the user entry (e.g. `default export` or `main()`)
  - returns an exit code and/or writes output to stdout

SDK note: the SDK itself remains library-only (`sdk/rust`); the `main` entrypoints live under `samples/`.

### 4.2 User-facing project shapes

Support two shapes (same runtime supports both):

- Shape A: single-file JS embedded into WASM
  - user writes `main.js`
  - `build.rs` embeds it via `include_str!`
  - minimal dependencies and easy distribution

- Shape B: multi-file JS loaded via WASI
  - user has multiple JS modules
  - Spear runtime mounts a read-only directory via WASI preopen
  - better for larger projects, requires Spearlet `wasi_allowed_dirs`

Both rely on Boa’s module loader.

## 5. JS API Design (Industry best practices)

### 5.1 Principles

- Options objects for complex inputs.
- Promise-first APIs (even if internally synchronous/blocking) so users can `await`.
- Explicit resource closing via `.close()` plus `try/finally` patterns.
- Discriminable errors with `code`/`errno`/`op`, not just messages.
- OpenAI-style shapes for chat completions: `chat.completions.create`.

### 5.2 Modules and naming

Inject these virtual modules into Boa (no npm dependency):

- `spear` (main entry)
- `spear/chat`
- `spear/audio`
- `spear/poll`
- `spear/time`
- `spear/errors`

Example:

```js
import { Spear } from "spear";

export default async function main() {
  const resp = await Spear.chat.completions.create({
    model: "gpt-4o-mini",
    messages: [{ role: "user", content: "Hi" }],
    timeoutMs: 30_000,
  });
  return resp.text();
}
```

### 5.3 Chat Completions API

#### 5.3.1 High-level (recommended)

```ts
Spear.chat.completions.create(options): Promise<ChatCompletionResponse>
```

`options`:
- `model: string`
- `messages: Array<{role: "system"|"user"|"assistant"|"tool", content: string}>`
- `timeoutMs?: number`
- `maxIterations?: number` (maps to `max_iterations`)
- `maxTotalToolCalls?: number` (maps to `max_total_tool_calls`)
- `tools?: Tool[]` (see below)
- `mcp?: { enabled?: boolean; serverIds?: string[]; toolAllowlist?: string[] }`
- `metrics?: boolean`
- `debug?: boolean` (optionally surfaces parsed `_spear` fields)

`ChatCompletionResponse`:
- `json(): any`
- `text(): string` (best-effort extraction of assistant text; fallback to raw JSON)
- `raw(): Uint8Array`
- `metrics(): any | null` (when enabled, fetched via `cchat_ctl_get_metrics`)

#### 5.3.2 Low-level (advanced)

```ts
const session = Spear.chat.session();
session.addMessage({ role, content });
session.setParam("model", "gpt-4o-mini");
const resp = await session.send({ autoToolCall: true, metrics: true });
```

Useful for session reuse and fine-grained param control.

### 5.4 Tools / Function Calling (JS handlers)

#### 5.4.1 Core constraint

Spear’s AUTO_TOOL_CALL flow executes tool calls in the runtime and calls back into WASM using “function offset/pointer (fn_offset) + args JSON”.

To allow “tool handlers written in JS”, the Rust SDK must ship a set of precompiled “tool trampoline functions” in the WASM. Each trampoline corresponds to a slot (0..N-1), and JS maintains `slot -> handler` mapping.

#### 5.4.2 Design: precompiled N trampolines (recommended)

- Compile `N` trampoline functions into `spear-boa`, e.g.:
  - `tool_trampoline_0(args_ptr, args_len, out_ptr, out_len_ptr) -> i32`
  - ...
  - `tool_trampoline_{N-1}(...)`
- Each trampoline:
  - reads args bytes (UTF-8 JSON) from WASM linear memory
  - calls the Boa JS handler registered for that slot
  - serializes the handler return to JSON (or `{error:{...}}`)
  - writes to out buffer with ENOSPC semantics

JS API:

```js
import { Spear } from "spear";

const tools = [
  Spear.tool({
    name: "sum",
    description: "Add two integers",
    parameters: {
      type: "object",
      properties: { a: { type: "integer" }, b: { type: "integer" } },
      required: ["a", "b"],
    },
    handler: ({ a, b }) => ({ sum: a + b }),
  }),
];

await Spear.chat.completions.create({
  model: "gpt-4o-mini",
  messages: [{ role: "user", content: "Call sum for a=7 b=35" }],
  tools,
});
```

Implementation details:
- `Spear.tool(...)` allocates a free slot and registers tool JSON via `cchat_write_fn`.
- SDK automatically sets `tool_arena_ptr`/`tool_arena_len` and enforces defaults for:
  - `maxIterations`, `maxTotalToolCalls`, `maxToolOutputBytes`.

Capacity policy:
- default `N = 32` (tunable via feature/compile-time config); if exhausted, throw `SpearError{code:"tool_slot_exhausted"}`.

#### 5.4.3 Alternative without AUTO_TOOL_CALL (not recommended unless hostcalls extend)

Because `cchat_write_msg` cannot set `tool_call_id` today, JS cannot fully reproduce OpenAI’s tool loop message format inside WASM. If JS-side tool loop is required, add a hostcall:
- `cchat_append_message_json(fd, msg_json_ptr, msg_json_len)` (supports `tool_call_id`, etc.)

This is an optional extension for later.

### 5.5 MCP (via chat params)

Align with the existing implementation (see `../samples/wasm-c/mcp_fs.c`) but expose a more idiomatic JS surface:

```js
await Spear.chat.completions.create({
  model: "gpt-4o-mini",
  mcp: {
    enabled: true,
    serverIds: ["fs"],
    toolAllowlist: ["read_*", "list_*"]
  },
  messages: [{ role: "user", content: "Read Cargo.toml first 5 lines" }],
});
```

Mapping to params:
- `mcp.enabled`
- `mcp.server_ids`
- `mcp.tool_allowlist`
- `mcp.tool_denylist` (optional)

Notes:

- If the task has MCP policy in `Task.config`, the host may auto-apply defaults (`mcp.enabled` / `mcp.server_ids`) when the chat session is created.
- Task-level filters are injected as `mcp.task_tool_allowlist` / `mcp.task_tool_denylist` and are read-only to the guest.
- The host enforces task policy: enabling MCP or setting `mcp.server_ids` outside the task allowed set will be rejected.

### 5.6 Audio (mic + rtasr)

Provide two layers:

1) Low-level FD-style
- `Spear.audio.mic.open(options) -> MicHandle`
- `MicHandle.read() -> Promise<Uint8Array>` (on no data, waits via epoll internally or surfaces `EAGAIN`)
- `MicHandle.status() -> Promise<object>` (maps to `mic_ctl GET_STATUS`)

2) High-level streaming
- `MicHandle.frames(): AsyncIterable<Uint8Array>`
- `Spear.audio.rtasr.open(options) -> RtAsrHandle`
- `RtAsrHandle.events(): AsyncIterable<object>` (parses `rtasr_read` bytes as JSON events)

Internally, implement a minimal loop using `spear_epoll_*`:
- on `EAGAIN`, register fd in epoll and block on `epoll_wait`
- return Promises so users can `for await`

## 6. Key Rust SDK Implementation Details

### 6.1 WASM ABI and build flags

To support tool trampolines using “function pointer/table offset” (`fn_offset`), ensure the WASM exports the function table (similar to the C sample’s `-Wl,--export-table`).

Recommended template configuration:
- `.cargo/config.toml` injects linker args for `wasm32-wasip1` (`--export-table`, `--export-memory`)
- optional `lto` and `panic = "abort"` to reduce size

### 6.2 Memory and strings

- Hostcall I/O uses UTF-8.
- Rust ↔ JS:
  - string encoding/decoding centralized in `spear-boa`
  - binary data exposed as `Uint8Array` (avoid base64)

### 6.3 errno / rc mapping

Define `SpearError extends Error`:
- `code: string` (stable discriminator: `invalid_fd`, `buffer_too_small`, `eagain`, ...)
- `errno: number` (raw negative errno)
- `op: string` (operation name, e.g. `cchat_send`)

Provide helpers like `SpearError.is(e, "eagain")`.

### 6.4 Resource lifecycle

- Wrap every FD in `class Handle { close(): void }`.
- Provide canonical JS usage:

```js
const mic = await Spear.audio.mic.open({ source: "device" });
try {
  for await (const frame of mic.frames()) {
    // ...
  }
} finally {
  mic.close();
}
```

- Optional: use `FinalizationRegistry` as a non-deterministic safety net; docs should still recommend explicit close.

## 7. Optional Enhancements (Future)

To better match JS ecosystem expectations, consider adding these hostcalls later (not required for v1):

1) `spear.log(level, msg)` → map to `console.*`.
2) `spear.env_get(key)` → `process.env` subset.
3) `spear.http_call(method, url, headers, body)` → a small `fetch` subset.
4) objectstore put/get → module/config loading and result persistence.
5) `cchat_append_message_json` → unlock JS-side tool loop and richer message structures.

## 8. Testing Strategy

- Rust unit tests:
  - errno mapping
  - ENOSPC growth loops
  - tool slot allocation/release
- WASM integration tests:
  - extend existing approach (see `tests/wasm_openai_e2e_tests.rs`) with “Boa runtime + chat completion” e2e
- Samples:
  - WASM-C: `samples/wasm-c/*`
  - WASM-JS (Boa JS runner): `samples/wasm-js/chat_completion`, `samples/wasm-js/chat_completion_tool_sum`

## 9. Milestones

M1 (minimum viable)
- `spear-wasm-sys` + `spear-wasm` wrappers for existing hostcalls
- A WASM-JS runner sample (`samples/wasm-js/chat_completion`) runs a single JS file (`entry.mjs`) and exposes `Spear.chat.completions.create`

M2 (tool calling)
- precompiled N tool trampolines + JS `Spear.tool()`
- `AUTO_TOOL_CALL` works with JS handlers
  - sample: `samples/wasm-js/chat_completion_tool_sum`

M3 (audio streaming)
- JS wrappers for `mic`/`rtasr` + `AsyncIterable`
- epoll-based waiting for `EAGAIN`

## 10. Open Questions

- Boa version and WASI-target compatibility: define the minimal required feature set (modules, Promise job queue behavior).
- Stability of “fn pointer → i32 offset” in Rust WASM output: validate via a minimal PoC against current runtime expectations.
- Runtime engine coverage: hostcall linking is currently more complete behind the `wasmedge` feature; clarify whether wasmtime parity is needed.
