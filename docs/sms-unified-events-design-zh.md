# SMS 统一事件系统（Unified Events）设计方案（面向未来消息队列 / Message Broker）

## 背景与目标

现状：SMS 的 task 消费链路已经完成收敛，Spearlet 直接消费统一事件流中的 `sms.TaskEvent` payload。相关实现：

- Durable + broadcast：`UnifiedEventBus` [unified_events.rs](../src/sms/unified_events.rs)
- 订阅 RPC：`EventsService.SubscribeEvents(selector, after_seq)` [service.rs](../src/sms/service.rs)
- Spearlet 消费侧：`TaskEventSubscriber`（本地 cursor + 断线重连）[task_events.rs](../src/spearlet/task_events.rs)

问题：未来不仅是 task，artifact/instance/execution/backend registry 等对象的创建/更新/删除也需要事件。继续为每个对象手写一套 “outbox+订阅+游标” 会造成协议碎片化与重复实现，也不利于后续接入 Kafka/RabbitMQ/Pulsar/NATS 等消息中间件。

本设计目标：

- **统一**：抽象出通用事件模型与通用订阅接口，Task 只是统一事件流中的一种 payload。
- **可回放**：服务端 durable 存储，客户端基于游标/序号稳定恢复。
- **可扩展**：支持未来的消息中间件分发（Outbox/Relay/Publisher 模式），且不破坏现有消费语义。
- **优雅**：协议简洁、字段语义清晰、演进路径可控、对现网影响最小。

非目标（第一阶段不做）：

- 端到端 exactly-once（行业主流仍以 at-least-once + 幂等为主）。
- 复杂的服务器端表达式过滤（优先简单可用、必要时再演进）。

## 术语与一致性语义

- **事件（Event）**：对某个资源（resource）的状态变化的不可变事实记录。
- **资源（Resource）**：系统内的一等实体，例如 Task、Node、Artifact、Execution、Instance、BackendSnapshot 等。
- **流（Stream）**：事件的有序序列。行业常见对应：Kafka 的 topic/partition、RabbitMQ 的 exchange/queue（配合 routing key）、或 EventStoreDB 的 stream。
- **序号（seq）**：某个 stream 内单调递增的序列号，用作“断点续订”的游标。

一致性约束（第一阶段）：

- **同一 stream 内有序**：事件在同一 stream 内按 `seq` 严格递增（单写者或串行 append）。
- **跨 stream 不承诺全局顺序**：避免全局计数器成为瓶颈。
- **投递语义**：对订阅者为 at-least-once。客户端必须幂等处理（以 `event_id`/`(stream, seq)` 去重）。

## 统一事件模型（Envelope）

建议新增 `proto/sms/events.proto`，定义通用事件封装（envelope）：

- **必需字段**
  - `event_id`：全局唯一 ID，推荐 ULID（可排序 + 低协调成本），字符串形式。
  - `ts_ms`：毫秒时间戳（UTC epoch ms）。
  - `stream`：事件流名称（string），例如 `type.task`、`node.{node_uuid}`、`resource.task.{task_id}`。
  - `seq`：stream 内序号（uint64），用于客户端 resume。
  - `resource_type`：枚举或 string（推荐 enum + 保留 unknown），例如 TASK/EXECUTION/ARTIFACT/NODE。
  - `resource_id`：资源主键（string），例如 task_id、execution_id。
  - `op`：枚举（CREATE/UPDATE/DELETE/UPSERT/HEARTBEAT/STATE_TRANSITION）。
  - `schema_version`：payload schema 版本（uint32），用于演进。
- **可选字段（强烈建议预留）**
  - `node_uuid`：该事件关联的节点（用于路由/过滤）。
  - `correlation_id`：链路关联（request_id/execution_id 等）。
  - `cause`：触发原因（人工、自动调度、健康检查等）。
  - `headers`：map<string,string>，用于扩展与各类 broker 的消息属性/headers 映射。
  - `payload`：`google.protobuf.Any` 或 `bytes` + `content_type`。

payload 选型建议：

