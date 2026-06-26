# Rust SDK（Boa JS → WASM → Spear）细节方案（ZH）

## 1. 背景与问题

Spear 当前的 WASM 侧能力主要通过 `import_module("spear")` 的 hostcall 暴露，并且已经提供了 C 头文件 SDK（`../sdk/c/include/spear.h`）用于简化 WASM-C 的调用。

当用户希望使用 JavaScript 编写逻辑，并在 Spear 上以 WASM 方式运行时，常见路径是：在 Rust 里嵌入 JS 引擎（Boa），把该 Rust 程序编译为 WASI target（当前为 `wasm32-wasip1`），由 Spear 的 WASM runtime 执行。

此时用户会遇到三类成本：
- 运行时成本：自己搭 Boa、模块加载、事件循环、异常处理、返回值约定。
- Spear hostcall 成本：需要自己处理指针/长度/缓冲区扩容、errno 语义、FD 生命周期、epoll 等。
- JS 体验成本：hostcall 风格偏系统接口，缺少 JS 常用的 Promise/Options object/AsyncIterator/Abort 等模式。

本方案在 Spear 仓库内新增 Rust 侧 SDK，提供一个“可编译为 WASM 的 Boa JS 运行时 + JS 友好 API”，并把 Spear 相关 hostcall 以更符合 JS 生态的方式注入到 Boa 的 JS 环境中。

## 2. 目标与非目标

### 2.1 目标（Goals）

- 提供 `sdk/rust`，让用户以最少步骤把 JS 代码打包为可在 Spear 运行的 WASM。
- 在 Boa JS 里提供一套 JS 友好的 Spear API（面向对象/Options object/Promise/AsyncIterator），避免暴露指针与 errno 细节。
- 与当前 Spear WASM hostcall 设计对齐（`spear` import module，FD/epoll 模型，chat/mic/rtasr 等）。
- 支持 OpenAI 风格的 chat completions 与工具调用（tools/function calling），并尽量做到“写 JS 工具函数即可用”。
- 支持 MCP（通过现有的 chat params 驱动 MCP tool 注入/执行），在 JS 侧提供更自然的配置入口。

### 2.2 非目标（Non-goals）

- 不追求完整 Node.js/Web API 兼容（例如完整的 `fs`、`net`、`fetch`、`ReadableStream` 生态）。
- 不在第一期引入新的 Spear hostcall（如 http/objectstore/env/log）作为强依赖；会在“可选增强”章节提出扩展项。
- 不定义对外发布到 npm 的包形态（Boa 内运行的“模块系统”是运行时内置/虚拟的，而非真实 npm 安装）。

## 3. 现状复用点（Spear 已有基础）

### 3.1 WASM hostcalls

当前 hostcalls 主要集中在：
- `time_now_ms` / `wall_time_s` / `sleep_ms` / `random_i64`
- `cchat_*`（chat session、发送、接收、AUTO_TOOL_CALL、metrics）
- `rtasr_*`（实时 ASR）
- `mic_*`（麦克风帧读取）
- `spear_epoll_*`、`spear_fd_ctl`（fd/epoll 抽象）

实现位于 `../src/spearlet/execution/runtime/wasm_hostcalls.rs`，C SDK 对应声明位于 `../sdk/c/include/spear.h`。

### 3.2 C SDK 的模式（可借鉴点）

C SDK 的关键模式包括：
- 用宏固定 import module/name
- 用 “cap + ENOSPC 重试” 模式处理变长输出（`*_recv_alloc`）
- 用 `AUTO_TOOL_CALL` 让 runtime 自动迭代工具调用
- tool arena（`tool_arena_ptr` / `tool_arena_len`）降低工具调用过程中的临时内存碎片

Rust SDK 需要把这些模式内化，变成对 JS 用户不可见的实现细节。

## 4. 总体架构

### 4.1 组件分层

建议在仓库新增 `sdk/rust/`，内部以 workspace 的方式组织多个 crate（便于分层、复用、减少 feature 蔓延）：

1) `spear-wasm-sys`
- 低层 `extern "C"` 绑定：声明 `#[link(wasm_import_module = "spear")]` 的 hostcall 函数。
- 只负责 ABI 正确、无分配/无 JSON。

2) `spear-wasm`
- 安全封装：
  - errno/rc → `Result<T, SpearError>`
  - 变长输出 buffer 扩容
  - `Fd` RAII（显式 close，防止泄漏）
  - epoll wait 解析（`Vec<(fd, events)>`）
