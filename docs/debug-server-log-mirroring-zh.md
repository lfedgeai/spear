# Debug Server 日志镜像

## 概览

SPEAR 现在会把 WASM guest 日志镜像到 debug server，同时保留原有 execution log 链路。

这样不会改变现有 execution logs 的语义，但可以让 guest 侧排障更容易直接在 debug server UI 中完成。

## 当前行为

- WASM guest 内部的 `spear.log(...)` 仍然会写入本地 WASM log ring
- 这些日志仍然会以 `wasm` stream 刷到 SMS execution logs
- 同一条日志现在也会额外镜像到 debug server

## Debug Server 事件结构

镜像后的 WASM 日志会以 debug event 的形式发送，包含：

- `streamName = wasm-log`
- `msg = <原始日志消息>`
- `data.eventType = log`
- `data.source = wasm`
- `data.level = <trace|debug|info|warn|error>`
- `data.taskId`
- `data.executionId`
- `data.instanceId`

debug server 仍然沿用已有的 session 和 client 配置，因此现有的 `session / client / stream` 选择模型保持不变。

## 这样设计的原因

- 不会破坏或替换现有 execution log pipeline
- 仍然保持 SMS 是 runtime execution logs 的主链路
- 额外提供一个更适合快速排障的观测面
- 每条镜像日志都保留 instance 和 execution 上下文
