# SMS Endpoint Gateway 设计（WS → WS → Instance）

## 1. 背景与目标

希望在 SMS（Metadata Server）侧提供一个统一的 WebSocket 路由，用于承载所有已注册 task 的 `endpoint` 调用。endpoint 约束为仅允许字母数字下划线和减号（`[A-Za-z0-9_-]+`）。当客户端连接到某个 endpoint 后：

- SMS 根据 endpoint 找到对应 task
- SMS 查找一个可用的 execution（其所属 instance 可处理该任务），并连接到该 execution 的 WS 数据面
- 若没有可用 execution：触发启动一个 execution，待可用后再连接
- SMS 在客户端 WS 与上游 execution WS 之间做双向代理（proxy）
- 客户端与任务侧通过 SSF（二进制帧）进行多路复用的请求/响应交互

目标：

- 一个 endpoint 像“长连接函数入口”：`GET /e/{endpoint}/ws` 建立 WS 后即可交互
- 数据面复用现有 user stream + SSF + Spearlet WebSocket，不引入新的底层协议
- 具备可观测性、超时、错误归因与资源保护（限流/背压/熔断），便于线上排障与稳定性保障

非目标（本期不做）：

- 多租户隔离与复杂配额系统（仅预留扩展点）
- 完整 OpenAPI 动态生成（可选）
- 流式返回（先做 request/response，后续可扩展）
- 连接级会话恢复/断点续传（先不做 resume，后续可扩展）

## 2. 现状与依赖

### 2.1 现状

- Task 注册接口已包含 `endpoint` 字段，并在 SMS 侧存储，但当前不参与执行链路。
- SMS 已有 execution stream session + WS proxy 机制：
  - 控制面：`POST /api/v1/executions/{execution_id}/streams/session` 生成短期 token（TTL=60s）
  - 数据面：`GET /api/v1/executions/{execution_id}/streams/ws?token=...` proxy 到 Spearlet
- Spearlet 节点提供 `GET /api/v1/executions/{execution_id}/streams/ws`，将 WS 二进制帧写入/读出 execution 维度的 user stream hub。

### 2.2 关键约束

- user stream hub 以 `execution_id` 作为隔离 key。因此 endpoint 调用必须绑定到某个 `execution_id`，才能复用现有数据面。
- Host(Linux) errno 与 WASI errno 编号可能不同。协议/SDK 侧必须使用 Spear ABI 常量，不能用 `libc::*` 做跨平台判定。

## 3. API 设计

### 3.1 Endpoint 路由

新增：

- `GET /e/{endpoint}/ws`（WebSocket Upgrade）

规则：

- `endpoint` 必须匹配正则：`^[A-Za-z0-9_-]+$`
- 建议最大长度：64（可配置）
- 建议规范化：存储与匹配均采用 lower-case（避免大小写歧义），并将大小写视为不敏感（case-insensitive）
- 建议通过 `Sec-WebSocket-Protocol` 明确应用子协议版本（例如 `ssf.v1`），便于兼容升级

### 3.2 Task 注册约束

Task 注册时对 `endpoint` 做校验与索引：

- 非空
- 正则匹配
- 全局唯一（或未来按 namespace 唯一）

### 3.3 错误码

WS 握手阶段（HTTP 状态码）：

- 400 `INVALID_ENDPOINT`：endpoint 不合法
- 404 `ENDPOINT_NOT_FOUND`：endpoint 未注册
- 409 `ENDPOINT_CONFLICT`：重复注册（注册阶段）
- 401 `UNAUTHENTICATED`：未提供或提供了无效凭证
- 403 `FORBIDDEN`：无权限访问该 endpoint（预留 ACL/策略点）
- 429 `RATE_LIMITED`：触发连接级/用户级/endpoint 级限流
- 413 `FRAME_TOO_LARGE`：客户端声明或发送的帧超过限制（握手阶段仅能做静态校验）
- 503 `NO_EXECUTION_AVAILABLE`：无可用 execution 且无法启动
- 504 `UPSTREAM_TIMEOUT`：启动/连接上游超时
- 502 `UPSTREAM_WS_FAILED`：上游 WS 连接/转发失败

