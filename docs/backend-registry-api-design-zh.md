# Backend Registry API 与 Web Admin Tab 方案设计

## 背景

Spearlet 会基于 `SpearletConfig.ai.backends` 与 `ai.credentials` 构建运行时 backend registry，构建逻辑见 [builder.rs](../src/spearlet/execution/ai/router/builder.rs)。目前系统缺少一个面向用户的 API 来列出“当前已注册且可用”的 backends，也缺少 Web Admin 页面展示“available backends”的入口。

实现状态说明（以当前仓库为准）：

- Spearlet 已经会周期性向 SMS 上报 backend 快照（包含本地模型的 managed backends）。
- hosting 完全以配置为准（必填：`local|remote`），不再做推断。

本文给出一套业界 best practice、且在当前架构下较为优雅的方案，包含：

- 后端 API（节点维度与集群聚合维度）的设计
- Web Admin 新增 Tab 的展示与交互

## 目标

- 提供 node-scoped API：列出某个 Spearlet 节点当前可用的 backends（通过 env 校验后真正可用）。
- 提供 cluster-scoped API：聚合所有节点的 backend 可用性分布。
- 严格不泄露 secrets：不返回 credential 值，只返回“存在/可用”的信号。
- 支持过滤、分页，以及可选的增量更新（SSE）。

## 非目标

- 在 Web Admin 中直接编辑/下发 backend 配置。
- 暴露 credential 明文。

## 数据模型

### BackendInfo（node-scoped）

- `name`: string
- `kind`: string（例如 `openai_chat_completion`、`openai_realtime_ws`）
- `operations`: string[]（例如 `chat_completions`）
- `features`: string[]（可选）
- `transports`: string[]（例如 `http`、`ws`）
- `weight`: number
- `priority`: number
- `base_url`: string（如适用）
- `status`: enum `available | unavailable`
- `status_reason`: string（例如 `missing env OPENAI_CHAT_API_KEY`）
- `instance_id`: string（可选，如果能映射到运行时实例）

### AggregatedBackendInfo（cluster-scoped）

- `name`: string
- `kind`: string
- `operations`: string[]
- `features`: string[]
- `transports`: string[]
- `available_nodes`: number
- `total_nodes`: number
- `nodes`: [{ `node_uuid`: string, `status`: `available|unavailable`, `status_reason`: string }]

## Spearlet API（节点维度）

建议在 Spearlet 增加一个新的服务：

- gRPC：`BackendService.ListBackends()` → `BackendInfo[]`
- HTTP gateway：`GET /backends` → JSON `BackendInfo[]`

实现建议：

- registry 构建逻辑位于 [builder.rs](../src/spearlet/execution/ai/router/builder.rs)。在构建 registry 的同时收集两类信息：
  - `BackendRegistry.instances()` 中已注册（可用）的实例
  - 对于配置中因 env 校验失败而被跳过的 backend：也纳入列表但标记 `status=unavailable` 并提供 `status_reason`

过滤与分页：

- Query 参数：`kind`、`operation`、`transport`、`status`、`limit`、`offset`
- 节点维度默认 `limit=200`

## 节点主动上报（方案 A，推荐）

为了避免 Web Admin 每次刷新都做 N×fanout（逐节点拉取），推荐由 Spearlet **主动上报**当前 backend 快照到 SMS，由 SMS 负责存储与聚合查询。

关键点：

- 不复用 `HeartbeatRequest.health_info`（当前 SMS 侧不会持久化该字段，而且 `map<string,string>` 不利于结构化演进）。
- 新增专用 RPC，使用结构化 message，便于版本演进、限流与大小控制。

### RPC / proto 草案（示意）

在 `proto/sms` 增加一个新 service（或扩展现有 NodeService）：

- `ReportNodeBackends(ReportNodeBackendsRequest) -> ReportNodeBackendsResponse`

请求体建议字段：

- `node_uuid`: string
- `reported_at_ms`: int64
- `revision`: uint64（节点侧单调递增，用于幂等/去重）
- `backends[]`: BackendInfo（结构化列表）

