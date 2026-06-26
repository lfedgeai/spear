# AI Models（UI 命名）与本地模型部署（Local Provider）设计文档

## 1. 背景与目标

当前系统在运行时与 Web Admin 中主要以 “Backends” 作为可观测与路由的抽象：Spearlet 通过 `spearlet.ai.backends[]`（以及可选的 discovery，例如 Ollama 导入）构建运行时 registry，并周期性向 SMS 上报节点 backend 快照（见 [backend_registry.proto](../proto/sms/backend_registry.proto) 与 Spearlet 的 reporter 逻辑）。

你希望：

- UI 上将 “Backends” 更名为 “AI Models”，并在 “AI Models” 下按层级展示：
  - Remote：OpenAI 等
  - Local：Ollama、llama.cpp、vLLM 等
- 明确 backend 是否属于 remote 或 local（是否需要新增类型字段）。
- 在 “Local” 下允许在 Web Admin 中选择某个节点，创建某个 provider（ollama / llamacpp / vllm）的某个模型，并纳入路由与可观测。

本文给出一套偏行业规范（industry norm）与最佳实践（best practice）的设计，并且细化到可落地的协议（proto）与核心函数边界，供你 review 并据此实施。

非目标（本设计不强行要求你一次做完）：

- 不要求你立刻把所有 local provider 都做成 “自动下载安装 + 全生命周期托管”。
- 不要求引入 Kubernetes CRD 等重量级控制面（但会给出与之对齐的设计模式）。

## 2. 关键结论（TL;DR）

- UI 更名为 “AI Models”是合理的，因为用户心智是“我在用哪个模型”，而不是“我在选哪个 backend 连接器”。
- 内部仍建议保留 “Backend” 作为路由与执行的基础抽象：backend 是“可调用端点实例（endpoint instance）”，model 是“请求参数/能力维度”；UI 的 “AI Models” 本质是对 backends 的聚合视图（model-centric view）。
- 是否需要给 backend 增加 type（remote/local）：
  - 推荐增加一个显式字段（例如 `hosting` / `location`），避免仅靠 `kind` 或 `base_url` 做脆弱推断。
  - 同时保持向后兼容：当新字段缺失时，UI/聚合层可用推断规则兜底。
- Web Admin 在某个节点创建 local provider 的某个模型：推荐采用 **“声明式期望状态（desired state） + 节点侧 reconcile（控制循环） + 状态上报”** 的控制器模式（与 Kubernetes 一致），而不是“Web Admin 直接远程执行命令”。

## 3. 术语与分层（建议统一）

为了避免 “backend / provider / model” 混用导致 UI 与协议不清晰，建议固定以下分层：

### 3.1 Provider（提供方/引擎）

Provider 表示“模型服务实现形态”，例如：

- Remote provider：OpenAI、Azure OpenAI、OpenAI-compatible SaaS
- Local provider：Ollama、llama.cpp server、vLLM server

它回答的问题是：**谁在提供推理服务、通过什么协议暴露出来**。

### 3.2 Model（模型标识）

Model 表示“用户可选择的模型名/版本”，例如：

- `gpt-4o-mini`
- `llama3:8b`
- `Qwen2.5-7B-Instruct`

它回答的问题是：**推理目标是什么模型**。

### 3.3 Backend（路由与执行的最小单元）

Backend 表示“可调用的端点实例（endpoint instance）”，它通常包含：

- `kind`（适配器类型，例如 `openai_chat_completion`、`ollama_chat`）
- `base_url`（要请求的地址）
- `credential_ref`（如需鉴权）
- `capabilities`（ops/features/transports/weight/priority 等）
- `model`（可选：固定绑定某个模型；若为空则代表动态 model）

它回答的问题是：**我到底向哪个 endpoint 发请求、它能做什么、优先级/权重是多少**。

### 3.4 AI Model（UI 展示对象）

UI 的 “AI Models” 推荐定义为对 backend 的聚合视图：

- 一个 AI Model 由 `(provider, model, hosting)` 唯一标识（推荐）。
- 一个 AI Model 下可映射多个 backend（例如同一模型在多个节点以本地方式部署、或者同一模型有多个 remote endpoint）。

## 4. UI 信息架构（从 Backends 到 AI Models）

