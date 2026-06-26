# SMS Instance / Execution / Log 索引细节设计（面向 UI 与未来 MQ）

## 背景

UI 需求：

- 在 UI 选择某个 task 后，展示该 task 当前有哪些运行中的 instance（以及它们所在 node）。
- 用户进一步选择 instance 后，能够查看该 instance 相关的“现有 log”（用于排障与追踪）。

仓库现状（已存在的相关能力）：

- SMS 统一事件（Unified Events）内核与订阅接口已具备，采用 durable replay + live fanout（KV outbox + broadcast）的最小可用模式：
  - 设计：[sms-unified-events-design-zh.md](./sms-unified-events-design-zh.md)
  - 实现：[unified_events.rs](../src/sms/unified_events.rs)、[service.rs](../src/sms/service.rs)
- Spearlet invocation/execution 的协议已明确，且响应包含 `execution_id` 与 `instance_id`，并可流式输出：
  - [function.proto](../proto/spearlet/function.proto)
- 日志系统已有“按 execution 持久化/分页/tail/下载”的设计文档：
  - [invocation-log-storage-design-zh.md](./invocation-log-storage-design-zh.md)

## 关键约束（必须符合的业界规范）

若目标是符合业界 best practice：**日志持久化与检索的主键应为 execution_id，而不是 instance_id**。

- instance 是执行载体（容器/进程/沙箱/wasm instance），可能承载多个 execution。
- 常见模型：invocation（请求关联）→ execution（一次具体尝试）→ log（附着在 execution）。

因此推荐 UI 路径：

- **Task → Instances（聚合/筛选） → Executions（日志入口） → Logs（按 execution）**

如果 UI 必须从 instance “一键进入日志”，推荐默认跳转到“该 instance 最近一次 RUNNING/最新的 execution”的日志。

## 目标与非目标

### 目标

- 在 SMS 内提供稳定的 “task → active instances” 物化视图，并支持分页查询。
- 提供 “instance → recent executions” 视图，使 UI 可以从 instance 进入 execution。
- 为日志系统提供可靠的元数据索引（execution → log_ref），并与现有日志设计一致。
- 全链路 MQ 无关：继续使用 `EventEnvelope` + 多 stream 策略，未来可映射到 Kafka/RabbitMQ/Pulsar/NATS。
- 内存可控：不在 SMS 内存中维护全量集合；采用 KV 物化视图 + checkpoint，可重放恢复。

### 非目标（第一阶段不做）

- exactly-once（采用 at-least-once + 幂等）
- 任意表达式过滤订阅（仍以结构化 selector + stream taxonomy 为主）
- 无界历史存储（必须有 retention/上限/TTL）

## 术语

- **Task**：SMS 注册的任务（控制面实体）。
- **Instance**：在 node 上承载 task 的运行实例（容器/进程/沙箱等）。
- **Invocation**：一次用户请求（客户端侧关联 id），可能触发一次或多次执行（重试/迁移）。
- **Execution**：一次具体执行尝试（落在某个 node/instance），具备稳定的 `execution_id`，并承载日志流。

## 总体架构（推荐标准做法）

采用业界常见的 **Report + Outbox + Projector（物化视图）**：

1. **Spearlet 上报（Report）**：Spearlet 在 instance / execution 生命周期变化时，上报到 SMS（幂等 upsert）。
2. **SMS 持久化权威状态（KV）**：SMS 存储 instance/execution 记录（中心元数据）。
3. **SMS 发统一事件（Outbox + Streams）**：每次写入产生统一事件，写入 `all/type/resource/node` 等多条 stream。
4. **SMS 内部 Projector 维护索引视图（Materialized Views）**：订阅 `type.instance` / `type.execution`，维护 task→instances、instance→executions 等索引，以便 UI 高效查询。

不推荐的做法（反模式）：

- SMS 通过网络去“订阅所有 node 的 ExecutionService / 拉取 instance 列表”来维护中心视图。这会引入 O(nodes) 的连接/流管理、背压和不可控失败模式，不利于未来接入 broker 的扩展。

## 数据模型（SMS 内部 KV 存储）

你们当前 KV 序列化使用 JSON（`storage::kv::serialization`），本方案延续相同风格。

### InstanceRecord（中心状态）

Key：

- `instance:{instance_id}` → InstanceRecord

建议字段：

- `instance_id: string`（全局唯一；Spearlet 生成 ULID/UUID 均可）
- `task_id: string`
- `node_uuid: string`
- `status: string/enum`（RUNNING/IDLE/TERMINATING/TERMINATED/UNKNOWN）
- `created_at_ms: int64`
- `updated_at_ms: int64`（用于幂等与乱序合并）
- `last_seen_ms: int64`（用于 stale/TTL 判定）
- `current_execution_id: string`（可选）
- `metadata: map<string,string>`（runtime_type、image、sandbox、labels 等）

