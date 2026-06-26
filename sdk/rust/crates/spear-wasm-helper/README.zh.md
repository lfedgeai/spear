# spear-wasm-helper

构建在 `spear-wasm` 之上的 Rust 高层辅助 crate。

## 定位

`spear-wasm-helper` 位于低层 `spear-wasm` hostcall wrapper 与具体 WASM sample / app 之间。

当你需要下面这些能力时，优先使用这个 crate：

- 可复用的 `user stream` 状态管理
- 可复用的 `SSF v1` / stream 协议解析
- 可复用的 RTASR transcript 事件解析
- 面向 app 编排的 RTASR 基础会话骨架

但不要把 sample 私有状态机或产品行为直接堆到这个 crate 里。

## 分层

- `spear-wasm-sys`：原始 hostcall 绑定
- `spear-wasm`：安全的低层 Rust wrapper
- `spear-wasm-helper`：面向 Rust-on-WASM app 的高层可复用 building block
- `samples/wasm-rust/*`：具体 app 状态机与业务行为

## 模块

- `time_format`：秒级 `[HH:MM:SS]` 前缀格式化
- `ctl_pump`：`user_stream_ctl_*` 事件 draining 辅助
- `event_loop`：轻量 `EpollDriver` 与 ready-event wrapper
- `app_event`：把底层 ready event 分类成逻辑 `AppEvent`
- `stream`：`ManagedStream`、`StreamEndpoint`、`BufferedTextOutput`
- `user_stream_protocol`：`SSF v1` / `CTRL(open)` / `utterance_begin` 解析辅助
- `rtasr_protocol`：transcript 事件解析辅助
- `rtasr_session`：`BaseRtasrSession`、`RtasrConnectOptions`

## 适用场景

- Rust-first WASM 示例
- 运行在 Spear 中的小型 Rust guest app
- 比 `spear-wasm` 更高层、但仍保持通用性的 guest-side 编排辅助模块

## 非目标

- `live_caption`、`voice_chat` 这类产品级流程本身
- 下游 chat orchestration
- 泛化文本流缓冲之外的 UI 展示策略
