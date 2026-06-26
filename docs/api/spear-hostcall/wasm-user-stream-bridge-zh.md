# Spear Hostcall 工程设计：面向 WASM 的双向用户流桥接（v1）

## 0. 目的

本文提出一个业界 best practice 导向的方案，用于支持外部客户端与 Spear WASM 实例之间的**双向流式输入/输出**，包含：

- 面向网络侧的传输层（优先 WebSocket；可选 gRPC streaming），适配浏览器与实时媒体。
- 面向未来演进的**二进制帧格式**：既能承载 raw bytes，也能携带类型化元信息（音频/视频/文本等）。
- 面向 WASM 的 I/O 模型：确定性、避免重入、可扩展 —— **fd + 非阻塞 read/write + epoll 就绪通知**。

该设计刻意复用 Spear 已有的 fd/epoll 子系统与惯例：

- 通用 fd/epoll 子系统：[fd-epoll-subsystem-zh.md](./fd-epoll-subsystem-zh.md)
- 实时 ASR（流式 fd 的具体示例）：[realtime-asr-epoll-zh.md](./realtime-asr-epoll-zh.md)

---

## 1. 目标 / 非目标

### 1.1 目标

- 客户端 ⇄ spearlet ⇄ WASM guest 的**双向流式**。
- **协议兼容性**：版本化帧、可扩展 header、为音视频等模态提供类型化 metadata。
- **背压**：有界队列 + 显式流控；禁止无界缓存。
- **WASM 友好语义**：非阻塞 `read/write`（`-EAGAIN`），通过 epoll 避免 busy loop。
- **安全边界**：WASM 不直接接触网络凭证；host 统一做鉴权/授权/配额/限流。

说明：这里的 `EAGAIN/EBADF/...` 使用 **SPEAR 虚拟 errno 常量**（与 C/Rust WASM SDK 对齐，例如 `SPEAR_EAGAIN=11`），而不是宿主 OS 的 `libc` errno 值。这样可以避免 macOS 与 Linux 的 errno 数值差异导致 guest 侧无法识别错误码。

### 1.2 非目标（v1）

- 完整 WASI sockets 兼容。
- WebRTC（datachannel/media）传输。
- 跨 execution 的多租户 fanout（v1 以单 execution/session 为主）。

---

## 2. 高层架构

### 2.1 数据流

客户端（浏览器/服务）连接到 stream gateway，并绑定到某个 execution 的 stream session：

1. 客户端 → SMS（HTTP）：调用 `POST /api/v1/executions/{execution_id}/streams/session` 创建短期 stream session token。
2. 客户端 → SMS（WebSocket）：连接 `GET /api/v1/executions/{execution_id}/streams/ws?token=...` 并发送 SSF 帧。
3. SMS：校验 token，并将 WebSocket 代理到目标 spearlet。
4. Spearlet：将 inbound SSF 帧写入 execution-local stream hub，并通过 fd/epoll hostcall 暴露给 WASM。
5. WASM guest：通过 fd 风格的 `user_stream_*` hostcall 做 `read/write`，通过 `spear_epoll_*` 等待就绪。
6. Spearlet → SMS → 客户端（WebSocket）：转发 WASM 写出的 outbound 帧。

### 2.2 为什么推荐 WASM 侧使用 “fd + epoll”

### 2.3 Rust SDK 与 Boa JS 用法

- Rust：`sdk/rust/crates/spear-wasm` 提供 `user_stream_*` / `user_stream_ctl_*` / `spear_epoll_*` 的安全封装。
- Rust helper 层：`sdk/rust/crates/spear-wasm-helper` 提供可复用的 `ManagedStream`、SSF/user stream 协议解析以及文本输出缓冲能力，适合 Rust-first guest app 直接复用。
- Boa JS：`sdk/rust/crates/spear-boa` 暴露 `Spear.userStream`，JS 可直接 open/read/write，而无需处理原始指针。
- Boa JS helper 模块：`spear/time_format`、`spear/rtasr_event`、`spear/stream_gate` 为 JS-first guest app 提供可复用的时间戳、RTASR 事件与 user stream gate 辅助能力。

向 WASM 实例交付 inbound 数据，常见有两种方案：

- **(A) host 在数据到达时“回调”调用 guest 函数**。
- **(B) guest 通过 `read/write` 主动拉取**，并用 epoll 等待。

业界 best practice 推荐：**(B)**。

原因：