### ExecutionRecord（中心状态 + 日志索引入口）

Key：

- `execution:{execution_id}` → ExecutionRecord

建议字段（与 Spearlet [function.proto](../proto/spearlet/function.proto) 对齐）：

- `execution_id: string`（主键）
- `invocation_id: string`（用于 UI 关联与重试聚合）
- `task_id: string`
- `function_name: string`
- `node_uuid: string`
- `instance_id: string`
- `status: string/enum`（PENDING/RUNNING/COMPLETED/FAILED/CANCELLED/TIMEOUT）
- `started_at_ms: int64`
- `completed_at_ms: int64`
- `log_ref: LogRef`（见下）
- `metadata: map<string,string>`（execution_time_ms、error_message、exit_code 等）

### LogRef（指向日志存储）

日志落地建议严格按 [invocation-log-storage-design-zh.md](./invocation-log-storage-design-zh.md) 的 “按 execution 存储”。

LogRef 建议字段：

- `backend: string`（sms_file / object_store / broker 等）
- `uri_prefix: string`（例如 `smslog://executions/{execution_id}/`）
- `content_type: string`（text/plain / application/x-ndjson 等）
- `compression: string`（gzip 等）

## 物化视图（索引）设计

UI 需要高效的 “task → active instances” 与 “instance → executions”。

### TaskActiveInstancesIndex

Key：

- `idx:task_active_instances:{task_id}` → `Vec<InstanceSummary>`（bounded）

InstanceSummary 建议字段：

- `instance_id`
- `node_uuid`
- `status`
- `last_seen_ms`
- `current_execution_id`（可选）

约束（必须）：

- 每个 task 的 active instance 最多保留 N 条（建议 N=256 或 1024）。
- stale 清理：当 `last_seen_ms` 超过阈值（例如 2× heartbeat_timeout 或固定 2–5 分钟），从 active 集合移除或标记 stale。

### InstanceRecentExecutionsIndex

Key：

- `idx:instance_recent_executions:{instance_id}` → `Vec<ExecutionSummary>`（bounded）

ExecutionSummary 建议字段：

- `execution_id`
- `task_id`
- `status`
- `started_at_ms`
- `completed_at_ms`
- `function_name`

约束（必须）：

- 每个 instance 最多保留 M 条（建议 100 或 1000）。

### 可选：TaskRecentExecutionsIndex

Key：

- `idx:task_recent_executions:{task_id}` → `Vec<ExecutionSummary>`

用途：

- UI 不经 instance 直接展示 “该 task 最近 executions”，并可直接点 execution 看日志。

## 事件模型与 stream taxonomy（与 Unified Events 对齐）

SMS 统一事件已具备 `ResourceType::INSTANCE` 与 `ResourceType::EXECUTION`（见 [events.proto](../proto/sms/events.proto)）。

建议采用你们当前已实现的多流写入策略（避免复杂服务器端过滤）：

### Instance 事件（resource_type = INSTANCE）

写入 streams：

- `all`
- `type.instance`
- `resource.instance.{instance_id}`
- `resource.task.{task_id}`
- `node.{node_uuid}`（建议保留）

payload：

- `google.protobuf.Any(type_url="type.googleapis.com/sms.Instance", value=Instance.encode())`

op：

- CREATE / UPDATE / DELETE（或 UPSERT）

### Execution 事件（resource_type = EXECUTION）

写入 streams：

- `all`
- `type.execution`
- `resource.execution.{execution_id}`
- `resource.task.{task_id}`
- `resource.instance.{instance_id}`
- `node.{node_uuid}`（建议保留）

payload：

- `sms.Execution` protobuf

说明：

- 同一事件写入多个 stream 时，不承诺跨 stream 全局顺序；每个 stream 自己维护 `seq`。
- Projector 只订阅 `type.instance` / `type.execution` 即可构建索引。

## Projector（在 SMS 内构建索引的方式）

业界命名与模式：Projector / Materialized View / Read Model（CQRS）。

### 订阅来源

Projector 订阅：

- `type.instance`
- `type.execution`

订阅方式复用 `SubscribeEvents` 的 replay+live 语义：

- selector.resource_type = INSTANCE
- selector.resource_type = EXECUTION

### Checkpoint

每个 projector 需要在 KV 存 checkpoint，确保重启可恢复：

- `projection_checkpoint:type.instance` → `last_seq`
- `projection_checkpoint:type.execution` → `last_seq`

### 处理逻辑（示意）