### 4.1 顶层导航命名

将 Web Admin 的 “Backends” 改为 “AI Models” 更贴近用户心智；但 UI 内部可以保留 “Instances / Backends” 子视图用于运维排障。

推荐层级：

- AI Models
  - Remote
    - OpenAI
    - OpenAI-compatible
    - …
  - Local
    - Ollama
    - llama.cpp
    - vLLM
    - …

### 4.2 列表主视图（AI Models）

每行代表一个 AI Model（聚合对象），字段建议：

- Provider（OpenAI/Ollama/vLLM/…）
- Model（模型名；若 backend 未固定绑定，则显示 “(dynamic)” 或折叠为 Provider-level）
- Hosting（Remote/Local）
- Operations（chat_completions、speech_to_text、embeddings 等；仅展示当前已实现或已注册能力）
- Available nodes / Total nodes（仅对 Local 或多节点 remote 有意义）
- Status（聚合：例如有任一 backend available 则 partial/available）

点击进入详情页/抽屉：

- 展示其下 backend instances（按 node 分组），包括 `base_url`、`weight/priority`、`status_reason`。

### 4.3 Local 下的创建入口

Local → 某 Provider（例如 Ollama）→ “Create model on node”

这里的用户操作语义应是：**创建/更新一条“模型部署期望（Model Deployment Spec）”**，由节点侧去落实。

## 5. 数据模型设计

### 5.1 扩展 backend 快照的最小字段（推荐）

现有的 backend 快照模型在 SMS/Spearlet 间传递（见 [backend_registry.proto](../proto/sms/backend_registry.proto)），当前 `BackendInfo` 缺少对 UI “AI Models”聚合最关键的两个字段：`model` 与 “remote/local”。

建议以 **向后兼容** 的方式扩展（proto3 新增字段天然兼容）：

#### 新增枚举：BackendHosting（示意）

```proto
enum BackendHosting {
  BACKEND_HOSTING_UNSPECIFIED = 0;
  BACKEND_HOSTING_REMOTE = 1;     // 请求远端服务（SaaS / remote cluster）
  BACKEND_HOSTING_NODE_LOCAL = 2; // 节点本地（loopback 或 node-local）
}
```

#### 扩展 BackendInfo（示意）

```proto
message BackendInfo {
  // existing fields...

  // provider identifier for grouping / provider 标识（用于 UI 分组）
  string provider = 11;

  // fixed bound model name if any / 若固定绑定模型，则填模型名
  string model = 12;

  // explicit hosting type / 显式声明 remote vs local
  BackendHosting hosting = 13;
}
```

兼容策略：

- 新字段缺失时，SMS 聚合层可用以下兜底推断：
  - `kind` in `openai_*` → Remote
  - `kind` in `ollama_*` 且 `base_url` 为 loopback → NodeLocal
  - 其它 → Unspecified（显示为 “Unknown”）

为什么不只靠推断：

- `base_url` 可能是 `http://10.x.x.x:11434`（非 loopback 的 Ollama），它可能是“同集群本地”或“远端”，靠规则容易错。
- `kind` 只是适配器类型，不等价于部署形态。

### 5.2 UI 聚合对象：AiModelInfo（建议由 SMS 计算并返回）

为避免前端做复杂聚合，建议由 SMS 的 Web Admin BFF 提供一个 model-centric API。建议的数据形态：

```json
{
  "provider": "ollama",
  "model": "llama3:8b",
  "hosting": "local",
  "operations": ["chat_completions"],
  "features": ["supports_stream"],
  "transports": ["http"],
  "available_nodes": 3,
  "total_nodes": 5,
  "instances": [
    {
      "node_uuid": "…",
      "backend_name": "ollama/llama3-8b",
      "kind": "ollama_chat",
      "base_url": "http://127.0.0.1:11434",
      "status": "available",
      "status_reason": ""
    }
  ]
}
```

实现来源：

- 复用现有 `BackendRegistryService.ListNodeBackendSnapshots()` 拉到节点快照，然后在 SMS 侧做 grouping：
  - key = `(provider, model, hosting)`；当 `model` 缺失时可退化 key = `(provider, "(dynamic)", hosting)`。