- 避免 **重入（re-entrancy）** 与“host 夹在 guest 调用栈中间”的复杂场景（很难做安全且确定）。
- 与 Spear 现有 hostcall 习惯一致（`rtasr_*`、`mic_*`、`spear_fd_ctl`、`spear_epoll_*`）。
- 让 **单线程 guest 事件循环** 能够可预测地复用多个 stream/fd。

---

## 3. 传输层：WebSocket（优先）+ gRPC Streaming（可选）

### 3.1 WebSocket 作为优先方案

选择 WebSocket 的原因：

- 浏览器原生支持的双向二进制传输。
- 适合实时音视频 chunk 与增量输出。
- 相比“模拟全双工的 HTTP streaming”更易部署、更直观。

推荐的 WebSocket 属性：

- 路径（gateway，推荐）：`GET /api/v1/executions/{execution_id}/streams/ws?token=...`
- Stream session 创建（gateway，推荐）：`POST /api/v1/executions/{execution_id}/streams/session`
- 路径（spearlet，内网/直连）：`GET /api/v1/executions/{execution_id}/streams/ws`
- 子协议：`Sec-WebSocket-Protocol: spear.stream.v1`
- 数据面：**仅二进制帧**；文本帧可用于调试（非必须）。

### 3.2 鉴权 / 授权

best practice：

- 沿用现有 HTTP/gRPC gateway 的鉴权方式（token/cookie/mtls），但必须做**execution 级授权**。
- 对公网的 gateway WS 端点，建议要求短期有效的 **stream session token**。

握手携带凭证的选项：

- Upgrade 请求的 `Authorization: Bearer ...` header（推荐）。
- 或 `?token=...` query（不推荐；必须短期有效，且要防日志泄露）。

### 3.3 gRPC streaming（可选替代）

面向服务间调用（非浏览器）时，gRPC bidi stream 更适合。

若引入，建议复用与 WS 相同的逻辑消息与流控，on-wire 可直接承载 `bytes frame`（SSF）。

---

## 4. On-wire 帧协议：Spear Stream Frame（SSF）

### 4.1 为什么不直接用 “raw bytes”

仅 raw bytes 不利于长期演进：

- 无版本 → 变更难以兼容。
- 无类型 → OPEN/CLOSE/ACK/ERROR 等控制语义只能靠临时约定。
- 无元信息 → 音视频 codec、timestamp、content-type 等只能靠业务层拼凑。

因此 v1 采用一个紧凑且可扩展的二进制 envelope：**Spear Stream Frame（SSF）**。

### 4.2 SSF v1 帧结构（小端）

每个 WebSocket 二进制 message 内包含且仅包含一个 SSF frame。

Header（固定 32 字节）：

| Offset | Size | 字段 | 含义 |
|---:|---:|---|---|
| 0 | 4 | `magic` | ASCII `"SPST"`（`0x53505354`） |
| 4 | 2 | `version` | `1` |
| 6 | 2 | `header_len` | `32`（未来可扩展） |
| 8 | 2 | `msg_type` | 见下文 |
| 10 | 2 | `flags` | bitset |
| 12 | 4 | `stream_id` | session 内逻辑 stream |
| 16 | 8 | `seq` | 发送方序号（按 direction + stream_id 计数） |
| 24 | 4 | `meta_len` | metadata 段字节数 |
| 28 | 4 | `data_len` | data 段字节数 |

Body：

- `meta` bytes（长度 = `meta_len`）
- `data` bytes（长度 = `data_len`）

约束：

- `header_len` 必须 >= 32；当 `header_len > 32` 时，接收方必须忽略未知的 header 扩展字节。
- `meta_len + data_len` 必须等于 WS payload size 减去 `header_len`。

### 4.3 消息类型（v1）

当前实现只要求 SSF v1 header 合法（magic/version/header_len），并使用 `stream_id` 做路由；`msg_type` 会被解析并透传给 guest，但 host 不解释其语义。

推荐的 v1 约定：

- `1 = CTRL`：
  - 控制类帧（例如 OPEN/keepalive）。
  - `meta`：JSON UTF-8（可选）。
  - `data`：通常为空。
- `2 = DATA`：
  - `meta`：可选（每 chunk 覆盖；性能考虑建议常为空）。
  - `data`：raw bytes（音频/视频/文本或任意二进制）。
- `3 = COMMIT`：
  - 标记一次用户输入片段的结束（应用层语义）。
  - `meta`：JSON UTF-8（可选）。
  - `data`：通常为空。