- 第一版建议用 `google.protobuf.Any`，因为你们已经使用 proto，并且 Any 对 schema 演进更稳。
- 同时保留 `bytes payload_bytes + string content_type` 的扩展位（便于将来 JSON/Avro/Protobuf 多协议）。

## 订阅协议（可回放 + 实时）

### 最小可用订阅接口

新增 RPC（独立于 TaskService）：

- `rpc SubscribeEvents(SubscribeEventsRequest) returns (stream EventEnvelope);`
- `SubscribeEventsRequest` 核心字段：
  - `selector`：订阅选择器（见下一节）
  - `resume_token`：断点（推荐：`map<string, uint64> stream_seq` 或 `oneof { StreamCursor ... }`）
  - `max_batch_replay`：replay 上限（防止一次拉爆）

服务器实现模式沿用现有最佳实践：

- 先 replay（从 KV/存储中扫描并按 `seq` 排序返回）。
- 再 live（broadcast 或 watch）。
- 若订阅 lagged：返回可恢复错误（例如 `aborted/resync required`），客户端回退到 replay。

### Selector 设计（优雅且可演进）

建议 selector 不做复杂表达式，保持结构化过滤：

- `by_stream_prefix`：订阅某些 stream 前缀（例如 `type.task` 或 `resource.task.`）。
- `by_node_uuid`：订阅特定 node 的所有事件（等价于多个 stream）。
- `by_resource_type` + 可选 `resource_id_prefix`：订阅特定资源类型事件。

这 3 类足够覆盖绝大多数场景，并且与主流 broker 的路由模型自然兼容（topic/partition、exchange/routing-key 等）。

## SMS 存储设计（Outbox 优先）

你们现有 TaskEventBus 已经是典型 outbox 思路（KV durable + broadcast）。统一化后建议变为：

- **EventStore（append-only）**
  - `append(stream, envelope) -> (seq)`
  - `scan(stream, after_seq, limit) -> Vec<EventEnvelope>`
  - `scan_prefix(prefix, ...)`（用于 by_stream_prefix）
- **EventBus（live fanout）**
  - 基于 broadcast 的 per-stream channel（或 per-node/per-prefix channel）

KV key 设计建议（与现有保持一致、利于迁移）：

- `events:{stream}:{seq} -> StoredEnvelope`
- `events_counter:{stream} -> last_seq`
- `events_index:resource:{resource_type}:{resource_id}:{stream}:{seq} -> 1`（可选，后续需要按 resource 查询时再加）

保留策略（Retention）：

- 第一阶段沿用 “每 stream 保留最近 N 条” 的简化策略（类似现有 `MAX_EVENTS_PER_NODE`）。
- 第二阶段可升级为 “按时间 TTL + compact（按 resource_id 保留最新）”，并在外部 broker 后端由其 retention / DLQ 策略接管。

## 消息中间件适配（留足空间的行业标准做法）

核心原则：**SMS 是权威写入点；外部消息中间件是分发与多消费者平台**。避免出现“写 broker 成功但写 SMS 失败”的一致性缺口。

推荐采用 **Transactional Outbox / Relay（发布器）**：

- SMS 在写入业务状态时同步写入 outbox（同一持久化后端，最好同事务）。
- 独立的 `EventRelay`（可内置进 SMS，也可单独进程）持续扫描 outbox，将新事件投递到外部 broker。
- Relay 维护自己的 checkpoint（例如 `relay_checkpoint:{stream} -> last_published_seq`），确保至少一次投递；消费者侧幂等。

### Broker 抽象（保持 MQ 无关）

为避免设计被 Kafka/RabbitMQ 绑定，建议定义内部抽象：

- `BrokerPublisher`
  - `publish(topic_or_exchange, routing_key, headers, payload) -> Result<()>`
- `BrokerConfig`
  - type: `kafka | rabbitmq | pulsar | nats | ...`
  - connection: url/credentials（由部署层注入）
- `RelayCheckpointStore`
  - `get(stream) -> seq`
  - `set(stream, seq)`

Relay 层只依赖 `BrokerPublisher`，从而支持多种 MQ 实现。

### 统一映射建议（Kafka / RabbitMQ）