## 6. Local 模型“创建/部署”的控制面最佳实践

### 6.1 为什么要 “desired state + reconcile”

如果 Web Admin 直接对某节点执行：

- `ssh node && ollama pull …`
- `ssh node && start vllm …`

会立刻遇到问题：

- 安全：远程命令执行面太大（RCE 风险）。
- 不可审计：难以追踪谁做了什么变更。
- 不可恢复：节点重启/进程崩溃后无人保证恢复到期望状态。

因此推荐对齐行业规范：**SMS 存储“期望状态”，Spearlet 类似 kubelet 做节点侧控制循环（controller）**。

### 6.2 新增概念：ModelDeployment（本地模型部署记录）

一个 ModelDeployment 代表 “在某些节点上确保某 provider 的某模型处于可用状态”，其核心字段：

- `deployment_id`（稳定 ID）
- `target`（node_uuid 或 node selector）
- `provider`（ollama/llamacpp/vllm）
- `model`（模型标识）
- `serving`（端口、并发、GPU、ctx 等参数；provider-specific）
- `lifecycle`（创建/更新/删除策略）

### 6.3 协议：ModelDeploymentRegistryService（建议新增 proto）

参考现有的 [mcp_registry.proto](../proto/sms/mcp_registry.proto) 的 “revision + watch” 模式，建议新增一个 registry service：

#### Proto（示意）

```proto
syntax = "proto3";
package sms;

enum ModelDeploymentPhase {
  MODEL_DEPLOYMENT_PHASE_UNSPECIFIED = 0;
  MODEL_DEPLOYMENT_PHASE_PENDING = 1;
  MODEL_DEPLOYMENT_PHASE_PULLING = 2;
  MODEL_DEPLOYMENT_PHASE_STARTING = 3;
  MODEL_DEPLOYMENT_PHASE_READY = 4;
  MODEL_DEPLOYMENT_PHASE_FAILED = 5;
  MODEL_DEPLOYMENT_PHASE_DELETING = 6;
}

message ModelDeploymentSpec {
  string target_node_uuid = 1; // MVP: 单节点绑定
  string provider = 2;         // ollama | llamacpp | vllm
  string model = 3;            // model identifier
  map<string, string> params = 4; // provider-specific, strongly validate in code
}

message ModelDeploymentStatus {
  ModelDeploymentPhase phase = 1;
  string message = 2;
  int64 updated_at_ms = 3;
}

message ModelDeploymentRecord {
  string deployment_id = 1;
  uint64 revision = 2;
  int64 created_at_ms = 3;
  int64 updated_at_ms = 4;
  ModelDeploymentSpec spec = 5;
  ModelDeploymentStatus status = 6;
}

message ListModelDeploymentsRequest {
  uint32 limit = 1;
  uint32 offset = 2;
  string target_node_uuid = 3; // optional filter
  string provider = 4;         // optional filter
}

message ListModelDeploymentsResponse {
  uint64 revision = 1;
  repeated ModelDeploymentRecord records = 2;
  uint32 total_count = 3;
}

message WatchModelDeploymentsRequest {
  uint64 since_revision = 1;
  string target_node_uuid = 2; // spearet 节点只 watch 自己
}

message ModelDeploymentEvent {
  uint64 revision = 1;
  repeated string upserts = 2;
  repeated string deletes = 3;
}

message WatchModelDeploymentsResponse {
  ModelDeploymentEvent event = 1;
}

message UpsertModelDeploymentRequest { ModelDeploymentRecord record = 1; }
message UpsertModelDeploymentResponse { uint64 revision = 1; }
message DeleteModelDeploymentRequest { string deployment_id = 1; }
message DeleteModelDeploymentResponse { uint64 revision = 1; }

message ReportModelDeploymentStatusRequest {
  string deployment_id = 1;
  string node_uuid = 2;
  uint64 observed_revision = 3;
  ModelDeploymentStatus status = 4;
}
message ReportModelDeploymentStatusResponse { bool success = 1; }

service ModelDeploymentRegistryService {
  rpc ListModelDeployments(ListModelDeploymentsRequest) returns (ListModelDeploymentsResponse);
  rpc WatchModelDeployments(WatchModelDeploymentsRequest) returns (stream WatchModelDeploymentsResponse);
  rpc UpsertModelDeployment(UpsertModelDeploymentRequest) returns (UpsertModelDeploymentResponse);
  rpc DeleteModelDeployment(DeleteModelDeploymentRequest) returns (DeleteModelDeploymentResponse);
  rpc ReportModelDeploymentStatus(ReportModelDeploymentStatusRequest) returns (ReportModelDeploymentStatusResponse);
}
```

