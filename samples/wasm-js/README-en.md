# wasm-js samples

These samples are JS-first: you write JavaScript (e.g. `src/entry.mjs`), and it runs in SPEAR as a WASM executable.

Under the hood, a small Rust “Boa JS runner” is compiled to WASM (`wasm32-wasip1`) and embeds/loads the JS entry.

So the focus here is JS, even though the runner itself is written in Rust.

Shared JS-first sample helpers now live in two layers:

- SDK-level generic helpers are exposed by `spear-boa` modules such as `spear/time_format`, `spear/rtasr_event`, and `spear/stream_gate`.
- Sample-local formatting helpers remain under `./lib/`, and are imported via stable virtual specifiers such as `app/lib/text_output` and `app/lib/transcript_writer`.

## Samples

- `chat_completion`: Chat completion via `Spear.chat.completions.create`.
- `chat_completion_tool_sum`: Tool calling via `Spear.tool(...)`.
- `router_filter_keyword`: Router filter sample.
- `user_stream_echo`: Bidirectional stream echo via `Spear.userStream` (JS).
- `user_stream_chat_completion`: Interactive user input over `Spear.userStream` → Chat Completion → write back to user stream (supports `/model <name>`). Uses SSF v1 DATA+COMMIT input semantics.
- `user_stream_live_caption`: Voice uplink on `stream_id=2` → RTASR incremental transcript on `stream_id=1`. Handles press-and-hold voice COMMIT and accepts both legacy and `conversation.item.*` transcription event names.
- `user_stream_voice_chat`: Voice uplink on `stream_id=2` → RTASR transcript → Chat Completion reply on `stream_id=1`. Waits for voice COMMIT before running the downstream chat step.

## Current voice defaults

- Console sends mono PCM16LE audio at `16000Hz`.
- Console currently batches capture chunks with `chunkMs=300`.
- `user_stream_live_caption` currently uses `server_vad` with `silence_ms=600`.
- `user_stream_live_caption` aggregates about `240ms` of PCM before each `rtasr.writeAudio()` call to reduce user-stream and JS bridge overhead.