- 保持与 JS/Boa 无关：以后也可给 Rust-on-WASM（非 JS）直接用。
- 对下游 chat 而言，公开 wrapper 现为 `ChatRequestContext`，它表达的是单轮请求上下文，而不是长期多轮对话 session。

3) `spear-wasm-helper`
- 构建在 `spear-wasm` 之上的 Rust 高层 helper。
- 适合 Rust-first WASM app / sample 复用下列能力：
  - `user stream` 状态管理
  - 共享的 SSF / stream 协议解析
  - 共享的 RTASR transcript 事件解析
  - RTASR 基础会话骨架
- 让产品级状态机不必进入低层 SDK。

4) `spear-boa`
- Boa 集成：
  - 注入虚拟模块：`spear`、`spear/chat`、`spear/audio`、`spear/poll`、`spear/errors` 等。
  - 将 `spear-wasm` 的能力映射到 JS 友好的 API（Promise/Options object）。
  - 维护工具回调注册表（JS tool handler 表）。
- 同时暴露一小层面向 JS-first guest 逻辑复用的 helper 模块，例如：
  - `spear/time_format`
  - `spear/rtasr_event`
  - `spear/stream_gate`

5) 样例（`samples/wasm-js/*`）（以 JS 为主；通过 Boa JS runner 编译为 WASI 可执行）
- 提供一个轻量 runner：
  - 加载用户 JS（内嵌或从 WASI 允许目录读取）
  - 创建 Boa `Context`
  - 安装 `spear-boa` 模块与全局对象
  - 执行用户 entry（例如 `default export` 或 `main()`）
  - 将退出码/结果写 stdout 或按约定返回

说明：SDK 本身保持为库（`sdk/rust`），`main` 入口放在 `samples/` 下作为可运行示例。

### 4.2 用户侧开发形态

建议提供两种形态（同一个 runtime 支持）：

- 形态 A：单文件 JS → 内嵌到 WASM
  - 用户写 `main.js`
  - `build.rs` 把 JS 作为 `include_str!` 打进 wasm
  - 适合最小依赖、易分发

- 形态 B：目录型 JS → WASI 读文件
  - 用户有多个 JS 模块文件
  - Spear 运行时通过 WASI preopen 映射只读目录
  - 适合中大型项目，但要求 Spearlet 允许对应目录（现有 WASM config 已有 `wasi_allowed_dirs`）

两种形态都通过 Boa 的 module loader 实现 `import`。

## 5. JS API 设计（面向业界 best practice）

### 5.1 设计原则

- Options object：复杂参数一律用对象参数，避免长参数列表。
- Promise-first：对外 API 默认返回 Promise（即便内部同步阻塞），让用户用 `await` 组织逻辑。
- 资源显式关闭：FD/会话这类资源提供 `.close()`，并提供 `using`/`try/finally` 示例。
- 错误可判别：错误对象要带 `code`/`errno`/`op`，避免只能靠 message。
- 兼容 OpenAI 风格：chat API 尽量贴近 OpenAI SDK 的 `chat.completions.create` 调用形态。

### 5.2 模块与命名

在 Boa 中注入以下“虚拟模块”（不依赖 npm）：

- `spear`：主入口（聚合导出）
- `spear/chat`：chat 相关
- `spear/audio`：mic/rtasr
- `spear/poll`：fd/epoll
- `spear/time`：time/sleep/random
- `spear/errors`：错误类型与 errno 映射

用户侧使用：

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

#### 5.3.1 高层 API（推荐）

```ts
Spear.chat.completions.create(options): Promise<ChatCompletionResponse>
```

`options`：
- `model: string`
- `messages: Array<{role: "system"|"user"|"assistant"|"tool", content: string}>`
- `timeoutMs?: number`
- `maxIterations?: number`（对应 `max_iterations`）
- `maxTotalToolCalls?: number`（对应 `max_total_tool_calls`）
- `tools?: Tool[]`（见下）
- `mcp?: { enabled?: boolean; serverIds?: string[]; toolAllowlist?: string[] }`
- `metrics?: boolean`
- `debug?: boolean`（可选：在返回对象里附带 `_spear` 字段解析结果）

返回 `ChatCompletionResponse`：
- `json(): any`（解析后的对象）
- `text(): string`（尽力提取 assistant 内容；提取失败返回原始 JSON）
- `raw(): Uint8Array`（原始字节）
- `metrics(): any | null`（若启用 metrics，通过 `cchat_ctl_get_metrics` 获取并解析）

