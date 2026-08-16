# AI Backend Unified Control Plane Design

## 背景

当前 SMS/Spearlet 对 AI backend 的控制面由两条彼此割裂的路径组成：

- `remote backend`
  - 由 SMS 持久化保存
  - 通过 Spearlet 的 `BackendAssignmentController` 同步到节点
  - 节点再通过 `backend_reporter` 回报运行时快照
- `local deployment`
  - 由 `model_deployment_registry` 管理
  - 由 Spearlet 统一 assignment controller 驱动本地模型服务启动与停止
  - 同样通过 `backend_reporter` 回报运行时快照

在 UI 上，`AI Models` 页面又对节点上报快照按 `(provider, model, hosting)` 做聚合，并把聚合结果同时当成：

- 观测视图
- 控制入口

这会带来一组结构性问题：

- 控制面主实体没有稳定身份，`provider/model/hosting` 被误用为近似主键
- `remote` 与 `local` 资源模型割裂，扩展新 backend driver 时需要重复建设
- “模型聚合视图”和“控制面资源”混在一起，导致删除、启停、分页、计数都容易失真
- 节点 enable/disable 没有显式 placement 语义，只能依赖删除、同步或 credential 间接实现
- observed state 与 desired state 语义混乱

本设计文档提出一套**不兼容旧模型**的新架构，把 `remote/local` 统一到一套带 UUID 的 AI backend 控制面中。

## 目标

### 核心目标

- 使用 `backend_id(uuid)` 作为 AI backend 的唯一身份
- 用一套统一的控制面资源同时表达 `remote` 与 `local`
- 把“backend 在哪些节点启用”从隐式逻辑提升为显式 `placement`
- 把节点上报状态从匿名快照改为带 `backend_id` 的 observed state
- 把 `AI Models` 页面降级为只读聚合视图，不再承担 destructive/control 语义
- 为后续 driver 扩展、调度、状态收敛、审计与持久化打下统一基础

### 非目标

- 不考虑旧 `remote backend` / `model deployment` 数据迁移
- 不要求兼容现有 Web Admin API
- 不要求兼容当前 `AI Models` 页面的删除/编辑逻辑
- 不在本设计中解决所有本地模型 provider 的实现细节

## 设计原则

### 1. 稳定身份优先

- `backend_id` 是控制面主键
- `provider/model/hosting` 是属性，不是 identity
- `spec.name` 不再承担主键角色，只作为 runtime 可见名字的一部分

### 2. 控制面与观测面分离

- SMS 保存 `desired state`
- Spearlet 报告 `observed state`
- `AI Models` 是 read model，不是 source of truth

### 3. Remote / Local 同模

- `remote` 与 `local` 共享同一控制面资源模型
- 差异只体现在节点侧 materialization/driver 实现

### 4. Placement 显式化

- backend 是否在某节点启用，不靠同步、删除或推断
- 必须由 SMS 的 placement 资源明确表达

### 5. 节点控制器幂等

- Spearlet 只处理“分配给本节点的 backend”
- reconcile 必须按 `backend_id` 幂等收敛

### 6. 强类型优先

- 新控制面、状态和 read model 全部采用 typed proto/domain model
- 不再依赖松散 JSON 聚合对象作为主流程输入

## 当前实现问题总结

### 1. 控制面割裂

- `remote backend` 使用 `AdminBackendsState`
- `local deployment` 使用 `ModelDeploymentRegistryState`
- 两者生命周期、持久化、字段语义、状态流都不同

### 2. 聚合视图承担了控制语义

当前 `AI Models` 列表按 `(provider, model, hosting)` 聚合，适合作为观测视图，但不适合作为控制对象。它会天然导致：

- 一行对应多个 backend definition
- 删除一行可能删多个 backend
- 分页与总数语义错误
- 节点数统计按 instance 重复累计

### 3. 没有显式 enable/disable placement 语义

当前系统里：

- `remote backend` 的启停主要靠 create/delete
- `local deployment` 的启停主要靠 create/delete
- `credential.disabled` 间接影响运行时可用性

这种设计把：

- 资源配置是否存在
- 是否想启用
- 密钥是否可用
- 节点当前是否 ready

混成了不同来源的状态，难以解释也难以治理。

### 4. 节点上报状态没有绑定控制面 identity

当前 node report 的主要对象是 `backend_name/spec/status` 快照，缺乏稳定的 `backend_id` 关联，导致 SMS 很难严格区分：