其他消息类型（OPEN/COMMIT/CLOSE/ACK/ERROR 等）作为后续扩展预留。
- `4 = CLOSE`：
  - 半关闭或全关闭（由 flags 决定）。
- `5 = ACK`：
  - 流控确认 / window update（见下文）。
- `6 = ERROR`：
  - `meta`：JSON error 对象；`data`：可选诊断 bytes（需限长）。
- `7 = PING`，`8 = PONG`：
  - 作为应用层保活补充（WS 自带 ping/pong 之外的可选机制）。

### 4.3.1 SDK 常量

- C：`sdk/c/include/spear_ssf.h` 提供 `SPEAR_SSF_MSG_TYPE_CTRL/DATA/COMMIT`。
- Rust（guest）：`spear_wasm::ssf::SsfMsgType`。
- JS（web-console）：`web-console/src/ssf/index.ts` 导出 `SsfMsgType` 以及 `encodeSsfV1Frame/decodeSsfV1Frame`。
- JS（Boa 运行时）：`import * as ssf from "spear/ssf"` 导出 `MsgType`。

### 4.4 元信息约定

v1 的 metadata 编码：**JSON UTF-8**（便于调试与跨语言互通）。

推荐字段（示例）：

- `OPEN`：
  - `session_id`（string）
  - `content_type`（string，例如 `audio/pcm;rate=16000;channels=1`）
  - `codec`（string，例如 `pcm_s16le`、`opus`、`h264`）
  - `timebase`（object，例如 `{ "unit": "ms" }`）
  - `limits`（object，服务端下发的生效限制）
- `DATA`：
  - `timestamp_ms`（number）
  - `duration_ms`（number）
  - `is_keyframe`（bool，用于视频）

接收方必须忽略未知字段。

---

## 5. 背压与流控

### 5.1 Host 侧有界队列（必须）

对每个 `(execution_id, stream_id, direction)`：

- `recv_queue`：client→guest 的 inbound 消息
- `send_queue`：guest→client 的 outbound 消息

每个队列必须受以下限制：

- `max_queue_bytes`
- `max_frame_bytes`
- `max_frames`

溢出策略（推荐默认）：

- Inbound（client→guest）：发送 `ERROR` 并关闭（保护执行确定性，避免 host 无界缓存）。
- Outbound（guest→client）：对 guest write 施加 `-EAGAIN` 背压。

### 5.2 基于 credit 的流控（推荐）

为了避免“发送方过快导致接收方被动堆积”，SSF v1 通过 `ACK` 引入显式 credit：

- 每个方向维护 `credit_bytes`。
- 发送方不得发送超过剩余 credit 的 `DATA`（以 outstanding bytes 计）。
- 接收方通过 `ACK` 追加 credit：
  - `meta`：`{ "grant_bytes": <u64>, "ack_seq": <u64> }`

一个最小可用的 v1 也可以：

- 在 `OPEN.meta.limits.initial_credit_bytes` 中给初始 credit。
- 仅在队列消费推进时发送 `ACK`。

### 5.3 WASM 可见的背压语义

guest 侧通过以下方式感知背压：

- `user_stream_write(...) -> -EAGAIN`：当 outbound 队列满且 fd 为 non-blocking。
- `EPOLLOUT`：当 capacity 恢复时触发就绪。

---

## 6. WASM 侧 ABI（Hostcalls）

本节定义一个新的 hostcall family：**`user_stream_*`**，并与统一 fd/epoll 子系统集成。

### 6.1 句柄模型

- guest 可见句柄为 `i32 fd`。
- `fd` entry 存储于统一 `FdTable`。
- 就绪性通过 `spear_epoll_*` 暴露。
- 通用控制通过 `spear_fd_ctl` 完成（flags/status/metrics）。详见 [fd-epoll-subsystem-zh.md](./fd-epoll-subsystem-zh.md)。

### 6.2 函数签名（v1）

创建/打开：

- `user_stream_open(stream_id: i32, direction: i32) -> i32`
- `user_stream_ctl_open() -> i32`

I/O：

- `user_stream_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32`
- `user_stream_write(fd: i32, buf_ptr: i32, buf_len: i32) -> i32`
- `user_stream_ctl_read(fd: i32, out_ptr: i32, out_len_ptr: i32) -> i32`

关闭：

- `user_stream_close(fd: i32) -> i32`

常量：

- `direction`：
  - `USER_STREAM_DIR_IN = 1`（client → guest）
  - `USER_STREAM_DIR_OUT = 2`（guest → client）
  - `USER_STREAM_DIR_BIDI = 3`