WS 建连成功后（WebSocket Close 或协议内 error frame）：

- 4400 `INVALID_PROTOCOL`：客户端帧不符合 SSF/约定
- 4500 `TASK_ERROR`：任务侧返回 error frame
- 4501 `UPSTREAM_OVERLOADED`：上游过载/背压（建议由 SMS 统一转换并可选重试路由）
- 4502 `STREAM_NOT_FOUND`：收到未知 stream_id（映射不存在或已回收）

建议：

- 连接级错误优先使用标准 WS close code（例如 1008/1011）并尽量给出简短 reason；自定义业务错误码更适合放在 SSF error frame 的结构化 `code` 中，避免不同客户端对 close code 的兼容性差异
- stream 级错误尽量使用 SSF error frame 终结该 stream，而非直接关闭整条 WS（除非协议违规/资源保护）

## 4. 执行模型

### 4.1 endpoint → task

为避免全表扫描，建议在 SMS 侧维护索引：

- `endpoint -> task_id`

索引在 `RegisterTask/UnregisterTask` 时更新。

### 4.2 task → instance / execution

选择策略（最小可用版）：

1. `ListTaskInstances(task_id)` → 过滤 active & fresh
2. 对候选 instance 选择一个“可用 execution”（例如最近的 running execution，或按策略挑选）
3. 若没有可用 execution：触发 placement / invocation 启动一个 execution（并由 Spearlet 拉起/复用 instance）

备注：

- task 不再绑定单一 `node_uuid`；placement 基于 task 的 `desired_replicas`、`scheduling_strategy` 与当前 active instances 分布做选择
- MVP 已支持 `SPREAD`：优先选择当前该 task 实例数更少的 node
- 后续可以引入负载、并发、亲和性、最近使用时间等排序

### 4.3 execution 绑定策略

本期建议采用“按 endpoint 复用 execution”的模型：

- 一个 endpoint 对应的任务可通过同一个 execution（长驻）处理多个请求（通过 SSF stream_id 多路复用）
- SMS 在建立 endpoint WS 时优先复用可用 execution；若不存在，则启动一个 execution 并连接
- 对 execution 的生命周期管理：支持 idle timeout 自动回收；也支持固定常驻（配置化）

### 4.4 多 instance 的 multiplex 与负载均衡

当同一个 task 在多个 node 上有多个 instance（进而对应多个可用 execution）时，endpoint WS 需要在这些 execution 之间做 multiplex。关键结论：

- 负载均衡粒度应为“逻辑流”（SSF `stream_id`），而不是 endpoint 级别固定绑定单个 execution。
- 为了允许多个客户端共享同一个 execution，SMS 必须做 `stream_id` 的命名空间隔离（重写/映射），否则不同客户端使用相同 `stream_id` 会在 execution 内冲突。
- 当前实现优先保留原始 `client_stream_id`（例如常见的 `1/2` 文本/语音流）只要 execution 内尚未被占用；只有发生冲突时才分配新的 `upstream_stream_id`。这样单客户端 user-stream workload 仍能保持稳定流语义，多客户端场景仍保留隔离能力。

#### 4.4.1 分配策略（per-stream routing）

对每个 endpoint WS 连接，SMS 维护一个路由表：

- `(client_conn_id, client_stream_id) -> upstream_execution_id`
- `(client_conn_id, client_stream_id) -> upstream_stream_id`

当 SMS 收到客户端 WS 的一帧 SSF 数据：

1. 解析 SSF header 得到 `client_stream_id`
2. 若该 `(client_conn_id, client_stream_id)` 尚未绑定上游：
   - 从候选 execution 池选择一个 execution（见下文策略）
   - 为该 execution 分配一个在 execution 内唯一的 `upstream_stream_id`
   - 记录映射并计数（execution 的 active_streams +1）
3. 将 frame 的 `stream_id` 重写为 `upstream_stream_id`
4. 转发到对应 execution 的上游 WS

当 SMS 从上游 WS 收到一帧响应：