- 这是哪个控制面 backend 的实现结果
- 当前状态是否对应最新 generation

## 新架构总览

新架构分成 4 层：

1. **AiBackend**
   - 控制面的主资源
   - 带 UUID
   - 描述“想要什么 backend”
2. **AiBackendPlacement**
   - 控制“backend 在哪些节点启用”
3. **AiBackendNodeStatus**
   - 节点回报“backend 在本节点上的实际状态”
4. **AiModelView**
   - 按 `provider/model/hosting` 聚合的只读运营视图

其中：

- `AiBackend` + `Placement` = desired state
- `AiBackendNodeStatus` = observed state
- `AiModelView` = read model

## 统一资源模型

### AiBackendRecord

AI backend 的控制面主资源。

建议字段：

- `backend_id: string`
- `display_name: string`
- `provider: string`
- `model: string`
- `hosting: enum { REMOTE, LOCAL }`
- `backend_kind: string`
- `desired_state: enum { ENABLED, DISABLED }`
- `management_mode: enum { SMS_REMOTE, SMS_LOCAL }`
- `credential_ref: optional<string>`
- `spec: BackendSpec`
- `labels: map<string, string>`
- `metadata: Struct`
- `generation: uint64`
- `created_at_ms`
- `updated_at_ms`

说明：

- `spec.name` 不再是控制面 identity
- SMS 在创建 backend 时可以统一生成 `spec.name = "backend-{backend_id}"`
- `provider/model` 必须成为显式字段，不能再依赖推断

### AiBackendPlacementRecord

显式表达“这个 backend 应在哪个节点启用”。

第一阶段采用最简单的 per-node placement：

- `placement_id: string`
- `backend_id: string`
- `node_uuid: string`
- `desired_state: enum { ENABLED, DISABLED }`
- `weight_override: optional<int32>`
- `priority_override: optional<int32>`
- `generation: uint64`
- `created_at_ms`
- `updated_at_ms`

后续可扩展：

- `node_selector`
- `placement_policy`
- `rollout_strategy`

### AiBackendNodeStatusRecord

节点观察到的实际状态。

建议字段：

- `backend_id: string`
- `node_uuid: string`
- `observed_generation: uint64`
- `status: enum { PENDING, RECONCILING, READY, DEGRADED, ERROR, DISABLED }`
- `status_reason: string`
- `runtime_backend_name: string`
- `endpoint: string`
- `available: bool`
- `operations: repeated string`
- `features: repeated string`
- `transports: repeated string`
- `last_heartbeat_at_ms`

关键点：

- `backend_id + node_uuid` 唯一
- 节点状态和 generation 绑定，便于判断状态是否过期

### AiModelView

这是只读聚合视图，不承担控制语义。

聚合 key：

- `provider`
- `model`
- `hosting`

建议字段：

- `provider`
- `model`
- `hosting`
- `backend_ids`
- `operations`
- `features`
- `transports`
- `ready_nodes`
- `total_nodes`
- `instances`

## Remote / Local 的统一方式

### Remote Backend

流程：

1. SMS 创建 `AiBackend(hosting=REMOTE)`
2. SMS 创建 placement
3. Spearlet 看到 placement
4. `remote_driver` 将 backend materialize 为 runtime backend
5. Spearlet 上报 `AiBackendNodeStatus`

特点：

- 不启动本地模型进程
- 只把远端 endpoint backend 注入运行时

### Local Backend

流程：

1. SMS 创建 `AiBackend(hosting=LOCAL)`
2. SMS 创建 placement
3. Spearlet 看到 placement
4. 对应 local driver 启动本地模型服务
5. 生成 endpoint/base_url
6. 注入 runtime backend
7. 上报 `AiBackendNodeStatus`

特点：

- 由 driver 负责实际进程/模型生命周期

### 关键结论

`remote` 和 `local` 的差异只存在于 driver 层：

- `remote_driver`
- `llamacpp_driver`
- `vllm_driver`

上层控制面资源模型完全一致。

## Proto 草案

### `proto/sms/ai_backend.proto`