#### 5.3.2 低层 API（进阶）

```ts
const session = Spear.chat.session();
session.addMessage({ role, content });
session.setParam("model", "gpt-4o-mini");
const resp = await session.send({ autoToolCall: true, metrics: true });
```

用于需要复用 session、或需要精细控制 params 的场景。

### 5.4 Tools / Function Calling（JS 工具函数）

#### 5.4.1 核心约束

Spear 的 AUTO_TOOL_CALL 机制会把工具调用下沉到 runtime，并以 “函数表偏移/指针（fn_offset）+ args JSON” 的形式回调到 WASM 内函数。

要让“工具 handler 写在 JS 里”成为可能，Rust SDK 需要在 WASM 内预置一组“工具 trampoline 函数”，每个 trampoline 对应一个 slot（0..N-1），并在 JS 侧维护 `slot -> handler` 映射。

#### 5.4.2 方案：预置 N 个 trampoline（推荐）

- 在 `spear-boa` 内部编译进 `N` 个形如：
  - `tool_trampoline_0(args_ptr, args_len, out_ptr, out_len_ptr) -> i32`
  - ...
  - `tool_trampoline_{N-1}(...)`
- 每个 trampoline：
  - 从 WASM 内存读取 args（UTF-8 JSON）
  - 调用 Boa 中对应 slot 的 JS handler
  - 将 handler 的返回值序列化为 JSON string（或 `{error:{...}}`）
  - 写回 out buffer，遵循 `ENOSPC` 扩容语义

JS 侧 API：

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

实现细节：
- `Spear.tool(...)` 分配一个空闲 slot，并将 tool 的 JSON schema 注册到 `cchat_write_fn`。
- SDK 自动配置 `tool_arena_ptr`/`tool_arena_len`，并提供默认上限：
  - `maxIterations`、`maxTotalToolCalls`、`maxToolOutputBytes`（用于限制工具输出大小）。

容量策略：
- N 默认 32（可通过 feature/编译参数调整），超出则抛 `SpearError{code:"tool_slot_exhausted"}`。

#### 5.4.3 不使用 AUTO_TOOL_CALL 的备选方案（不推荐，除非扩展 hostcall）

因为 `cchat_write_msg` 目前无法设置 `tool_call_id`，JS 无法在 WASM 内完整复刻 OpenAI tool loop 的 message 格式；因此若坚持 JS 自己做 tool loop，需要新增 hostcall：
- `cchat_append_message_json(fd, msg_json_ptr, msg_json_len)`（允许设置 `tool_call_id` 等字段）

这属于“可选增强”，可在后续版本再引入。

### 5.5 MCP（通过 chat params）

与现有实现对齐（参考 `../samples/wasm-c/mcp_fs.c`），在 JS API 里做成更自然的配置项：

```js
await Spear.chat.completions.create({
  model: "gpt-4o-mini",
  mcp: {
    enabled: true,
    serverIds: ["fs"],
    toolAllowlist: ["read_*", "list_*"]
  },
  messages: [{ role: "user", content: "Read Cargo.toml first 5 lines" }],
  tools: [],
});
```

SDK 会把上述 options 映射为 params：
- `mcp.enabled`
- `mcp.server_ids`
- `mcp.tool_allowlist`
- `mcp.tool_denylist`（可选）

说明：

- 如果 task 在 `Task.config` 中配置了 MCP 策略，host 可能会在创建 chat session 时自动写入缺省值（`mcp.enabled` / `mcp.server_ids`），SDK 未必需要显式设置。
- task 级过滤会以 `mcp.task_tool_allowlist` / `mcp.task_tool_denylist` 注入，并且对 WASM 侧只读。
- host 会强制 task policy：若 task 未允许启用 MCP，或你试图把 `mcp.server_ids` 设到 task allowed 之外，会被拒绝。

### 5.6 Audio（mic + rtasr）

提供两层 API：

1) 低层 FD API（贴近 hostcall）：
- `Spear.audio.mic.open(options) -> MicHandle`
- `MicHandle.read() -> Promise<Uint8Array>`（无数据时内部用 epoll 等待或抛 `EAGAIN`）
- `MicHandle.status() -> Promise<object>`（映射 `mic_ctl GET_STATUS`）