1. 解析 `upstream_stream_id`
2. 反向查找 `(client_conn_id, client_stream_id)`
3. 将 frame 的 `stream_id` 重写回 `client_stream_id`
4. 转发到对应客户端 WS

#### 4.4.2 execution 选择（load balancing）

候选 execution 池来源：

- 优先复用当前 task 的活跃 execution（运行中且 WS 可用）
- 不足时触发启动新的 execution 并加入池

选择策略建议从简单到复杂演进：

- Round-robin
- Random
- Least active streams（推荐）：选择当前 `active_streams` 最小的 execution
- Metrics-based：结合 instance 指标（CPU、并发、队列长度等）

#### 4.4.3 stream_id 重写（命名空间隔离）

必须重写的原因：

- execution 内的 user stream hub 以 `(execution_id, stream_id)` 作为路由键
- 多个客户端连接如果复用同一个 execution，会自然产生 `stream_id=1/2/...` 的冲突

因此 SMS 必须保证：

- 对同一 execution：所有上游 `upstream_stream_id` 全局唯一
- client 侧 stream_id 可以局部唯一（每个 client 连接从 1 开始均可）

分配方式：

- 每个 execution 维护一个单调递增的 `next_upstream_stream_id: u32`
- 分配时 `upstream_stream_id = next++`（跳过 0）
- 连接断开或收到 close/error 时回收映射并减少 `active_streams`

#### 4.4.4 失败与恢复语义

- 上游 execution WS 断开：
  - 将该 execution 标记为 unhealthy 并移出候选池
  - 对已绑定到该 execution 的 active streams：向客户端发送 error frame 或 close
  - 新的 streams 分配到其它 execution
- execution 启动失败：
  - 若候选池为空则返回握手失败（503/504）
  - 否则继续在现有 execution 上服务并记录告警

## 5. 数据面协议（WS/SSF）

### 5.1 总体

客户端通过 WS 与 SMS 交互，SMS 再通过 WS 与 Spearlet（execution 维度的 user stream hub）交互。两段 WS 的业务帧均为二进制 SSF v1（并建议通过 `Sec-WebSocket-Protocol: ssf.v1` 显式协商）：

- `stream_id`：客户端为每个请求分配一个 u32（单 execution 内唯一）
- `msg_type`：区分 request/response/error（可用固定数值约定）
- `metadata`：JSON（包含 method/path/headers/trace_id 等）
- `payload`：请求体 bytes（JSON/二进制均可）

任务侧行为约定：

- 监听 ctl 事件（stream connected）
- 对每个 stream 读取 inbound 帧 → 处理 → 写回 outbound 帧（同 stream_id）

建议补齐（便于跨语言 SDK 与线上排障）：

- `msg_type` 约定：
  - `0x01` request
  - `0x02` response
  - `0x03` error
  - `0x04` cancel（客户端取消该 stream；SMS 仅转发语义，不做自动重试）
- `metadata` 规范（建议字段）：
  - `request_id`：每个 stream 必须携带（SMS 如缺失则生成并回写/透传）
  - `traceparent`/`tracestate`：W3C Trace Context（如系统已采用）
  - `deadline_ms`：客户端期望的剩余时间预算（SMS 可用于做本地超时与上游选择）
  - `idempotency_key`：可选，用于允许 SMS 在“仅对幂等请求”做安全重试/换路由
- `error` 负载建议采用结构化 JSON（放在 `metadata` 或 `payload` 中二选一，固定一种）：
  - `code`（稳定枚举）、`message`（面向开发者）、`retryable`（是否建议重试）、`details`（可选）
- 流生命周期：
  - 当收到 response 或 error（终结帧）后，SMS 立即回收该 stream 的映射与计数
  - 对长时间无任何帧往来的 stream，SMS 可按 `stream_idle_timeout` 主动 error+回收，避免泄漏
- 帧大小与连接保护（建议默认值，可配置）：
  - `max_frame_bytes`（例如 1–4MiB）
  - `max_active_streams_per_conn`、`max_active_streams_per_execution`
  - 触发时优先返回可观测的错误（握手期用 413/429；建连后用 error frame/close）

### 5.2 SSF Builder/Parser 归属