```proto
syntax = "proto3";

package sms;

import "google/protobuf/struct.proto";
import "sms/backend_spec.proto";

enum AiBackendHosting {
  AI_BACKEND_HOSTING_UNSPECIFIED = 0;
  AI_BACKEND_HOSTING_REMOTE = 1;
  AI_BACKEND_HOSTING_LOCAL = 2;
}

enum AiBackendDesiredState {
  AI_BACKEND_DESIRED_STATE_UNSPECIFIED = 0;
  AI_BACKEND_DESIRED_STATE_ENABLED = 1;
  AI_BACKEND_DESIRED_STATE_DISABLED = 2;
}

enum AiBackendManagementMode {
  AI_BACKEND_MANAGEMENT_MODE_UNSPECIFIED = 0;
  AI_BACKEND_MANAGEMENT_MODE_SMS_REMOTE = 1;
  AI_BACKEND_MANAGEMENT_MODE_SMS_LOCAL = 2;
}

message AiBackendRecord {
  string backend_id = 1;
  string display_name = 2;
  string provider = 3;
  string model = 4;
  AiBackendHosting hosting = 5;
  string backend_kind = 6;
  AiBackendDesiredState desired_state = 7;
  AiBackendManagementMode management_mode = 8;
  string credential_ref = 9;
  BackendSpec spec = 10;
  map<string, string> labels = 11;
  google.protobuf.Struct metadata = 12;
  uint64 generation = 13;
  int64 created_at_ms = 14;
  int64 updated_at_ms = 15;
}
```

### `proto/sms/ai_backend_placement.proto`

```proto
syntax = "proto3";

package sms;

import "sms/ai_backend.proto";

message AiBackendPlacementRecord {
  string placement_id = 1;
  string backend_id = 2;
  string node_uuid = 3;
  AiBackendDesiredState desired_state = 4;
  optional int32 weight_override = 5;
  optional int32 priority_override = 6;
  uint64 generation = 7;
  int64 created_at_ms = 8;
  int64 updated_at_ms = 9;
}
```

### `proto/sms/ai_backend_status.proto`

```proto
syntax = "proto3";

package sms;

enum AiBackendNodeStatus {
  AI_BACKEND_NODE_STATUS_UNSPECIFIED = 0;
  AI_BACKEND_NODE_STATUS_PENDING = 1;
  AI_BACKEND_NODE_STATUS_RECONCILING = 2;
  AI_BACKEND_NODE_STATUS_READY = 3;
  AI_BACKEND_NODE_STATUS_DEGRADED = 4;
  AI_BACKEND_NODE_STATUS_ERROR = 5;
  AI_BACKEND_NODE_STATUS_DISABLED = 6;
}

message AiBackendNodeStatusRecord {
  string backend_id = 1;
  string node_uuid = 2;
  uint64 observed_generation = 3;
  AiBackendNodeStatus status = 4;
  string status_reason = 5;
  string runtime_backend_name = 6;
  string endpoint = 7;
  bool available = 8;
  repeated string operations = 9;
  repeated string features = 10;
  repeated string transports = 11;
  int64 last_heartbeat_at_ms = 12;
}
```

## 模块拆分

### SMS

建议新增目录：

```text
src/sms/ai_backends/
  mod.rs
  model.rs
  proto_conv.rs
  repository.rs
  repository_kv.rs
  service.rs
  placement_service.rs
  status_service.rs
  read_model.rs
  validator.rs
```

职责如下：

- `model.rs`
  - domain struct
- `proto_conv.rs`
  - proto/domain 转换
- `repository.rs`
  - trait 定义
- `repository_kv.rs`
  - 基于 KV 的实现
- `service.rs`
  - backend CRUD、enable/disable
- `placement_service.rs`
  - placement CRUD
- `status_service.rs`
  - 节点状态上报、清理
- `read_model.rs`
  - `AiModelView` 聚合
- `validator.rs`
  - 参数校验与约束

### Spearlet

建议新增目录：

```text
src/spearlet/ai/backends/
  mod.rs
  assignment_controller.rs
  driver.rs
  remote_driver.rs
  llamacpp_driver.rs
  vllm_driver.rs
  runtime_registry.rs
  status_reporter.rs
  materializer.rs
```

职责如下：

- `assignment_controller.rs`
  - watch 本节点 placements
  - 拉 backend records
  - 生成 node-local desired assignments
- `driver.rs`
  - 统一 driver trait
- `remote_driver.rs`
  - remote backend materialization
- `llamacpp_driver.rs`
  - llama.cpp 进程管理
- `vllm_driver.rs`
  - vLLM 进程管理
- `runtime_registry.rs`
  - 按 `backend_id` 管理 runtime backend
