# Samples

This directory contains buildable WASM samples (C + JS-first + Rust-first) and their build outputs.

## Layout

- `wasm-c/`: sample sources (C)
- `wasm-js/`: JS-first WASM samples (Boa JS runner compiled to WASM)
- `wasm-rust/`: Rust-first WASM samples (Rust guest SDK compiled to WASM)
  - Reuses `sdk/rust/crates/spear-wasm-helper` for shared Rust-first sample helpers
- `build/`: build outputs (`.wasm`)

## Build

Run from the repo root:

```bash
make samples
```

The build uses `zig` (`zig cc -target wasm32-wasi`) if available; otherwise it falls back to `clang` + `WASI_SYSROOT`.

WASM-JS samples are built with `cargo build --release --target wasm32-wasip1` into `build/js/`.
WASM-Rust samples are built with `cargo build --release --target wasm32-wasip1` into `build/rust/`.

## Samples

- `hello.c`: minimal sample
- `chat_completion.c`: basic Chat Completion call
- `chat_completion_tool_sum.c`: WASM custom tool + AUTO_TOOL_CALL loop
- `mic_rtasr.c`: mic + realtime ASR sample
- `mcp_fs.c`: MCP filesystem (stdio) tool injection + execution sample
- `user_stream_echo.c`: bidirectional user stream echo sample
- `user_stream_voice_chat.c`: press-and-hold voice input on `stream_id=2` → RTASR transcript → Chat Completion reply on `stream_id=1`
- `user_stream_live_caption.c`: live captions on `stream_id=2` → incremental transcript on `stream_id=1`

## JS samples (Boa JS runner compiled to WASM)

- `wasm-js/chat_completion`: executes `entry.mjs` via Boa JS runtime and calls Chat Completion
  - Output: `./build/js/js-chat_completion.wasm`
- `wasm-js/chat_completion_tool_sum`: executes `entry.mjs` via Boa JS runtime for tool calling (sum)
  - Output: `./build/js/js-chat_completion_tool_sum.wasm`
- `wasm-js/router_filter_keyword`: router keyword filter sample
  - Output: `./build/js/js-router_filter_keyword.wasm`
- `wasm-js/user_stream_echo`: bidirectional user stream echo sample
  - Output: `./build/js/js-user_stream_echo.wasm`
- `wasm-js/user_stream_chat_completion`: interactive user input → chat completion → output back to user stream
  - Output: `./build/js/js-user_stream_chat_completion.wasm`
- `wasm-js/user_stream_live_caption`: voice uplink on `stream_id=2` → RTASR incremental transcript on `stream_id=1`
  - Output: `./build/js/js-user_stream_live_caption.wasm`
- `wasm-js/user_stream_voice_chat`: press-and-hold voice input on `stream_id=2` → RTASR transcript → chat response on `stream_id=1`
  - Output: `./build/js/js-user_stream_voice_chat.wasm`

## Rust samples (Rust guest SDK compiled to WASM)

- `wasm-rust/user_stream_live_caption`: Rust-first live-caption sample with an epoll-driven event loop and modular `app` / `protocol` / `rtasr_session` / `stream` components
  - Output: `./build/rust/user_stream_live_caption.wasm`
- `wasm-rust/user_stream_voice_chat`: Rust-first voice-chat sample with epoll-driven user-stream handling and downstream Chat Completion orchestration
  - Output: `./build/rust/user_stream_voice_chat.wasm`

## Voice user-stream notes

- `stream_id=1` carries text output and committed text input.
- `stream_id=2` carries PCM16LE voice input and requires CTRL(OPEN) before DATA/COMMIT.
- Press-and-hold flows send `utterance_begin`, stream audio DATA, then send voice COMMIT on release.
- The live-caption and voice-chat samples accept both `input_audio_transcription.*` and `conversation.item.input_audio_transcription.*` RTASR event names.
- The current Console voice defaults are `16000Hz`, mono, `chunkMs=300`, which matches the live-caption sample's 16k PCM assumptions.

## MCP sample (mcp_fs)

This sample demonstrates:

1) enabling MCP via `cchat_ctl_set_param` session params (`mcp.enabled=true`, `mcp.server_ids=["fs"]`, etc.)
2) runtime MCP tool injection into `tools`
3) using `AUTO_TOOL_CALL` so the runtime executes MCP tool calls automatically

### Prerequisites

- SMS loads MCP server configs (this repo includes `config/sms/mcp.d/fs.toml`).
  - You must set it explicitly via `--mcp-dir ./config/sms/mcp.d` or `SMS_MCP_DIR=./config/sms/mcp.d`.
- `npx` is available on the host (the fs server is started via stdio using `@modelcontextprotocol/server-filesystem`)

If SMS does not load MCP configs, Spearlet will see an empty MCP registry, no MCP tools will be injected into `tools`, and the model may respond as if MCP tools do not exist.

### What to look for

- Source: `./wasm-c/mcp_fs.c`
- Output: `./build/mcp_fs.wasm`

If the final response includes tool calls/tool outputs (and ends with `MCP_OK`), MCP injection and execution are working.