建议将 SSF v1 的 build/parse 抽成可复用模块（共享 crate 或 SMS 侧复制轻量实现），避免 SMS 自己拼字节出错。

## 6. 端到端时序

1. Client → SMS：`GET /e/{endpoint}/ws`（WebSocket Upgrade）
2. SMS：校验 endpoint；通过索引找到 task_id
3. SMS：查找可用 execution；若不存在则启动一个 execution 并等待可用
4. SMS：创建 stream session token（绑定 execution_id）
5. SMS：作为 WS client 连接 `ws://sms/api/v1/executions/{execution_id}/streams/ws?token=...`（或直连 Spearlet）
6. SMS：启动双向代理：Client WS ↔ Upstream WS（二进制帧透传）
7. Client：发送 SSF request frame（stream_id=rid）
8. Task：通过 user stream 收到 request frame，处理并写回 response frame
9. Client：收到 response frame（同 stream_id），完成一次请求

## 7. 超时、重试与取消

- WS 握手 + 上游准备总超时：默认 30s（可配置）
- placement/instance 启动超时：默认 10s（可配置）
- 上游 WS connect 超时：默认 2s（可配置）
- WS 空闲超时：默认 10min（可配置，空闲则断开并回收 execution）
- WS keepalive：建议启用 ping/pong（例如 30s 间隔，N 次未响应则断开），用于及时发现半开连接并释放资源

重试策略（建议）：

- 若上游 WS 连接失败或 execution 不可用，可切换 execution（或触发重建）重试（最多 N 次）
- 对 request 级别的自动重试必须满足：显式标记为幂等（例如 `idempotency_key` 存在或 `metadata.method` 属于安全方法）且任务侧实现幂等语义；默认不做请求级重试
- 断连时的清理：关闭上游 WS；按策略决定是否 terminate execution（取决于是否“长驻/共享”）

取消语义（建议）：

- 客户端发送 `msg_type=cancel` 表示取消对应 stream；SMS 转发到上游并回收本地映射
- 上游可返回 `error{code=CANCELLED}` 或直接静默终止该 stream；二者需在 SDK 层统一抽象

## 8. 安全与鉴权

- endpoint 严格字符集与长度限制，避免 path traversal 与路由碰撞
- 必须校验 `Origin`（浏览器场景）与 TLS 终止策略，避免跨站 WS 滥用与中间人风险
- 可选的 API key / token 鉴权（在 SMS handler 校验），并预留按 endpoint 的 ACL/策略点（RBAC/ABAC）
- 增加连接级/用户级/endpoint 级限流与并发上限（推荐接入全局限流组件或基于令牌桶）
- WS token 使用现有 stream session 机制（TTL），避免外部直接连 Spearlet；建议 token 绑定 principal 与 execution_id，且仅允许单次使用或短时间重放窗口
- 审计日志：记录 endpoint、主体（user/app）、连接来源、结果（允许/拒绝）与原因码

## 9. 可观测性

建议指标与日志：

- endpoint 路由命中耗时（endpoint → task_id）
- instance 选择耗时、placement 耗时
- WS connect 耗时
- roundtrip latency（HTTP→WS→HTTP）
- error 分类：not_found / no_instance / ws_fail / timeout / task_error

trace_id：

- SMS 为每次请求生成 request_id，写入 SSF metadata，并在日志中全链路打印

建议补齐（避免高基数与便于告警）：

- 指标维度控制：endpoint 可能高基数，建议提供开关或做采样/分桶；告警优先基于 code/阶段（resolve/start/connect/proxy）
- 结构化日志字段：`endpoint`、`task_id`、`execution_id`、`client_conn_id`、`client_stream_id`、`upstream_stream_id`、`request_id`、`error_code`、`latency_ms`
- trace span 建议：连接握手、上游连接、每个 stream（request→response/error）分别建 span，并传播 `traceparent`

## 10. 实现落点（建议）

- SMS 路由：`src/sms/routes.rs` 增加 `.route("/e/{endpoint}/ws", get(endpoint_ws_proxy))`
- 新 handler：`src/sms/handlers/endpoint_gateway.rs`
- endpoint 索引：在 task store 中维护 `endpoint -> task_id`（注册/注销时更新，大小写不敏感）
- 上游 WS：SMS 作为 WS client 连接 Spearlet 并做双向转发（可选进一步复用 stream session token 机制做更强的访问控制）