- `status_reporter.rs`
  - 按 `backend_id` 上报状态

### Web Admin

建议新增目录：

```text
web-admin/src/features/ai-backends/
  AiBackendsPage.tsx
  AiBackendDetailPage.tsx
  AiBackendEditorDialog.tsx
  PlacementPanel.tsx
  PlacementEditorDialog.tsx
  NodeStatusPanel.tsx
  queries.ts
  types.ts
```

信息架构拆成三层：

- `AI Backends`
  - 控制面资源页
- `Placements`
  - 节点启用关系页
- `AI Models`
  - 只读运营聚合页

## KV 存储设计

第一阶段所有控制面资源都可先放入 admin KV。

### 主记录

- `ai:backend:{backend_id}`
- `ai:placement:{placement_id}`
- `ai:status:{backend_id}:{node_uuid}`

### 辅助索引

- `ai:index:placements_by_backend:{backend_id}:{placement_id}`
- `ai:index:placements_by_node:{node_uuid}:{placement_id}`
- `ai:index:statuses_by_backend:{backend_id}:{node_uuid}`

### 原则

- 写入幂等
- 删除必须清理索引
- 优先使用 typed record 序列化
- 第一阶段不追求复杂事务模型

## 核心服务接口草图

### Repository

```rust
pub trait AiBackendRepository {
    async fn insert_backend(&self, record: AiBackendRecord) -> Result<AiBackendRecord, SmsError>;
    async fn update_backend(&self, record: AiBackendRecord) -> Result<AiBackendRecord, SmsError>;
    async fn get_backend(&self, backend_id: &str) -> Result<Option<AiBackendRecord>, SmsError>;
    async fn delete_backend(&self, backend_id: &str) -> Result<bool, SmsError>;

    async fn upsert_placement(
        &self,
        record: AiBackendPlacementRecord,
    ) -> Result<AiBackendPlacementRecord, SmsError>;

    async fn upsert_node_status(
        &self,
        record: AiBackendNodeStatusRecord,
    ) -> Result<AiBackendNodeStatusRecord, SmsError>;
}
```

### Backend Service

```rust
pub trait AiBackendService {
    async fn create_backend(&self, input: CreateAiBackendInput) -> Result<AiBackendRecord, SmsError>;
    async fn update_backend(&self, input: UpdateAiBackendInput) -> Result<AiBackendRecord, SmsError>;
    async fn delete_backend(&self, backend_id: &str) -> Result<bool, SmsError>;
    async fn enable_backend(&self, backend_id: &str) -> Result<AiBackendRecord, SmsError>;
    async fn disable_backend(&self, backend_id: &str) -> Result<AiBackendRecord, SmsError>;
}
```

### Placement Service

```rust
pub trait AiBackendPlacementService {
    async fn upsert_placement(
        &self,
        input: UpsertPlacementInput,
    ) -> Result<AiBackendPlacementRecord, SmsError>;

    async fn delete_placement(&self, placement_id: &str) -> Result<bool, SmsError>;

    async fn list_node_assignments(
        &self,
        node_uuid: &str,
    ) -> Result<Vec<ResolvedBackendAssignment>, SmsError>;
}
```

### Status Service

```rust
pub trait AiBackendStatusService {
    async fn report_statuses(
        &self,
        node_uuid: &str,
        records: Vec<AiBackendNodeStatusRecord>,
    ) -> Result<(), SmsError>;

    async fn cleanup_stale_statuses(&self, older_than_ms: i64) -> Result<u64, SmsError>;
}
```

## 节点侧 reconcile 流程

节点侧不再做“remote sync + local controller”两套平行逻辑，而是统一为 assignment controller。

### 输入

- 当前节点的 placements
- placements 引用到的 backend records

### 中间态

```rust
pub struct ResolvedBackendAssignment {
    pub backend: AiBackendRecord,
    pub placement: AiBackendPlacementRecord,
}
```

### 输出

- 当前节点应该 materialize 的 runtime backend 集合

### Reconcile 流程

1. 拉当前节点全部 placements
2. 过滤 `placement.desired_state == ENABLED`
3. 拉对应 backend records
4. 过滤 `backend.desired_state == ENABLED`
5. 构建 desired assignment set
6. 与当前 runtime set 比较
7. 新增 assignment 交给 driver `reconcile`
8. 移除或 disabled assignment 交给 driver `disable`
9. 生成并上报 node status

### Driver 选择

