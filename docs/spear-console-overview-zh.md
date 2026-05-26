# SPEAR Console 概览

SPEAR Console 是由 SMS 提供的轻量用户前端页面，用于基于现有 stream/ws 协议与 execution 进行交互。

## 访问方式

- 入口：`http://<sms-host>:<sms-port>/console`

## 开关配置

SPEAR Console 默认启用（由 SMS HTTP gateway 提供 `/console` 路由）。可通过以下方式关闭：

- 配置文件：`enable_console = false`
- 环境变量：`SMS_ENABLE_CONSOLE=false`
- CLI：`--disable-console`（或 `--enable-console`）

## UI 设计原则（建议）

- 聊天为主：连接/调试相关能力不能挤占主聊天窗口的注意力。
- 渐进披露：默认只暴露一个 “Connect” 入口，细节按需展开。
- Same-origin 优先：避免让用户手动填写 Base URL 或复制不透明 ID。
- 状态清晰：始终可见 Connected/Connecting/Disconnected 与最近一次失败原因。
- 默认安全：限制并发请求数，明确展示限流/背压类错误。

## 布局（建议）

- 顶部栏：产品标题 + 连接状态 chip（状态 + 目标）+ `Connect` / `Disconnect`。
- 主区域：沿用现有聊天窗口（前端用户与后端 Agent 仍在这里交互）。
- 侧边抽屉：连接选择器 + 可选的 “Connection details”。

线框图：[spear-console-wireframe.svg](diagrams/spear-console-wireframe.svg)

## 连接模型

SPEAR Console 复用当前 execution stream 协议：

1) 在 “Start a chat” 弹窗中选择 Task -> Instance -> Execution。
2) Console 以同域方式调用 `POST /api/v1/executions/{execution_id}/streams/session` 获取 `ws_url`。
3) Console 通过 WebSocket 连接 `ws_url`，并使用 SSF frame 交互。
   - 连接建立（`open`）后，Console 会先发送一个初始 SSF v1 CTRL 帧（`msgType=1`，空 data），用于立刻建立 `stream_id=1`，避免等待用户首次输入。
   - 用户输入会先发送一条或多条 DATA 帧（`msgType=2`），并在输入完成后发送 COMMIT 帧（`msgType=3`）标记该段输入结束。

### 多客户端并发（同一 execution）

- 支持：多个 WebSocket client 可以同时连接到同一个 `execution_id` 的 `ws_url`（都连 SMS，而不是直接连 spearlet）。
- 实现：SMS 会对同一个 execution 复用一条到 spearlet 的 upstream WS，并对每个 client 做 `stream_id` 重写与路由，避免不同 client 使用相同 `stream_id`（通常都是 `1`）导致串话。
- 限制：不建议多个 client 直接连同一个 spearlet execution 的 `/streams/ws`（spearlet 侧当前按 `stream_id` 组织 channel，多个连接会竞争消费 outbound）。

### 可选：按 Endpoint 连接（建议）

当任务配置了 `endpoint` 时，Console 可以提供第二种连接模式：

1) 用户选择 endpoint（可搜索的 `endpoint` 列表）。
2) Console 连接 `wss://<sms-host>:<sms-port>/e/{endpoint}/ws` 并交换 SSF 帧。

聊天窗口保持不变，仅连接目标不同。

## 信息展示

SPEAR Console 将 task/instance/execution 这类“连接信息”收敛到 Info 弹窗（对话框右上角），主界面保持简洁，仅保留与聊天直接相关的操作。

## 为什么用选择弹窗

SPEAR Console 默认假设 same-origin，因此不暴露 “SMS Base URL” 输入，降低误配置与操作成本。

选择弹窗用于避免用户手动复制粘贴不透明的 ID，并能在 UI 中展示 instance/execution 的基础状态信息。

## 使用到的 SMS HTTP API

- `GET /api/v1/tasks`
- `GET /api/v1/tasks/{task_id}/instances`
- `GET /api/v1/instances/{instance_id}/executions`
- `POST /api/v1/executions/{execution_id}/streams/session`

## 常见问题排查

### Console 发消息后一直等待、SMS 日志提示 404 Not Found

典型日志：

- `register execution stream client failed ... connect upstream failed ... HTTP 404`

含义：SMS 已经收到了 Console 的 WebSocket 连接，但在连接 spearlet 上游 WS（`/api/v1/executions/{execution_id}/streams/ws`）时打到了错误的地址/端口，导致握手 404，流无法建立，Console 无法收到返回。

优先检查：

- node 注册信息是否正确：`node.ip_address` 不能是 `0.0.0.0/::`，`node.http_port` 需要指向 spearlet 的 HTTP gateway 端口。
- spearlet 是否正确“对外宣告” IP：
  - 本机跑：可设置 `SPEARLET_ADVERTISE_IP=127.0.0.1`
  - K8s：优先注入 `POD_IP`，或显式设置 `SPEARLET_ADVERTISE_IP=<PodIP/可达IP>`
