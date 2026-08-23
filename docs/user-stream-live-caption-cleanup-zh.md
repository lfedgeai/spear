# User Stream Live Caption 清理说明

## 范围

本次清理针对 `samples/wasm-rust/user_stream_live_caption` 这个 Rust sample。
目标是在不改变运行时行为、也不引入额外抽象层的前提下提升可读性。

## 变更

- 将 `rtasr_session.rs` 中待发送音频的聚合方式，从“chunk 集合 + 字节计数”简化为单一连续缓冲区。
- 移除了只为旧分块表示服务的写入前合并逻辑。
- 保留 commit flush 的最小音频门槛，但将内部状态命名调整为更贴近语义的形式。
- 将 `app.rs` 中 `loop + 嵌套 match` 的文本流读取逻辑改为更直接的 `while let` drain 循环。
- 抽取重复的字幕开启逻辑为单独 helper，让转写事件处理分支更易读。

## 验证

- 现有单元测试继续通过。
- 新增了针对待发送音频顺序以及 flush 状态重置行为的聚焦测试。
- 清理后该 sample 可以通过 `cargo clippy --tests -- -D warnings`。