- `hosting = REMOTE` -> `remote_driver`
- `hosting = LOCAL && backend_kind = llamacpp_*` -> `llamacpp_driver`
- `hosting = LOCAL && backend_kind = vllm_*` -> `vllm_driver`

## UI 信息架构

### AI Backends

这是新的主控制面资源页，一行一个 `backend_id`。

展示字段：

- Name
- Provider
- Model
- Hosting
- Kind
- Desired State
- Enabled Nodes
- Ready Nodes

操作：

- Create
- Edit
- Enable/Disable
- Delete
- Manage Placements

### AI Backend Detail

Tabs 建议：

- `Overview`
- `Placements`
- `Node Status`
- `Spec`

### AI Models

保留，但降级为纯 read model：

- 不再允许 delete
- 不再允许 enable/disable
- 只负责：
  - provider/model 聚合可见性
  - backend coverage
  - node readiness

## 分阶段实施计划

### Phase 0

定义新世界，但不替换旧世界。

内容：

- 新 proto
- 新 domain model
- 新 repository/service skeleton
- 新 read model skeleton
- 新单测骨架

不做：

- 不接旧 UI
- 不迁移旧数据
- 不删除旧逻辑

### Phase 1

打通 SMS 控制面。

内容：

- backend CRUD
- placement CRUD
- status report
- node delete -> status cleanup
- `AiModelView` 聚合

验收：

- `CreateAiBackend` 可用
- `Placement` 可用
- `Status` 可上报
- `Read model` total_count/pagination 正确

### Phase 2

实现 Spearlet assignment controller + remote driver。

内容：

- `assignment_controller`
- `remote_driver`
- `status_reporter`

验收：

- remote backend 能通过 placement 在指定节点 ready

### Phase 3

实现 local driver。

内容：

- `llamacpp_driver`
- `vllm_driver`

验收：

- local backend 能通过 placement 在指定节点启动并 ready

### Phase 4

上线新 Web Admin 页面。

内容：

- `AI Backends`
- `Placements`
- `Node Status`
- `AI Models` 只读化

### Phase 5

移除旧模型。

删除：

- `admin_backends.rs`
- `model_deployments.rs`
- `backend_assignment_controller.rs`
- 旧 local controller 主路径
- `AI Models` 控制语义相关逻辑

## 当前实现状态

截至当前代码实现，统一 AI backend control plane 已完成首批端到端落地，主要包括：

- 新 proto：
  - `proto/sms/ai_backend.proto`
  - `proto/sms/ai_backend_placement.proto`
  - `proto/sms/ai_backend_status.proto`
- 新 SMS 模块：
  - `src/sms/ai_backends/model.rs`
  - `src/sms/ai_backends/proto_conv.rs`
  - `src/sms/ai_backends/repository.rs`
  - `src/sms/ai_backends/repository_kv.rs`
  - `src/sms/ai_backends/service.rs`
  - `src/sms/ai_backends/placement_service.rs`
  - `src/sms/ai_backends/status_service.rs`
  - `src/sms/ai_backends/read_model.rs`
  - `src/sms/ai_backends/validator.rs`
- `build.rs` 已接入新的 proto 生成
- `src/sms/mod.rs` 已导出 `ai_backends` 模块
- 新 gRPC service 已接入 SMS：
  - `proto/sms/ai_backend_control_plane.proto`
  - `src/sms/ai_backend_rpc.rs`
  - `src/sms/grpc_server.rs`
- 新 HTTP/Web Admin API 已接入：
  - `src/sms/web_admin/router.rs`
  - `src/sms/web_admin.rs`
  - `src/sms/gateway.rs`
- 新 Web Admin 前端已接入：
  - `web-admin/src/api/ai-backends.ts`
  - `web-admin/src/features/ai-backends/*`
  - `web-admin/src/app/AppShell.tsx`
- 新 Spearlet assignment controller 已接入：
  - `src/spearlet/ai/backend_assignment_controller.rs`
  - `src/apps/spearlet/main.rs`
  - `src/spearlet/execution/ai/router/mod.rs`
  - `src/spearlet/backend_reporter.rs`
  - 当前只要 Spearlet 连接 SMS，就会启动该 controller，并直接承担 remote backend 收敛

当前这批实现的边界如下：