返回：

- `success`: bool
- `message`: string
- `accepted_revision`: uint64

安全要求：

- **严禁**返回或上报 credential value。
- `status_reason` 允许包含“缺少的 env 名称”等非敏感信息，不允许包含 env 值。

### SMS 侧存储

SMS 存储“每节点最新一次 backend 快照”，并为 Web Admin 提供聚合读取：

- `node_uuid -> { revision, reported_at_ms, backends[] }`

可以先用内存/现有 KV 抽象落地，后续切换 Sled/RocksDB 持久化。

### Spearlet 上报时机（best practice）

- 启动完成后立即上报一次。
- 周期性全量 resync（例如 60s 或 300s）作为兜底。
- 配置变化/可用性变化时触发（例如凭据 env 变更、热加载）。

## SMS Web Admin BFF（集群聚合维度）

新增接口：

- `GET /admin/api/backends` → `AggregatedBackendInfo[]`
  - 参数：`kind`、`operation`、`status`、`limit`、`offset`
  - 实现：直接读取 SMS 存储的“节点上报快照”，完成聚合。
  - 缓存：可选短 TTL cache（例如 5–15 秒）用于减少重复聚合计算。

- `GET /admin/api/nodes/{uuid}/backends` → node-scoped `BackendInfo[]`（透传/代理）

建议同时保留两种读取路径：

- `GET /admin/api/nodes/{uuid}/backends`：默认读 SMS 中该节点的快照（快速、稳定）。
- `GET /admin/api/nodes/{uuid}/backends?source=node`：可选透传/代理到节点实时接口（用于排障与一致性校验）。

- `GET /admin/api/backends/stream[?once=true]` → SSE snapshot，用于前端增量刷新（可选）。

增量更新建议基于“上报事件”驱动：当 SMS 接收到 `ReportNodeBackends` 后，向 SSE stream 推送该节点快照变更。

安全：

- 永不返回 credential 值；可选仅返回“需要的 env 名称”（不返回 value）。
- Web Admin 接口复用 `SMS_WEB_ADMIN_TOKEN` Bearer 鉴权。

## Web Admin UI

新增一个 Tab：`Backends`。

### 列表页

- 数据源：`GET /admin/api/backends`
- 展示列：name、kind、operations、transports、available_nodes/total_nodes、status
- 过滤：kind、operation、status
- 操作：查看节点分布（抽屉/详情面板）、刷新

### 详情抽屉

- 展示每节点可用性：node name/uuid、status、status_reason
- 可选展示 base_url 与 capabilities 摘要

### 节点详情补充

- 在现有 Nodes 页面节点详情中增加 “Backends” 区块，调用 `GET /admin/api/nodes/{uuid}/backends`

### 交互 best practice

- 长列表分页 + 虚拟滚动。
- status 用 badge，operations/transports 用 chip。
- filters 写入 query 参数，链接可分享。

## 可观测性

- 指标：聚合耗时、fanout 数量、cache 命中率。
- tracing：每个节点一次 fetch 一个 span，并记录错误分类（unavailable/timeout）。

## 分期落地

Phase 0（MVP）：

- Spearlet 本地生成 BackendInfo 列表（包含 available/unavailable + reason）。
- Spearlet 实现 `ReportNodeBackends` 主动上报。
- SMS 接收并存储节点快照。
- Web Admin 新 tab 先展示 cluster-scoped 聚合（读 SMS 快照）。

Phase 1：

- 增加 `GET /admin/api/nodes/{uuid}/backends?source=node` 透传能力（排障/抽查）。
- 增加一致性抽查任务（低频 pull 校验 push 数据正确性）。

Phase 2：

- SSE stream 推送 backend topology 变化（由上报事件触发）。
- 更丰富的过滤与导出。

## 兼容性与安全兜底

- 节点不可达：该节点 backends 标记为 `unavailable`，`status_reason=unreachable`，聚合仍然返回。
- 严格避免 secrets 泄露：仅返回“可用/不可用”与原因，不返回环境变量值。
