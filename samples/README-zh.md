# Samples（示例）

本目录包含可直接构建的 WASM 示例（C + Rust），以及对应的构建产物输出目录。

## 目录结构

- `wasm-c/`：示例源码（C）
- `wasm-js/`：以 JS 为主的 WASM 示例（Boa JS runner 编译为 WASM）
- `build/`：构建输出（`.wasm`）

## 构建

在仓库根目录执行：

```bash
make samples
```

构建会优先使用 `zig`（`zig cc -target wasm32-wasi`）；若未安装 `zig`，则使用 `clang` + `WASI_SYSROOT`。

WASM-JS 示例通过 `cargo build --release --target wasm32-wasip1` 构建，主要输出到 `build/js/`，并兼容拷贝到 `build/rust/`。

## 示例列表

- `hello.c`：最小示例
- `chat_completion.c`：基础 Chat Completion 调用
- `chat_completion_tool_sum.c`：WASM 自定义 tool（函数）+ AUTO_TOOL_CALL 闭环
- `mic_rtasr.c`：mic + realtime ASR 示例
- `mcp_fs.c`：MCP filesystem（stdio）工具注入与调用示例

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