返回值：

- `>= 0`：成功（write 返回写入字节数；read 返回读到的字节数；close/open 可返回 `0`）
- `< 0`：`-errno`（例如 `-EAGAIN`、`-EBADF`、`-EINVAL`、`-ENOTCONN`、`-EPIPE`）

### 6.3 read/write payload 约定

为了减少边界转换开销并保留未来扩展能力：

- `user_stream_read` 每次返回**一个完整 SSF frame**（二进制）。
- `user_stream_write` 每次写入**一个完整 SSF frame**（二进制）。

Host 责任：

- 校验 `magic/version/header_len`。
- 强制 `max_frame_bytes`。
- 强制方向：IN fd 禁止 write；OUT fd 禁止 read。
- 任何队列状态变化都要更新 fd readiness，并唤醒 epoll watcher。

Guest 责任：

- 提供 out buffer，并遵守 “buffer-too-small” 惯例：
  - 若 `*out_len_ptr < need`，host 写回 `need` 并返回 `-ENOSPC`。

该约定与仓库现有 hostcall 与 `spear_epoll_wait` 的 sizing 风格一致。

### 6.4 UserStreamFd 的 epoll 就绪语义

就绪位：

- `EPOLLIN`：inbound 队列非空（`user_stream_read` 会成功）
- `EPOLLOUT`：outbound 队列有容量（`user_stream_write` 会成功）
- `EPOLLERR`：session/transport 错误
- `EPOLLHUP`：对端关闭或 fd 已关闭

Level-triggered 规则：

- 条件成立期间，`spear_epoll_wait` 必须持续报告。

### 6.5 Guest 侧推荐用法（单线程事件循环）

推荐模式：

1. （可选）发现可用 stream：
   - `ctl_fd = user_stream_ctl_open()`
   - 将 `ctl_fd` 用 `EPOLLIN` 注册到 epoll
   - `user_stream_ctl_read` 返回固定 8 字节事件：`(u32 stream_id, u32 kind)`
2. 创建数据 fd：
   - `in_fd = user_stream_open(stream_id, USER_STREAM_DIR_IN)`
   - `out_fd = user_stream_open(stream_id, USER_STREAM_DIR_OUT)`（或 BIDI）
2. 注册到 epoll：
   - `epfd = spear_epoll_create()`
   - `spear_epoll_ctl(epfd, ADD, in_fd, EPOLLIN)`
   - `spear_epoll_ctl(epfd, ADD, out_fd, EPOLLOUT)`
3. 循环：
   - `spear_epoll_wait(epfd, ...)`
   - 读 fd：一直读到 `-EAGAIN`
   - 写 fd：写到 `-EAGAIN` 为止

---

## 7. Host 侧实现设计（Rust，函数级）

本节描述建议的模块划分与函数职责（名称为示意，应遵循 `src/spearlet/execution/` 现有风格）。

### 7.1 数据结构

新增一个 fd kind：

```rust
pub enum FdKind {
    // ...
    UserStream,
}
```

以及对应的 fd inner state：

```rust
pub struct UserStreamState {
    pub stream_id: u32,
    pub direction: UserStreamDirection,
    pub conn_state: UserStreamConnState,

    pub recv_queue: std::collections::VecDeque<Vec<u8>>,
    pub recv_queue_bytes: usize,

    pub send_queue: std::collections::VecDeque<Vec<u8>>,
    pub send_queue_bytes: usize,

    pub limits: UserStreamLimits,
    pub metrics: UserStreamMetrics,
    pub last_error: Option<String>,
}
```

### 7.2 WebSocket session manager

实现位置：

- `src/spearlet/http_gateway.rs`（`user_stream_ws_loop`）

核心职责：

- HTTP Upgrade 到 WebSocket，并强制子协议 `spear.stream.v1`。
- 解析 inbound WS 二进制消息为 SSF frame。
- 将 frame 路由到 `(execution_id, stream_id)` 的 inbound 队列。
- 从 outbound 队列取 frame 并发送到 WS。
- 处理 close/error，并把状态映射成 `EPOLLHUP/EPOLLERR`。

函数级设计（示意）：

```rust
pub async fn handle_user_stream_ws(
    state: AppState,
    execution_id: String,
    ws: WebSocketUpgrade,
) -> impl IntoResponse;

async fn ws_loop(
    hub: Arc<ExecutionUserStreamHub>,
    ws: axum::extract::ws::WebSocket,
) -> Result<(), UserStreamWsError>;
```

