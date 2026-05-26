# wasm-js 示例

这些示例以 JS 为主：你编写 JavaScript（例如 `src/entry.mjs`），并在 SPEAR 中以 WASM 可执行的形式运行。

底层实现是一个很小的 Rust “Boa JS runner”，它会被编译为 WASM（`wasm32-wasip1`），并在运行时嵌入/加载 JS 入口。

因此这里的重点是 JS（尽管 runner 本身是 Rust 写的）。

## 示例列表

- `chat_completion`：通过 `Spear.chat.completions.create` 调用 Chat Completion。
- `chat_completion_tool_sum`：通过 `Spear.tool(...)` 做 tool calling。
- `router_filter_keyword`：Router 关键词过滤示例。
- `user_stream_echo`：通过 `Spear.userStream` 实现双向 stream echo（JS）。
- `user_stream_chat_completion`：基于 `Spear.userStream` 的交互式用户输入 → Chat Completion → 输出回 user stream（支持 `/model <name>`）。使用 SSF v1 DATA+COMMIT 的输入语义。