2) 高层流式 API（JS best practice）：
- `MicHandle.frames(): AsyncIterable<Uint8Array>`
- `Spear.audio.rtasr.open(options) -> RtAsrHandle`
- `RtAsrHandle.events(): AsyncIterable<object>`（把 `rtasr_read` 的 bytes 解析为 JSON event）

内部实现建议使用 `spear_epoll_*` 构建最小事件循环：
- 当 `read()` 得到 `EAGAIN` 时，把 fd 注册到 epoll 并 `epoll_wait`
- 返回 Promise，使用户可以 `for await`

## 6. Rust SDK 的关键实现细节

### 6.1 WASM ABI 与构建参数

为支持 tool trampoline 的 “函数地址/表偏移” 用法，需要保证 WASM 导出函数表（类似 C 示例的 `-Wl,--export-table`）。

建议在模板工程中提供：
- `.cargo/config.toml`：对 `wasm32-wasip1` 注入 linker args（`--export-table`、`--export-memory`）
- 可选：开启 `lto`、`panic = "abort"`，减小体积

### 6.2 内存与字符串

- hostcall 输入输出统一使用 UTF-8。
- Rust ↔ JS：
  - JS string ↔ Rust `String`：编码/解码集中在 `spear-boa`。
  - bytes：使用 `Uint8Array` 承载，避免 base64。

### 6.3 errno / rc 映射

设计 `SpearError extends Error`：
- `code: string`（稳定、可判别，比如 `invalid_fd`/`buffer_too_small`/`eagain`）
- `errno: number`（原始负 errno）
- `op: string`（发生错误的操作名，比如 `cchat_send`）

并提供 `SpearError.is(e, "eagain")` 之类的判别函数。

### 6.4 资源生命周期

- 每个 FD 包装为 `class Handle { close(): void }`。
- JS 侧提供 `using` 示例：

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

- 可选：利用 `FinalizationRegistry` 做“兜底 close”（非确定性，仅作为防泄漏），但文档明确推荐显式 close。

## 7. 可选增强（后续可扩展项）

为进一步贴近 JS 生态与实际 Agent 开发诉求，建议后续增量加入以下 hostcalls（不是第一期必需）：

1) `spear.log(level, msg)`：JS 侧 `console.*` 直连宿主日志。
2) `spear.env_get(key)`：对齐 `process.env` 子集。
3) `spear.http_call(method, url, headers, body)`：提供 `fetch` 子集。
4) `objectstore put/get`：用于加载 JS 模块/配置，或写入结果。
5) `cchat_append_message_json`：解锁 JS 自己实现 tool loop 与更复杂消息结构。

## 8. 测试与验证策略

- Rust 单元测试：
  - errno 映射
  - buffer 扩容与 ENOSPC 语义
  - tool trampoline 的 slot 分配/释放
- WASM 集成测试：
  - 参考现有 `tests/wasm_openai_e2e_tests.rs` 的方式，新增 “Boa runtime + chat completion” 的 e2e。
- 样例：
  - WASM-C：`samples/wasm-c/*`
- WASM-JS（Boa JS runner）：`samples/wasm-js/chat_completion`、`samples/wasm-js/chat_completion_tool_sum`

## 9. 交付拆分（Milestones）

M1（最小可用）
- `spear-wasm-sys` + `spear-wasm` 封装现有 hostcalls
- WASM-JS runner 示例（`samples/wasm-js/chat_completion`）：运行单文件 JS（`entry.mjs`），注入 `Spear.chat.completions.create`

M2（工具调用）
- 预置 N 个 tool trampoline + JS `Spear.tool()` 注册
- `AUTO_TOOL_CALL` 跑通 JS 工具函数
  - 示例：`samples/wasm-js/chat_completion_tool_sum`

M3（音频流）
- `mic`/`rtasr` JS 封装 + `AsyncIterable`
- 用 epoll 处理 EAGAIN

## 10. 待确认点（Open Questions）

- Boa 版本选择与 WASI target 兼容性：需要明确最小可用特性集（模块系统、Promise job queue 支持程度）。
- tool trampoline 的 “fn_offset 表示” 在 Rust 编译产物上的稳定性：需要以最小 PoC 验证 “函数指针 → i32 offset” 是否与现有 runtime 一致。
- Spear WASM runtime 是否始终启用 wasmedge feature：当前 hostcall 链接逻辑在 `wasmedge` feature 下更完整，是否需要补齐 wasmtime 侧（如后续要切换 runtime 引擎）。