- 已完成：
  - UUID identity 的 backend/placement/status typed model
  - KV 主记录与二级索引的 repository
  - backend CRUD、placement upsert、status report 的 service
  - `AiModelView` 聚合与 read model
  - proto/domain conversion
  - SMS 侧 gRPC control-plane service
  - HTTP/Web Admin control-plane API
  - Web Admin 前端列表页、详情页、服务端分页/筛选与 placement/status 查询
  - Spearlet 侧基于 assignment 的 remote backend materialization 与 node status 上报
  - Spearlet 侧 `llamacpp` local assignment 的初步接入
  - Spearlet 侧显式 local provider 分流与 `vllm` placeholder 状态语义
  - Spearlet 侧 `vllm` 已支持 `external_endpoint` 接入模式，可将节点上已有的 vLLM 服务纳入统一控制面
  - Spearlet 侧 `llamacpp` 中间生命周期状态（`reconciling -> ready/error`）上报
  - 旧 `remote_backend_sync` 与旧 `LocalModelController` 运行时实现均已从节点侧移除
  - 节点侧 AI backend 生命周期现统一由 assignment controller 接管
  - Spearlet 现固定按 unified-only AI control plane 启动
  - monitoring 现暴露 unified controller 的 dynamic backend registry source 快照与同名 backend 冲突检测
  - 节点侧 AI backend 生命周期现固定由 unified assignment controller 驱动
  - Web Admin `AI Models` 已切到 unified read model（`ai-model-views`），并收敛为只读聚合页
  - Web Admin 旧 `/admin/api/ai-models`、`/admin/api/nodes/{uuid}/ai-models*`、`/admin/api/ai/remote-backends*` 入口已下线
  - SMS 旧 gRPC `AdminAiConfigService` 与 `ModelDeploymentRegistryService` 已移除
  - 已接受破坏式切换：旧 `admin_backends` / `model_deployments` 数据不再迁移，升级后仅认 unified `ai_backends`
  - 旧 `admin_backends.rs`、`model_deployments` helper 与对应 proto 已从活跃代码中删除
  - `vllm` 已迁入独立 skeleton module：`src/spearlet/local_models/vllm.rs`
  - 基础单测
- 尚未完成：
  - Spearlet `vllm` 与更多 local provider assignment 到 driver 的正式收敛

这意味着当前代码已经具备继续推进 `Phase 1` 的基础，但还没有替换现网控制路径。

## 测试策略

### SMS

- backend CRUD 单测
- placement CRUD 单测
- status report 单测
- stale status cleanup 单测
- node delete 联动清理单测
- `AiModelView` 聚合与分页单测

### Spearlet

- assignment controller 幂等测试
- remote driver reconcile/disable
- local driver reconcile/disable
- status reporter generation 对齐

### E2E

- create remote backend + placement -> ready
- disable backend -> disabled
- delete placement -> node no longer serves backend
- create local backend + placement -> process up + ready
- same provider/model with multiple backend_id -> `AI Models` 聚合正确，`AI Backends` 分开展示

## 风险与开放问题

### 1. `BackendSpec` 是否足够作为统一执行描述

当前看是足够的，但本地 driver 如果需要更多 provider-specific 参数，可能需要：

- 扩展 `metadata`
- 或在 `spec` 外增加 `driver_config`

### 2. Placement 是否需要 node selector

第一阶段建议先做 `node_uuid` 粒度 placement，避免过早复杂化。

### 3. Node status 是否需要 event sourcing

第一阶段建议先直接持久化 latest status。

如果后续要做审计与回放，再增加 event log。

### 4. 是否保留 `AI Models` 页面

建议保留，但明确只读化，不能继续承担 destructive control 语义。

## 推荐决策

建议立即确认以下两个硬规则：

1. `backend_id` 是唯一资源 identity，`provider/model/name` 永远不再承担主键角色
2. `AI Models` 只允许作为 read model，不再承载 delete / enable / disable 等控制语义

如果这两个决策成立，那么后续实施路径就会非常清晰。

## 相关实现参考

- 当前 unified backend 持久化：`src/sms/ai_backends/repository_kv.rs`
- 当前 AI Models 只读聚合：`src/sms/ai_backends/read_model.rs`
- 当前 AI backend gRPC 入口：`src/sms/ai_backend_rpc.rs`
- 当前远端与本地 backend 控制器：`src/spearlet/ai/backend_assignment_controller.rs`
- 当前本地 provider 支撑：`src/spearlet/local_models/provider.rs`
- 现有 backend 上报：`src/spearlet/backend_reporter.rs`