说明：

- MVP 建议先做 `target_node_uuid` 单节点绑定，降低调度复杂度；未来可扩展为 label selector。
- `params` 必须在代码中做强校验与 allowlist，避免把任意命令/路径直接塞进去造成 RCE。
- `ReportModelDeploymentStatus` 用于 UI 展示“正在拉取/启动失败”等过程状态；真正参与路由的仍以 backend snapshot 为准（避免双源不一致）。

### 6.4 Spearlet 节点侧：LocalModelController（reconcile loop）

节点侧应新增一个长期运行的控制器，职责：

- watch 自己相关的 ModelDeployment 变更（gRPC stream）
- 对每个 deployment 执行 reconcile：确保 provider/model 处于 desired 状态
- 将“可调用端点”注册为 backend，并通过现有 backend reporter 上报到 SMS
- 上报 deployment status（phase/message）

#### 关键 trait（Rust，示意）

```rust
pub trait LocalModelDriver: Send + Sync {
    fn provider(&self) -> &'static str;

    async fn ensure_model_present(
        &self,
        model: &str,
        params: &std::collections::HashMap<String, String>,
    ) -> Result<(), LocalModelError>;

    async fn ensure_serving(
        &self,
        model: &str,
        params: &std::collections::HashMap<String, String>,
    ) -> Result<ServingEndpoint, LocalModelError>;

    async fn stop_serving(
        &self,
        model: &str,
        params: &std::collections::HashMap<String, String>,
    ) -> Result<(), LocalModelError>;
}

pub struct ServingEndpoint {
    pub base_url: String,
    pub kind: String,                 // e.g. "ollama_chat" | "openai_compatible_chat"
    pub fixed_model: Option<String>,  // bind model if applicable
    pub transports: Vec<String>,
}
```

驱动实现建议：

- OllamaDriver（MVP 推荐）：
  - 依赖节点上已有 Ollama daemon
  - `ensure_model_present`：调用 Ollama API 触发 pull（或执行 `ollama pull`，但需更严格的安全模型）
  - `ensure_serving`：Ollama 对已安装模型通常可立即 serve；返回 `base_url=http://127.0.0.1:11434`
- llama.cpp / vLLM（建议分期）：
  - Phase 1：支持“节点托管进程”（启动本地服务并探活）
  - llama.cpp（llamacpp）建议优先支持 GGUF 文件直链下载：
    - params：`model_url`（http/https，指向具体 `.gguf` 文件），或 `model_path`（已存在的本地文件路径）
    - 本地缓存：当 `model_path` 已存在时跳过下载
    - 不依赖 `llama-cli --hf-repo/--hf-file` 作为下载方式
  - Phase 2：引入 container 管理，并强约束镜像 allowlist 与资源隔离

#### 控制循环状态机（建议）

- Pending：刚创建，尚未执行
- Pulling：下载/拉取模型
- Starting：启动服务进程或等待 ready
- Ready：可用（同时 backend snapshot 中该 backend 应为 available）
- Failed：失败（message 包含原因；可重试）
- Deleting：删除中（停止服务/清理）

### 6.5 与现有 Ollama discovery 的关系

已有的 Ollama discovery（见 [ollama-discovery-zh.md](./ollama-discovery-zh.md)）属于“运行态发现（read-only）”，它解决的是：

- 节点上已有 Ollama 模型 → 自动生成 backend 并展示/路由

本设计新增的 ModelDeployment 属于“控制面期望（write/control）”，它解决的是：

- Web Admin 希望在某节点“创建/确保”某模型存在并可用

两者可并存：

- discovery：用于“自动展示已有的”
- deployment：用于“声明式创建/管理的”

优先级建议：

- deployment 生成的 backend 命名使用单独前缀（例如 `managed/ollama/<model>`），避免与 discovery 的 `ollama/<model>` 冲突。

