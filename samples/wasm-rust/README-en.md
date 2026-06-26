# WASM-Rust Samples

This directory contains Rust-first WASM samples that use the Rust guest SDK directly instead of going through the Boa JS runner.

The shared `user stream` / `SSF` / `RTASR` helpers for these samples now live in the SDK helper crate `sdk/rust/crates/spear-wasm-helper`.

## Samples

- `user_stream_live_caption`: modular live-caption sample built on top of `spear-wasm`, `spear-ssf`, and the Rust RTASR / user-stream hostcall wrappers
  - Source: `./user_stream_live_caption/src/main.rs`
  - Output: `../build/rust/user_stream_live_caption.wasm`
- `user_stream_voice_chat`: modular voice-chat sample that combines RTASR transcription on `stream_id=2` with downstream Chat Completion output on `stream_id=1`
  - Source: `./user_stream_voice_chat/src/main.rs`
  - Output: `../build/rust/user_stream_voice_chat.wasm`

## Build

Run from the repo root:

```bash
make samples-rust
```

Or build all sample families together:

```bash
make samples
```

## Design Notes

- Uses `epoll` through the Rust guest SDK wrapper, so the event loop stays readable without a busy sleep loop.
- Keeps sample-specific orchestration in each sample, while extracting shared `stream` and protocol helpers into `sdk/rust/crates/spear-wasm-helper`.
- Also extracts a shared RTASR session skeleton into `spear-wasm-helper`, so sample-local `rtasr_session.rs` files only keep strategy-specific behavior.
- `user_stream_live_caption` matches the current live-caption behavior:
  - `stream_id=1` for transcript output
  - `stream_id=2` for audio uplink
  - `server_vad` with `silence_ms=600`
  - about `240ms` PCM aggregation before each `rtasr.write`
  - `conversation.item.input_audio_transcription.*` compatibility
- `user_stream_voice_chat` matches the current voice-chat behavior:
  - `stream_id=2` audio upload followed by RTASR transcription
  - waits for voice COMMIT before flushing RTASR
  - sends the final transcript and downstream Chat Completion reply to `stream_id=1`