## 11. Web Console 支持（设计）

目标：让 Web Console（浏览器）可以安全、可观测地连接到 endpoint，并以“请求/响应”的交互方式调试或调用任务侧能力。

### 11.1 UX/交互

- Endpoint 选择：从任务列表中展示 `endpoint`，支持搜索/过滤。
- 连接生命周期：Connect/Disconnect、连接状态、最后一次错误原因。
- 请求面板（推荐以 SSF request 抽象呈现）：
  - `metadata` 编辑（JSON，提供结构化表单视图与 raw 模式切换）
  - `payload` 编辑（text/base64/hex，按 content-type 提示）
  - 自动生成并展示 `request_id` / `traceparent`
  - 每个 request 分配本地 `client_stream_id` 并展示响应的 latency / error
- 历史与复现：保存最近 N 次请求的 metadata/payload（本地浏览器存储优先），支持一键重放。

### 11.2 浏览器侧协议与实现方式

浏览器 WebSocket 支持二进制帧，因此 Web Console 可以直接作为 endpoint gateway 的客户端：

- WebSocket：`wss://{sms}/e/{endpoint}/ws`
- 子协议：`Sec-WebSocket-Protocol: ssf.v1`
- 帧编码：使用 SSF v1（ArrayBuffer）进行 build/parse；每次请求使用一个 `stream_id`，以 response/error 终结该 stream。

建议在 Console 侧实现一个轻量 SSF JS 模块（只做 header parse/build + stream_id 递增），并严格限制：

- `max_frame_bytes`（前端与服务端一致）
- 单连接并发 streams 上限（避免 UI 误操作压垮 execution）

### 11.3 鉴权与安全（最佳实践）

浏览器 WS 不便携带自定义 header（除 subprotocol），因此推荐“先发 HTTPS 再连 WSS”的两段式：

1. Console 先调用 HTTPS 获取短期 token（避免直接暴露 endpoint WS 给跨站脚本或未授权用户）
2. Console 再用带 token 的 WSS URL 建连

建议新增（或复用现有模式实现同类能力）的控制面接口：

- `POST /api/v1/endpoints/{endpoint}/session`
  - 返回：`ws_url`（包含短期 token）、`expires_in_ms`
  - 语义：token 绑定到当前登录主体（user/app/session），并可绑定 `endpoint` 与可选的策略（例如只读/调试/生产禁用）

WS 建连时（握手阶段）建议强制校验：

- `Origin` 必须属于允许列表（同域或受信任域）
- TLS 必须启用（生产）
- token 校验与重放窗口（TTL/单次使用可配置）
- 连接级/用户级限流与并发上限

### 11.4 可观测性与审计

- Console 端：对每个 request 记录 `request_id`、开始/结束时间、结果码（本地展示为主）
- SMS 端：对连接与每个 stream 打结构化日志，并确保 `traceparent` 贯穿到任务侧
- 审计：记录谁在什么时间连接了哪个 endpoint、发起了哪些请求（仅记录 metadata 的关键字段，避免把 payload 作为审计日志默认落盘）

### 11.5 与现有架构的关系

- Console 不需要理解 execution/instance 的调度细节：仍由 SMS endpoint gateway 负责 per-stream 路由与 stream_id 重写。
- 若生产环境不允许 Console 直接连 `/e/`：可通过上述 session 控制面进行强约束，并支持按环境/租户/用户禁用或降级。

## 12. 风险与开放问题（Review 点）

1. endpoint 对应 execution 的生命周期策略：常驻 vs idle 回收，是否需要 min pool？
2. SSF 协议的 msg_type/metadata 规范需要定稿（尤其是 error frame 与 header 映射）
3. 是否要提供更上层的任务 SDK（例如 `Spear.endpoint.serve(handler)`）来规范任务侧行为？
4. endpoint 与现有 `/console`、`/admin` 路由的冲突策略（采用 `/e/` 前缀可规避）