为了“同一事件协议可以被不同 MQ 传输”，建议固定以下映射规则：

- **消息 key / routing key**
  - 默认：`resource_id`（保证同一资源内近似有序）
  - 若业务更关心同 node 内顺序：使用 `node_uuid`
  - 具体取舍由 `stream` 命名策略决定（见下文）
- **headers / properties**
  - `event_id`, `stream`, `seq`, `resource_type`, `resource_id`, `op`, `schema_version`, `ts_ms`
  - 在 RabbitMQ 中对应 message headers；在 Kafka 中对应 record headers；在 Pulsar/NATS 中对应 properties/headers。
- **payload**
  - protobuf bytes（EventEnvelope）
  - 若未来要多协议：使用 `content_type`（如 `application/protobuf`、`application/json`）

Kafka：

- topic：`sms.events.v1`（统一大 topic）或按域拆分（`sms.task.v1`、`sms.execution.v1`）
- partition key：使用上面的“消息 key”策略；consumer group 用于横向扩展
- offset 仅用于 Kafka 内部消费进度；**系统级 resume 仍用 `(stream, seq)`**

RabbitMQ：

- exchange：推荐 `sms.events.v1`（topic exchange）
- routing key：推荐 `{resource_type}.{node_uuid}.{resource_id}` 的可组合前缀（例如 `task.<node>.<task_id>`），便于按前缀/通配符绑定队列
- queue：由消费者自行创建绑定（competing consumers 模式）
- ack/retry/DLQ：由队列/死信交换机策略承载；消费者仍需幂等

## 兼容与迁移策略（从 SMS 侧先做）

### Phase 0：引入通用事件内核（只在 SMS 内部）

- 抽象 `UnifiedEventStore + UnifiedEventBus`，实现 KV outbox + broadcast。
- 保持现有 `TaskEventBus` 外观不变，但内部调用 unified 内核，做到：
  - 旧 key 兼容（或提供数据迁移脚本）
  - 旧 RPC `SubscribeTaskEvents` 行为不变

### Phase 1：发布新 gRPC（Unified Events Service）

- 新增 `EventsService.SubscribeEvents`
- Task 相关事件同时通过新服务可见（例如 stream 命名为 `type.task` 与 `resource.task.{task_id}`）
- 旧 RPC 保留一段时间（兼容期），并在文档中标注 deprecate

### Phase 2：扩展到其他资源

- 逐步把对象变更（artifact/execution/instance/registry）统一写入事件总线。
- Spearlet 侧按需订阅：
  - placement/调度仍按 node_uuid 订阅
  - 观测/审计按 resource_type 或 stream_prefix 订阅

### Phase 3：引入消息中间件 Relay（可选）

- 引入 `EventRelay`，将 outbox 投递到外部 broker（Kafka/RabbitMQ/Pulsar/NATS 等）
- SMS 内部订阅继续可用；外部系统优先用 broker
- 当 broker 成熟后，SMS 的 replay retention 可以缩短或只保留必要窗口

## 客户端（Spearlet）消费 best practice

- 游标存储：仍建议本地 durable（文件/kv），与现有 `task_events_cursor_{node}.json` 一致。
- 幂等处理：按 `(stream, seq)` 或 `event_id` 去重（尤其在 at-least-once 下）。
- 断线重连：
  - 优先用 replay_since（服务端可回放）
  - live lagged 时回退 replay

## 风险与权衡

- KV `scan_prefix` 在事件量很大时成本高：第一阶段可接受；第二阶段需引入更高效的索引或迁移到 RocksDB/LSM 更合适的数据布局；外部 broker 接入后也可减少 SMS replay 窗口。
- broadcast channel 可能 lag：必须把 “lagged -> resync” 做成显式语义（你们现在已经这么做了）。
- schema 演进：必须落实 `schema_version`，并对 payload 做向后兼容约束。

## 内存与背压（资源受限场景的设计要点）

统一事件系统的关键风险之一是“把持久化回放 + 实时 fanout”做成一个隐式的内存队列，从而在高 QPS 或慢消费者下导致内存膨胀。第一阶段即需要明确内存边界与背压策略。