### 7.3 execution 级 stream hub

目的：解耦 WS transport 与 fd table entry，并允许受控 attach。

实现位置：

- `src/spearlet/execution/host_api/user_stream.rs`（`ExecutionUserStreamHub`）

核心 API：

```rust
pub struct ExecutionUserStreamHub {
    pub execution_id: String,
    pub streams: dashmap::DashMap<u32, StreamChannel>,
    pub authz: UserStreamAuthz,
    pub limits: UserStreamLimits,
}

pub struct StreamChannel {
    pub inbound: StreamQueue,
    pub outbound: StreamQueue,
    pub watchers: StreamWatchers,
    pub conn_state: UserStreamConnState,
}

impl ExecutionUserStreamHub {
    pub fn attach_ws(&self, stream_id: u32, ws_peer: WsPeerInfo) -> Result<(), UserStreamError>;
    pub fn detach_ws(&self, reason: UserStreamCloseReason);

    pub fn push_inbound_frame(&self, stream_id: u32, frame: Vec<u8>) -> Result<(), UserStreamError>;
    pub fn pop_outbound_frame(&self, stream_id: u32) -> Option<Vec<u8>>;
}
```

### 7.4 Hostcall glue（WASM imports）

建议接入点：

- `src/spearlet/execution/runtime/wasm_hostcalls.rs`：
  - 增加 `user_stream_open/read/write/close`
  - 复用现有 linear memory helper 约定（`mem_read`、`mem_write_with_len`）

Hostcall 实现（示意）：

```rust
pub fn user_stream_open(host: &mut DefaultHostApi, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
pub fn user_stream_read(host: &mut DefaultHostApi, instance: &mut Instance, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
pub fn user_stream_write(host: &mut DefaultHostApi, instance: &mut Instance, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
pub fn user_stream_close(host: &mut DefaultHostApi, input: Vec<WasmValue>) -> Result<Vec<WasmValue>, CoreError>;
```

在 `DefaultHostApi` 内（示意）：

```rust
impl DefaultHostApi {
    pub fn user_stream_open(&self, stream_id: i32, direction: i32) -> i32;
    pub fn user_stream_read(&self, fd: i32) -> Result<Vec<u8>, i32>;
    pub fn user_stream_write(&self, fd: i32, bytes: &[u8]) -> i32;
    pub fn user_stream_close(&self, fd: i32) -> i32;
}
```

### 7.5 readiness 重计算与唤醒

遵循 fd/epoll 子系统的统一最佳实践：

- 任何队列/状态变化都必须：
  - 重计算 fd entry 的 `poll_mask`
  - 唤醒 watcher（`fd_table.notify_watchers(fd)`）

---

## 8. 故障处理

### 8.1 断连行为

- 客户端 WS close：
  - inbound：IN fd 置 `EPOLLHUP`；read 先 drain 剩余队列，之后按 EOF 策略返回（`0` 或 `-EPIPE`，需统一）。
  - outbound：后续 write 返回 `-EPIPE`。

### 8.2 错误映射

推荐 `-errno` 映射：

- 解析错误 / 非法 frame：`-EINVAL`（同时可回写 SSF `ERROR`）
- 授权失败：`-EACCES`（关闭）
- 未绑定 execution：`-ENOTCONN`
- 队列满：guest write 返回 `-EAGAIN`；client inbound 建议回 `ERROR` 并关闭以保护确定性

---

## 9. 可观测性（必须）

按 execution 与 stream_id 输出指标：

- `in_frames_total`、`out_frames_total`
- `in_bytes_total`、`out_bytes_total`
- `dropped_frames_total`（按原因分组）
- `queue_bytes`、`queue_len`
- `ws_disconnects_total`、`errors_total`

并通过 `spear_fd_ctl(..., GET_STATUS/GET_METRICS, ...)` 提供 JSON 便于调试。

---

## 10. 测试计划（推荐）

- 单测：
  - SSF parse/validate（magic/version/header_len；长度校验）
  - 队列上限与溢出策略
  - readiness 转换（IN/OUT/ERR/HUP）与 watcher 唤醒
- 集成测试：
  - WS client 推 DATA；guest 通过 `user_stream_read` + epoll 收到
  - guest 写；WS client 收到 frame
  - 背压：outbound 队列满时 guest write 返回 `-EAGAIN`
  - 断连：`EPOLLHUP` 可观测

文档版本：v1（2026-03-18）。