### 6.6 落地到代码的函数与模块划分（建议）

本节把上面的协议与控制器设计映射到当前代码库已有结构，减少实施时的歧义。

#### SMS：ModelDeployment registry（控制面存储与 gRPC）

对齐现有 MCP registry 的模式（参见 [mcp_registry.proto](../proto/sms/mcp_registry.proto)），建议新增：

- Proto：`proto/sms/model_deployment_registry.proto`（新增 service 与 record）
- Watch 基础设施：复用 SMS 现有的通用 watch hub：[registry_watch.rs](../src/sms/registry_watch.rs) 的 `RegistryWatchHub`（MCP registry 已迁移到该实现，见 [service.rs](../src/sms/service.rs)）。
  - 语义保持一致：`since_revision` 太旧返回 `FAILED_PRECONDITION("since_revision too old; resync required")`，消费者落后导致 broadcast lag 返回 `ABORTED("watch lagged; resync required")`。
  - 客户端约定：遇到上述错误应先 `List` 全量重同步，再从新 revision 继续 `Watch`。
- Rust store trait（示意）：

```rust
pub trait ModelDeploymentStore: Send + Sync {
    fn revision(&self) -> u64;

    fn list(
        &self,
        limit: u32,
        offset: u32,
        target_node_uuid: Option<&str>,
        provider: Option<&str>,
    ) -> Result<(Vec<ModelDeploymentRecord>, u32), StoreError>;

    fn get(&self, deployment_id: &str) -> Result<Option<ModelDeploymentRecord>, StoreError>;

    fn upsert(&self, record: ModelDeploymentRecord) -> Result<u64, StoreError>;

    fn delete(&self, deployment_id: &str) -> Result<u64, StoreError>;

    fn update_status(
        &self,
        deployment_id: &str,
        node_uuid: &str,
        observed_revision: u64,
        status: ModelDeploymentStatus,
    ) -> Result<u64, StoreError>;
}
```

推荐实现路径：

- 先做 in-memory（测试/快速闭环），再复用现有 KV 抽象持久化（类似其它 registry/store 的落地方式）。

gRPC service 实现（示意函数边界）：

- `ModelDeploymentRegistryServiceImpl::list_model_deployments(...)`
- `ModelDeploymentRegistryServiceImpl::watch_model_deployments(...)`
- `ModelDeploymentRegistryServiceImpl::upsert_model_deployment(...)`
- `ModelDeploymentRegistryServiceImpl::delete_model_deployment(...)`
- `ModelDeploymentRegistryServiceImpl::report_model_deployment_status(...)`

#### SMS：Web Admin BFF（/admin/api）

建议在 `src/sms/web_admin.rs` 中新增 handlers（与现有 backends/mcp handlers 保持一致风格）：

- `list_ai_models(...)`：从 `BackendRegistryService.ListNodeBackendSnapshots` 聚合并返回 `AiModelInfo[]`
- `get_ai_model_detail(...)`：返回某 `(provider, model)` 的实例分布
- `create_node_ai_model_deployment(...)`：把 HTTP body 转换成 `UpsertModelDeploymentRequest`
- `delete_node_ai_model_deployment(...)`：删除 deployment
- `list_node_ai_model_deployments(...)`：列出 node 下 deployments 与状态

#### Spearlet：LocalModelController（节点侧控制循环）

启动时机：

- 仅当 Spearlet 已连接 SMS（已有 “connect_requested” 分支）时启动，类似现有：
  - MCP registry sync
  - task subscriber
  - backend reporter

模块划分建议：

- `src/spearlet/local_models/mod.rs`
- `src/spearlet/local_models/controller.rs`：watch + reconcile + status report
- `src/spearlet/local_models/driver.rs`：`LocalModelDriver` trait 与通用错误类型
- `src/spearlet/local_models/drivers/ollama.rs`：OllamaDriver

控制循环核心函数（示意）：

```rust
impl LocalModelController {
    pub fn start(&self);

    async fn watch_loop(&self) -> Result<(), ControllerError>;

    async fn reconcile_one(&self, record: ModelDeploymentRecord) -> Result<(), ControllerError>;

    async fn apply_ready_backend(&self, endpoint: ServingEndpoint) -> Result<(), ControllerError>;

    async fn report_status(&self, deployment_id: &str, status: ModelDeploymentStatus);
}
```

