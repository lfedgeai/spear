# WASM Samples Build Guide

## Layout
- Source: `samples/wasm-c/hello.c`
- Source: `samples/wasm-c/chat_completion.c` (Chat Completions sample)
- Source: `samples/wasm-c/user_stream_echo.c` (bidirectional user stream echo)
- Source: `samples/wasm-c/user_stream_voice_chat.c` (Console press-and-hold: voice → transcription → chat sample)
- Source: `samples/wasm-c/user_stream_live_caption.c` (Console voice uplink → RTASR incremental transcript sample)
- Source: `samples/wasm-js/chat_completion/src/main.rs` (Boa JS runner compiled to WASM; runs `entry.mjs` → Chat Completion)
- Source: `samples/wasm-js/chat_completion_tool_sum/src/main.rs` (Boa JS runner compiled to WASM; runs `entry.mjs` → Tool calling)
- Source: `samples/wasm-js/router_filter_keyword/src/main.rs` (Boa JS runner compiled to WASM; runs `entry.mjs` → Router keyword filter)
- Source: `samples/wasm-js/user_stream_echo/src/main.rs` (Boa JS runner compiled to WASM; runs `entry.mjs` → bidirectional user stream echo)
- Source: `samples/wasm-js/user_stream_chat_completion/src/main.rs` (Boa JS runner compiled to WASM; runs `entry.mjs` → user input over user stream → Chat Completion)
- Source: `samples/wasm-js/user_stream_live_caption/src/main.rs` (Boa JS runner compiled to WASM; runs `entry.mjs` → voice uplink → RTASR live captions)
- Source: `samples/wasm-js/user_stream_voice_chat/src/main.rs` (Boa JS runner compiled to WASM; runs `entry.mjs` → voice uplink → RTASR transcript → Chat Completion)
- Source: `samples/wasm-rust/user_stream_live_caption/src/main.rs` (Rust guest SDK compiled to WASM; runs a modular epoll-driven live-caption state machine)
- Source: `samples/wasm-rust/user_stream_voice_chat/src/main.rs` (Rust guest SDK compiled to WASM; runs a modular epoll-driven voice-chat state machine)
- Shared SDK helper crate: `sdk/rust/crates/spear-wasm-helper/src/lib.rs` (reusable Rust-first sample helpers for user-stream state, SSF parsing, RTASR event parsing, and the base RTASR session skeleton)
- Source: `samples/wasm-c/mic_rtasr.c` (realtime mic → realtime ASR)
- Output: `samples/build/hello.wasm`
  - WASM-JS outputs: `samples/build/js/js-*.wasm`
  - WASM-Rust outputs: `samples/build/rust/*.wasm`

## mic_rtasr prerequisites

- Host binaries must enable `mic-device` (otherwise `source=device` fails)
- Host must configure a realtime ASR backend (e.g. `openai_realtime_ws`) plus its API key
- Default backend name is `openai-realtime-asr` (override at build time via `-DSP_RTASR_BACKEND=\"...\"`)

Note: the `mic_rtasr` sample uses `server_vad` segmentation by default (silence-based).

Suggested setup:

- Spearlet config example: `config/spearlet/config.toml` (includes an `openai_realtime_ws` backend)
- Set env before running: `OPENAI_REALTIME_API_KEY`

How to run: after building `samples/build/mic_rtasr.wasm`, upload it as a WASM executable and run it as a task (see `docs/api-usage-guide-en.md` for the upload/task workflow).

## chat_completion sample

- Uses `SP_MODEL` (default `gpt-4o-mini`) as the request model
- Optional: define `SP_ROUTE_OLLAMA_GEMMA3` at build time to switch the model to `SP_OLLAMA_GEMMA3_MODEL` (default `gemma3:1b`)
- Response JSON includes `_spear.backend` (selected backend name) and `_spear.model` (request model); the sample prints `debug_backend=...`

## Build
- Run: `make samples`
- Compiler priority:
  - Prefer `zig`: `zig cc -target wasm32-wasi`
  - Fallback `clang`: requires `WASI_SYSROOT` pointing to WASI SDK sysroot

WASM-JS samples:
- Built by `cargo build --release --target wasm32-wasip1`
- Controlled by Makefile vars:
  - `BUILD_JS_SAMPLES=0` to skip WASM-JS samples
  - `JS_SAMPLES="chat_completion chat_completion_tool_sum router_filter_keyword user_stream_echo user_stream_chat_completion user_stream_live_caption user_stream_voice_chat"` to select which samples to build
  - `JS_WASM_PREFIX="js-"` to set the WASM-JS output filename prefix (default `js-`)

WASM-Rust samples:
- Built by `cargo build --release --target wasm32-wasip1`
- Controlled by Makefile vars:
  - `BUILD_RUST_SAMPLES=0` to skip WASM-Rust samples
  - `RUST_SAMPLES="user_stream_live_caption user_stream_voice_chat"` to select which Rust-first samples to build
  - `SAMPLES_RUST_DIR="samples/wasm-rust"` to point to the Rust-first sample root

## clang usage
- Environment: `WASI_SYSROOT=/opt/wasi-sdk/share/wasi-sysroot` (adjust as needed)
- Command uses: `clang --target=wasm32-wasi --sysroot=$(WASI_SYSROOT)`
- Without SDK or sysroot, command fails; install `zig` or set `WASI_SYSROOT`

## Important changes
- `make samples` builds WASM-C, WASM-JS, and WASM-Rust samples and writes artifacts under `samples/build/`

## Voice user-stream behavior
- `stream_id=1` is the text stream for transcript output, committed text input, and model replies.
- `stream_id=2` is the voice uplink stream; the runtime expects CTRL(OPEN) before DATA/COMMIT.
- Press-and-hold flows send `utterance_begin`, stream PCM16LE DATA, and send voice COMMIT on release.
- The live-caption and voice-chat samples accept both `input_audio_transcription.*` and `conversation.item.input_audio_transcription.*` RTASR event names.
- Current Console defaults are `16000Hz`, mono, `chunkMs=300`; the JS live-caption sample uses `server_vad` with `silence_ms=600` and also aggregates about `240ms` of PCM before each `rtasr.writeAudio()` call.
- The Rust live-caption sample mirrors the same protocol behavior, but uses Rust SDK `epoll` wrappers and typed modules instead of the Boa JS runner.
- The Rust voice-chat sample mirrors the JS voice-chat behavior: wait for voice COMMIT, finish RTASR transcription, then invoke downstream Chat Completion and write the response back to `stream_id=1`.

## Runtime integration
- The generated `hello.wasm` can be uploaded via SMS file service and referenced in task registration `executable.uri`
- Spearlet WASM runtime validates module bytes during instance creation; invalid content errors out

## Sample source
```c
int main() { return 0; }
```
