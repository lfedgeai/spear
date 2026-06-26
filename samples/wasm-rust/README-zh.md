# WASM-Rust 示例

本目录包含以 Rust 为主的 WASM 示例；这类示例直接使用 Rust guest SDK，而不是通过 Boa JS runner 间接执行。

这些示例共享的 `user stream` / `SSF` / `RTASR` 辅助逻辑现已迁入 SDK 高层 crate `sdk/rust/crates/spear-wasm-helper`。

## 示例列表

- `user_stream_live_caption`：基于 `spear-wasm`、`spear-ssf` 以及 Rust 版 RTASR / user-stream hostcall wrapper 实现的模块化实时字幕示例
  - 源码：`./user_stream_live_caption/src/main.rs`
  - 产物：`../build/rust/user_stream_live_caption.wasm`
- `user_stream_voice_chat`：模块化的语音对话示例，在 `stream_id=2` 上完成 RTASR 转写，再将下游 Chat Completion 结果写回 `stream_id=1`
  - 源码：`./user_stream_voice_chat/src/main.rs`
  - 产物：`../build/rust/user_stream_voice_chat.wasm`

## 构建

在仓库根目录执行：

```bash
make samples-rust
```

如果要同时构建全部 sample 家族：

```bash
make samples
```

## 设计说明

- 通过 Rust guest SDK 的 `epoll` wrapper 实现事件循环，因此不需要 busy sleep loop，也更容易阅读。
- 将 sample 专属编排保留在各自 sample 中，同时把共享的 `stream` 与协议辅助逻辑抽到 `sdk/rust/crates/spear-wasm-helper`，便于扩展且不过度抽象。
- 同时把共享的 RTASR 会话骨架抽到 `spear-wasm-helper`，让各 sample 自己的 `rtasr_session.rs` 只保留策略差异。
- `user_stream_live_caption` 当前行为与 live caption 主链路保持一致：
  - `stream_id=1` 用于字幕输出
  - `stream_id=2` 用于语音上行
  - `server_vad` 使用 `silence_ms=600`
  - 每次 `rtasr.write` 前大约聚合 `240ms` PCM
  - 兼容 `conversation.item.input_audio_transcription.*` 事件名
- `user_stream_voice_chat` 当前行为与 voice chat 主链路保持一致：
  - `stream_id=2` 上送语音，再由 RTASR 完成转写
  - 收到 voice COMMIT 后再 flush RTASR
  - 将最终 transcript 与下游 Chat Completion 回复写回 `stream_id=1`
