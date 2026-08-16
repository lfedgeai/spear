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
   - 连接建立（`open`）后，Console 会为每个需要使用的 `stream_id` 先发送 SSF v1 CTRL(OPEN) 帧（`msgType=1`），之后才允许发送 DATA/COMMIT（不做兼容）。
   - 文本输入（默认 `stream_id=1`）：发送一条或多条 DATA 帧（`msgType=2`），并在输入完成后发送 COMMIT 帧（`msgType=3`）标记该段输入结束。
   - 语音输入（可选 `stream_id=2`）：按住说话开始时发送 CTRL(UTTERANCE_BEGIN)，持续发送 DATA(audio chunk)，松开时发送 COMMIT。完整协议见：[spear-console-voice-input-design-zh.md](./spear-console-voice-input-design-zh.md)。

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

## 共享前端样式层

- Console 与 Admin 现在共用同一份主题 token 源：[`theme.css`](../web-admin/src/shared/theme.css)。
- 共享主题 token 现在也覆盖了 Console 使用到的语义色与表面语义，例如 success/warning、遮罩层基色和阴影基色，因此 dark/light 切换时不只是背景前景一致，状态与层次也会一起保持协调。
- Console 与 Admin 也共用同一个主题应用 Hook：`web-admin/src/shared/theme-mode.ts`，统一维护 `class`、`data-theme`、`color-scheme` 与本地持久化状态。
- Console 直接复用 `web-admin/src/shared/components/ui/` 下的 Admin 风格基础组件，包括共享的 `Button`、`Input`、`Textarea`、`Card` 与 `Select`。
- Console 的语音控件也应尽量复用同一套按钮语言，使麦克风操作、split control 与模式菜单在视觉上继续与 Admin 基础控件保持一致。
- `web-admin/src/index.css` 与 `web-console/src/index.css` 都会引入同一个共享主题入口，因此 light/dark 颜色、字号基线、间距节奏与基础控件高度会保持一致。
- 顶部状态 chip 也应遵循同一套共享圆角语言，避免在周围控件都使用 Admin 盒状圆角时单独保留胶囊形态。
- 侧栏 eyebrow、分组标题、面板 caption 这类辅助标签，也应与 Admin 的次级文字保持同样克制的字重与字距节奏。
- 分区标题、面板标题、弹窗标题以及配套说明文字，也应共享同一套排版基线，而不是让 Console 的每个区域各自维护一套局部字号与行高。
- 落到实际规则上，Console 的一级标题应与 Admin header title 使用同一套 `text-sm / font-semibold / leading-5` 基线；配套说明文字则应统一为 `text-xs / leading-4`。
- 正文、只读值、状态文字、辅助说明和 badge 文本，也不应继续保留 `11px` 这类 Console 本地小字号；它们应回到与 Admin 一致的 `text-xs` 或 `text-sm` 基线，并使用稳定统一的行高，而不是继续混用 `1.45~1.65` 一类局部数值。
- readonly block、connection details、code block、endpoint row、chat bubble 这类信息密集组件，也应复用同一套紧凑密度规则，避免它们的内部 padding 和文字节奏再次偏离 Admin 的系统化风格。
- 间距体系也应优先收敛到 `4 / 8 / 12` 这类紧凑共享步进，避免在 header、card、section、drawer 之间混入过多彼此接近但不统一的局部数值。
- `web-console/src/main.css` 中保留的 console 专属样式，应只负责页面结构和交互布局；基础控件的视觉定义应尽量继续来自共享 primitive。
- 随着 shared primitive 覆盖范围扩大，`web-console/src/main.css` 中遗留的旧控件类应优先删除，而不是继续在原地重复换皮。

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
