# Samples（示例）

本目录包含可直接构建的 WASM 示例（C + JS-first + Rust-first），以及对应的构建产物输出目录。

## 目录结构

- `wasm-c/`：示例源码（C）
- `wasm-js/`：以 JS 为主的 WASM 示例（Boa JS runner 编译为 WASM）
- `wasm-rust/`：以 Rust 为主的 WASM 示例（Rust guest SDK 编译为 WASM）
  - 复用 `sdk/rust/crates/spear-wasm-helper` 作为 Rust-first 示例共享辅助模块
- `build/`：构建输出（`.wasm`）

## 构建

在仓库根目录执行：

```bash
make samples
```

构建会优先使用 `zig`（`zig cc -target wasm32-wasi`）；若未安装 `zig`，则使用 `clang` + `WASI_SYSROOT`。

WASM-JS 示例通过 `cargo build --release --target wasm32-wasip1` 构建，输出到 `build/js/`。
WASM-Rust 示例通过 `cargo build --release --target wasm32-wasip1` 构建，输出到 `build/rust/`。

## 示例列表

- `hello.c`：最小示例
- `chat_completion.c`：基础 Chat Completion 调用
- `chat_completion_tool_sum.c`：WASM 自定义 tool（函数）+ AUTO_TOOL_CALL 闭环
- `mic_rtasr.c`：mic + realtime ASR 示例
- `mcp_fs.c`：MCP filesystem（stdio）工具注入与调用示例
- `user_stream_echo.c`：双向 user stream echo 示例
- `user_stream_voice_chat.c`：按住说话语音输入走 `stream_id=2`，经 RTASR 转写后再将 Chat Completion 回复写回 `stream_id=1`
- `user_stream_live_caption.c`：语音输入走 `stream_id=2`，将增量字幕实时写回 `stream_id=1`

## JS 示例列表（Boa JS runner 编译为 WASM）

- `wasm-js/chat_completion`：通过 Boa JS 运行时执行 `entry.mjs`，调用 Chat Completion
  - 产物：`./build/js/js-chat_completion.wasm`
- `wasm-js/chat_completion_tool_sum`：通过 Boa JS 运行时执行 `entry.mjs`，进行 tool calling（sum）
  - 产物：`./build/js/js-chat_completion_tool_sum.wasm`
- `wasm-js/router_filter_keyword`：Router 关键词过滤示例
  - 产物：`./build/js/js-router_filter_keyword.wasm`
- `wasm-js/user_stream_echo`：双向 user stream echo 示例
  - 产物：`./build/js/js-user_stream_echo.wasm`
- `wasm-js/user_stream_chat_completion`：交互式用户输入 → Chat Completion → 输出回 user stream
  - 产物：`./build/js/js-user_stream_chat_completion.wasm`
- `wasm-js/user_stream_live_caption`：将 `stream_id=2` 的语音上行送入 RTASR，并把增量字幕写回 `stream_id=1`
  - 产物：`./build/js/js-user_stream_live_caption.wasm`
- `wasm-js/user_stream_voice_chat`：按住说话时把 `stream_id=2` 的语音上行送入 RTASR，松手后把转写与对话回复写回 `stream_id=1`
  - 产物：`./build/js/js-user_stream_voice_chat.wasm`

## Rust 示例列表（Rust guest SDK 编译为 WASM）

- `wasm-rust/user_stream_live_caption`：Rust-first 的实时字幕示例，采用 epoll 驱动事件循环，并按 `app` / `protocol` / `rtasr_session` / `stream` 模块拆分
  - 产物：`./build/rust/user_stream_live_caption.wasm`
- `wasm-rust/user_stream_voice_chat`：Rust-first 的语音对话示例，采用 epoll 驱动 user stream，并在转写完成后串接下游 Chat Completion
  - 产物：`./build/rust/user_stream_voice_chat.wasm`

## 语音 user stream 说明

- `stream_id=1` 承载文本输出，以及提交后的文本输入。
- `stream_id=2` 承载 PCM16LE 语音输入，且在发送 DATA/COMMIT 前必须先发送 CTRL(OPEN)。
- 按住说话流程会先发送 `utterance_begin`，持续发送音频 DATA，松手时发送 voice COMMIT。
- `live_caption` 与 `voice_chat` 示例同时兼容 `input_audio_transcription.*` 和 `conversation.item.input_audio_transcription.*` 两套 RTASR 事件名。
- 当前 Console 语音默认配置为 `16000Hz`、单声道、`chunkMs=300`，与 live caption 示例里的 16k 音频假设保持一致。

## MCP 示例（mcp_fs）

该示例演示：

1) 通过 `cchat_ctl_set_param` 开启 MCP（会话参数：`mcp.enabled=true`、`mcp.server_ids=["fs"]` 等）
2) 运行时把 MCP tools 注入到 `tools`
3) 通过 `AUTO_TOOL_CALL` 让运行时自动执行模型返回的 MCP tool call

### 前置条件

- SMS 已加载 MCP server 配置目录（仓库内已提供 `config/sms/mcp.d/fs.toml`）。
  - 需要显式配置：启动 SMS 时传 `--mcp-dir ./config/sms/mcp.d`，或设置环境变量 `SMS_MCP_DIR=./config/sms/mcp.d`。
- 本机可用 `npx`（`fs.toml` 使用 stdio 启动 `@modelcontextprotocol/server-filesystem`）

如果 SMS 未加载 MCP 配置目录，Spearlet 侧看到的 MCP registry 会是空的，MCP tools 不会被注入到 `tools`，模型就可能表现得像“没有 MCP 工具”一样。

### 运行要点

- `mcp_fs.c` 源码：`./wasm-c/mcp_fs.c`
- 构建产物：`./build/mcp_fs.wasm`

如果你看到响应里包含 tool call 与 tool output（并且最终输出 `MCP_OK`），就说明 MCP 注入与执行链路工作正常。