- Instance CREATE/UPDATE：
  - upsert `instance:{instance_id}`
  - 更新 `idx:task_active_instances:{task_id}`（依据 status 与 last_seen_ms）
- Instance DELETE：
  - 删除或 tombstone
  - 从 active index 移除
- Execution CREATE/UPDATE：
  - upsert `execution:{execution_id}`
  - 更新 `idx:instance_recent_executions:{instance_id}`
  - 可选更新 `idx:task_recent_executions:{task_id}`

### 幂等与乱序

- 事件投递语义 at-least-once：Projector 必须幂等（以 `(stream, seq)` 或 `event_id` 去重）。
- 若存在乱序：以 `updated_at_ms` 或单调 `version` 做 last-write-wins。

## UI 查询接口（建议形态）

### 按 task 查询 instances

- `ListTaskInstances(task_id, status=ACTIVE, limit, page_token)`

返回：

- `instances: [InstanceSummary]`
- `next_page_token`

### 按 instance 查询 executions

- `ListInstanceExecutions(instance_id, status_filter?, limit, page_token)`

返回：

- `executions: [ExecutionSummary]`
- `next_page_token`

### 按 execution 获取日志

建议严格按 execution_id：

- `GetExecutionLog(execution_id, cursor, limit)`（历史分页）
- `TailExecutionLog(execution_id, cursor)`（实时 tail）
- `DownloadExecutionLog(execution_id)`（整份下载）

实现与落地建议参见：[invocation-log-storage-design-zh.md](./invocation-log-storage-design-zh.md)

## Spearlet ↔ SMS 上报协议（建议新增 SMS gRPC 服务）

为了让 SMS 成为中心元数据权威写入点，建议新增 SMS gRPC 服务（proto 草案示意，便于后续实现）：

### InstanceRegistryService

- `ReportInstance(Instance) -> ReportInstanceResponse`（upsert）
- `HeartbeatInstance(InstanceHeartbeat) -> ...`（可选：若 instance 生命周期独立于 execution 更新）
- `DeleteInstance(instance_id) -> ...`（可选）

### ExecutionRegistryService

- `ReportExecution(Execution) -> ReportExecutionResponse`（upsert）
- `ReportExecutionStatus(ExecutionStatusUpdate) -> ...`（轻量更新）

幂等建议（必须）：

- 每条上报携带 `updated_at_ms` 或单调 `version`，SMS 侧做 last-write-wins。
- SMS 返回 accepted + current_version，Spearlet 可据此重试与回退。

## 可靠性、背压与成本控制（必须落地的工程约束）

- 上报与事件发布采用 best-effort：允许 SMS 短暂不可用；Spearlet 侧需 bounded queue，避免 OOM。
- 索引条目必须 bounded（N/M 上限），并有 TTL/stale 机制，避免 KV 无限增长。
- 日志必须分片/压缩/上限控制，避免单 execution 产生日志过大（参见现有日志文档）。

## 失败模式与恢复策略

- SMS 重启：Projector 从 `projection_checkpoint:*` 读游标，先 replay 再 live，恢复索引。
- Spearlet 掉线：instance `last_seen_ms` 不再推进，SMS 按超时标记 stale 并从 active 列表剔除。
- 重复事件/重复上报：以 `(stream, seq)` 或 `event_id` 去重；记录按 `version/updated_at_ms` last-write-wins。

## 演进计划（建议分阶段落地）

### Phase 1：最小可用（Execution 为主）

- SMS 增加 `ExecutionRecord` KV 存储 + “按 task 列 running executions”的查询。
- Spearlet 在 Invoke / 状态变更时上报 execution（含 instance_id）。
- UI 先用 “running executions 聚合得到 instance 列表” 快速满足展示需求。

### Phase 2：引入 InstanceRecord 与 TaskActiveInstancesIndex

- 增加 InstanceRegistry 上报。
- 增加 Projector 维护 `idx:task_active_instances:{task_id}`。
- UI 改用 `ListTaskInstances`，性能更稳、语义更清晰。

### Phase 3：日志闭环

- 落地 execution 日志存储与 tail/pagination/download（按现有日志设计）。
- UI 从 execution 进入日志（instance 仅用于筛选与定位 node）。

### Phase 4：接入外部 MQ（可选）

- 引入 Relay，将 `type.execution` / `type.instance` 等流投递到外部 broker。
- SMS 内部 replay retention 可缩短；外部系统订阅 broker 获取扩展能力。

## 与现有 Unified Events 的一致性说明

- 本方案严格沿用你们现有的 `EventEnvelope` 与 “多 stream 写入” 的 best practice（避免复杂服务器端过滤）。
- `all/type/resource/node` 的 stream taxonomy 与当前实现一致，且能自然映射到 Kafka topic / RabbitMQ routing key。
