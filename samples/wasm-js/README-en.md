# wasm-js samples

These samples are JS-first: you write JavaScript (e.g. `src/entry.mjs`), and it runs in SPEAR as a WASM executable.

Under the hood, a small Rust “Boa JS runner” is compiled to WASM (`wasm32-wasip1`) and embeds/loads the JS entry.

So the focus here is JS, even though the runner itself is written in Rust.

## Samples

- `chat_completion`: Chat completion via `Spear.chat.completions.create`.
- `chat_completion_tool_sum`: Tool calling via `Spear.tool(...)`.
- `router_filter_keyword`: Router filter sample.
- `user_stream_echo`: Bidirectional stream echo via `Spear.userStream` (JS).
- `user_stream_chat_completion`: Interactive user input over `Spear.userStream` → Chat Completion → write back to user stream (supports `/model <name>`). Uses SSF v1 DATA+COMMIT input semantics.
