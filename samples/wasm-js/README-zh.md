# wasm-js 示例

这些示例以 JS 为主：你编写 JavaScript（例如 `src/entry.mjs`），并在 SPEAR 中以 WASM 可执行的形式运行。

底层实现是一个很小的 Rust “Boa JS runner”，它会被编译为 WASM（`wasm32-wasip1`），并在运行时嵌入/加载 JS 入口。

因此这里的重点是 JS（尽管 runner 本身是 Rust 写的）。

这些 JS-first 示例共享的轻量 helper 现在分成两层：

- SDK 级通用 helper 由 `spear-boa` 暴露，例如 `spear/time_format`、`spear/rtasr_event`、`spear/stream_gate`。
- sample 本地的展示型 helper 继续放在 `./lib/` 下，并通过稳定的虚拟 specifier 导入，例如 `app/lib/text_output` 与 `app/lib/transcript_writer`。

## 示例列表

- `chat_completion`：通过 `Spear.chat.completions.create` 调用 Chat Completion。
- `chat_completion_tool_sum`：通过 `Spear.tool(...)` 做 tool calling。
- `router_filter_keyword`：Router 关键词过滤示例。
- `user_stream_echo`：通过 `Spear.userStream` 实现双向 stream echo（JS）。
- `user_stream_chat_completion`：基于 `Spear.userStream` 的交互式用户输入 → Chat Completion → 输出回 user stream（支持 `/model <name>`）。使用 SSF v1 DATA+COMMIT 的输入语义。
- `user_stream_live_caption`：将 `stream_id=2` 的语音上行送入 RTASR，并把增量字幕写回 `stream_id=1`。支持按住说话的 voice COMMIT，并兼容旧版与 `conversation.item.*` 两套转写事件名。
- `user_stream_voice_chat`：将 `stream_id=2` 的语音上行送入 RTASR，拿到最终转写后再把 Chat Completion 回复写回 `stream_id=1`。会在收到 voice COMMIT 后再进入下游对话阶段。

## 当前语音默认值

- Console 发送单声道 `16000Hz` 的 PCM16LE 音频。
- Console 当前使用 `chunkMs=300` 进行采集分片。
- `user_stream_live_caption` 当前使用 `server_vad`，其中 `silence_ms=600`。
- `user_stream_live_caption` 会先聚合约 `240ms` 的 PCM，再执行一次 `rtasr.writeAudio()`，以减少 user stream 与 JS bridge 的开销。