### 设计原则

- **默认零积压**：SMS 端不为每个订阅者缓存无限数据；持久化存储负责回放，内存只做短暂实时扇出。
- **有限缓冲**：所有内存缓冲必须有上限（条数/字节），并且达到上限时采取显式策略（drop/lagged/resync）。
- **慢消费者隔离**：单个慢消费者不能影响其他消费者，也不能影响事件写入路径。
- **显式退化**：当出现 lagged/内存压力时，返回“可恢复错误”，客户端回退到 replay。

### Live 扇出策略（broadcast 的边界）

现有实现是每 node 一个 `broadcast::channel(1024)`。统一后建议：

- **每 stream 独立 channel**（或按 node_uuid 聚合），buffer size 可配置（默认 1024），并支持按部署调小以节省内存。
- **不为无人订阅的 stream 建 channel**：只有 `receiver_count()>0` 时才发送（现有已做），避免无谓复制。
- **lagged 行为固定**：当 receiver lagged 时，服务端将其视为“需要 resync”，返回 `aborted("watch lagged; resync required")`（与现有 MCP watch 一致）。

说明：broadcast 在 Rust/Tokio 中对每个 receiver 维护序号，并不为每个 receiver 拷贝 buffer，但 buffer 里的每条消息会被保留直到所有 receiver 越过；因此 **buffer 太大 + 有慢消费者** 会显著增加驻留时间（间接增大内存占用）。所以 buffer size 必须可控。

### Replay 策略（避免一次性拉爆内存）

- replay 必须支持 `limit`，且服务端强制上限（例如 `max_batch_replay <= 1000`），避免单次 scan 把大量事件堆到内存。
- 订阅接口建议支持“分段 replay”：
  - 第一次订阅返回 replay + live 的组合流（你们现在这样做）
  - 但 replay 的每次返回条数固定上限，客户端在收到 `aborted/resync required` 时重新订阅并携带新的 `last_seq`
- 如需更强：增加 `ListEvents(stream, after_seq, limit)` 的非 streaming RPC，让客户端显式分页拉取（更可控、实现也简单）。

### Outbox Retention（限制持久化存储与 scan 成本）

- 第一阶段沿用“每 stream 保留最近 N 条”（现有 `MAX_EVENTS_PER_NODE`），并把 N 做成配置项，保证可按资源调整。
- 对于高频资源（如 metrics/heartbeat），应使用：
  - 独立 stream（避免污染关键业务 stream）
  - 更短 retention（例如仅保留最近 1k 或最近 5 分钟）
  - 或使用 compact 策略（按 resource_id 只保留最新一条状态事件）

### 事件体大小限制（防止 payload 放大）

- 服务端应限制单条事件 payload 最大字节数（例如 64KB/256KB），超限拒绝或裁剪。
- payload 应避免直接携带大对象；大对象使用 URI 引用（指向对象存储/结果存储），事件只携带索引与元数据。

### Relay（投递到外部 broker）内存策略

- Relay 不应“批量读取全量 outbox 到内存后再发送”，而是采用流式扫描：
  - 每次读取固定 batch（条数/字节上限）
  - 逐条（或小批）publish，成功后推进 checkpoint
- 并发 publish 需受限（例如 `max_inflight`），防止积压导致内存增长与 broker 连接压垮。

### 配置建议（第一阶段可落地）

- `events.live_buffer_size`（默认 1024，可按内存压到 64/128）
- `events.replay_max_batch`（默认 1000）
- `events.retention_per_stream`（默认 10_000，按 stream 类型可覆盖）
- `events.max_payload_bytes`（默认 256KB 或更小）
- `relay.max_inflight`（默认 64）

## 建议的下一步落地清单

- 在 SMS 内部落地 unified 内核：`sms/events_unified.rs`
- 增加 `proto/sms/events.proto` + `EventsService`（SubscribeEvents）
- 将现有 TaskEventBus 改成 adapter（内部调用 unified）
- 增加文档与示例：如何用 selector 订阅 node/task/execution
- 预留 Relay 接口与 checkpoint schema（不一定立刻实现 Kafka/RabbitMQ）
