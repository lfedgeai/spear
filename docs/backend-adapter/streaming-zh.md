# Realtime/Streaming 子系统设计要点

Realtime Voice/Streaming 与普通 HTTP 请求不同：它需要独立 transport（WebSocket/gRPC）、会话生命周期、以及事件驱动的增量输出。

注：当前运行时未实现 `realtime_voice`；现有 realtime 能力仅覆盖 `speech_to_text` 场景。以下内容保留为历史设计/未来规划参考。

## 1. 为什么要单独建模

legacy Go 的实时 ASR 通过 WebSocket 创建会话并 append 音频缓冲，收到 delta 事件后回写到任务侧（`legacy/spearlet/stream/rt_asr.go:46-140`）。这类能力无法用“单次 request/response”抽象覆盖。

因此建议将 realtime 明确建模为：

- 规划中的 `Operation::realtime_voice`
- `Feature::supports_bidi_stream`
- `Transport::websocket|grpc`

并把实现放到独立 stream 子系统中，但与 router/registry 共享同一套 capabilities 与 backend 实例管理。

## 2. 生命周期建议

至少包含：

- `create_session`
- `append_audio`（或 `send_input_event`）
- `commit`（结束一段输入，触发推理）
- `close_session`

输出以事件流表示：

- `delta`（部分识别/部分合成）
- `completed`
- `error`

## 3. 路由与能力约束

路由时应强制：

- 若未来恢复该能力，backend instance 需支持 `realtime_voice`
- transport 支持 `websocket|grpc`
- 并发与 session 时长上限（`max_session_seconds`）

如果无候选：

- 返回 `MissingCapabilities`
- 若允许降级：降级到 `speech_to_text`（非实时）或拒绝

## 4. 与 hostcall 的关系

两种常见形态：

- 复用“stream ctrl”式 hostcall：更贴近 legacy `MethodStreamCtrl`
- 为 realtime 单独设计 `rt_*` family：更清晰，但工作量更大

无论哪种，secret 与 URL 都必须由 host 配置提供。