与 backend registry 的联动：

- reconcile 成功后，生成一个 `AiBackendConfig`（或等价的运行时 backend spec）注入到 registry（建议用 “动态 registry handle” 的方式，见 [ollama-model-import-design-zh.md](./ollama-model-import-design-zh.md) 中关于 registry 热更新的建议），并确保 backend reporter 上报能看到该 backend。

## 7. Web Admin API 设计（BFF）

### 7.1 AI Models（聚合查询）

- `GET /admin/api/ai-models`
  - 返回 `AiModelInfo[]`（由 SMS 聚合 backend snapshots 计算）
  - query：
    - `hosting=remote|local`
    - `provider=ollama|openai|...`
    - `q=`（模糊搜索 provider/model/backend_name）
    - `limit/offset`

- `GET /admin/api/ai-models/{provider}/{model}`
  - 返回单个模型的聚合详情（包含 instances）

兼容性：

- 保留现有 `/admin/api/backends`，作为 “Instances（原 backend 视图）” 的底层数据源或调试入口。

### 7.2 Local：创建/删除模型部署

- `POST /admin/api/nodes/{node_uuid}/ai-models`
  - body：`{ provider, model, params }`
  - 行为：在 SMS 中 upsert 一条 ModelDeploymentRecord（target_node_uuid=node_uuid）

- `DELETE /admin/api/nodes/{node_uuid}/ai-models/deployments/{deployment_id}`
  - 行为：删除 deployment 记录（Spearlet 收到后 reconcile 停止服务并清理）

- `GET /admin/api/nodes/{node_uuid}/ai-models/deployments`
  - 行为：列出该节点相关的 deployments 与状态

## 8. 安全与合规（必须）

### 8.1 SSRF 防护

Local provider 常通过 `base_url` 访问本机服务（Ollama）。必须保持默认安全：

- 默认仅允许 loopback（`127.0.0.1/localhost/[::1]`）
- 若允许 remote，则必须额外做 CIDR deny（拒绝 metadata/link-local/private ranges 视你的威胁模型）

当支持通过 URL 下载模型文件（例如 llamacpp 的 `params.model_url`）时，同样需要 SSRF 防护：

- 默认仅允许 `http/https`，并限制到允许的域名/网段（建议 allowlist）
- 必须拒绝 `localhost/127.0.0.1/[::1]`、link-local、私网 CIDR、以及云厂商 metadata 地址

### 8.2 禁止 secrets 出现在协议与 UI

延续现有 best practice：

- 协议只传 `credential_ref` / env var 名称，不传值
- status_reason 不包含任何 secret 值

### 8.3 命令执行面最小化

如要做“自动拉取/启动进程”，必须：

- params 强校验（allowlist keys + format）
- 可执行文件/镜像 allowlist
- 运行账号最小权限
- 审计日志（谁在 Web Admin 创建了什么 deployment）

## 9. 分期落地建议（务实路线）

### Phase A：仅 UI 语义升级（最快闭环）

- UI：Backends → AI Models（分组 Remote/Local，仅展示）
- SMS：在 Web Admin BFF 侧新增 `/admin/api/ai-models`，对现有 backend snapshots 做聚合
- 不改 Spearlet 协议：hosting/provider/model 暂用推断

### Phase B：补齐协议字段（减少推断）

- 扩展 `BackendInfo` 增加 `provider/model/hosting`
- Spearlet 上报时填充这些字段（从 `AiBackendConfig.kind/model/base_url` 映射）

### Phase C：Local（Ollama）“创建模型”MVP

- SMS：实现 ModelDeploymentRegistryService + Web Admin 入口
- Spearlet：实现 LocalModelController + OllamaDriver（只依赖已有 daemon，通过 API 触发 pull）
- 路由：deployment 完成后，Spearlet 生成 `managed/ollama/<model>` backend 并上报

### Phase D：llama.cpp / vLLM 托管（可选）

- 先“注册 endpoint”，后“托管进程/容器”
- 强制 allowlist 与资源隔离，避免把系统变成通用远程执行平台
