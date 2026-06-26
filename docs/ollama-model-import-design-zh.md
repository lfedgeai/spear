# Ollama Serving Models 自动导入（配置与实现细节设计）

## 背景

当前 Spearlet 的 AI backends 来自静态配置 `spearlet.ai.backends[]`，并在进程启动时由 [builder.rs](../src/spearlet/execution/ai/router/builder.rs) 构建为运行时 `BackendRegistry`。这带来两个限制：

- 无法根据节点上 **Ollama 正在 serve 的模型** 动态生成可用 backend 列表。
- Web Admin 的“Backends”视图只能看到静态配置（或本地生成的固定项），无法直接反映 Ollama 运行态。

你希望提供一个配置开关：决定是否把 Ollama 当前正在 serve 的模型“导入”为 Spearlet 可路由的 backend（并在 Web Admin 可见）。

相关文档：

- Backend 注册发现与配置：[backend-adapter/backends-zh.md](./backend-adapter/backends-zh.md)
- LLM credential_ref 规范：[implementation/llm-credentials-implementation-zh.md](./implementation/llm-credentials-implementation-zh.md)
- Backend 可观测聚合（节点主动上报）：[backend-registry-api-design-zh.md](./backend-registry-api-design-zh.md)

## 目标

- 提供一个工业规范的配置项，默认关闭，显式开启后才导入 Ollama serving models。
- 导入的模型以“每个模型一个 backend instance”的形式加入路由候选。
- 导入过程具备：限流、缓存、背压、错误隔离（失败不影响已有 backends）、可观测性。
- 不引入 secret 泄露风险，不允许通过导入机制制造 SSRF。

## 非目标

- 不做“installed models（已安装但未在跑）”的全量导入（可作为未来扩展）。
- 不在本设计里提供 Web Admin 编辑/下发 Ollama 配置。
- 不定义跨节点的模型调度与迁移（这属于更上层的调度问题）。

## Best Practice 总结（业界常见做法）

- **显式 opt-in**：自动发现/导入默认关闭，避免“上线后突然出现新 backends 影响路由”。
- **可控范围**：必须有 allowlist/denylist 和最大导入数量（`max_models`）。
- **稳定命名**：导入生成的 backend 名称必须可预测且不与用户手写配置冲突。
- **分离静态与动态**：静态配置是“声明式”来源；动态导入是“运行态观测”来源，二者合并但保留可追溯 metadata。
- **失败隔离**：Ollama 不可用/接口变更/返回异常时，只影响导入源，不影响现有路由。
- **安全默认值**：默认只允许 `localhost/127.0.0.1`，显式配置才允许远端。

## 配置设计

在 `SpearletConfig.ai` 下新增一节 `discovery`（遵循现有 `deny_unknown_fields`，必须在 schema 中显式加入字段）：

```toml
[spearlet.ai.discovery.ollama]
enabled = false

# Ollama HTTP endpoint
base_url = "http://127.0.0.1:11434"

# 导入范围：
# - serving: 只导入当前正在运行/serve 的模型（推荐，符合你的需求）
# - installed: 导入已安装模型（未来扩展）
scope = "serving"

# 周期刷新（秒）。0 表示只在启动时导入一次
refresh_interval_secs = 15

# 安全：是否允许非 localhost 的 base_url（默认 false）
allow_remote = false

# 导入数量上限（防止节点返回过多模型导致路由膨胀）
max_models = 20

# 命名规则
name_prefix = "ollama/"

# 冲突策略：
# - skip: 如果 name 已存在（用户静态配置同名），跳过导入
# - override: 覆盖同名项（不推荐，容易造成意外）
name_conflict = "skip"

# allow/deny 支持 glob（例如 llama3*）
allow_models = ["*"]
deny_models = []

# 导入 backend 的默认 capability
ops = ["chat_completions"]
features = ["supports_stream"]
transports = ["http"]

# 路由默认参数
weight = 10
priority = -10

# 模型名到 backend 的映射策略
# - backend_name: 后端名仅用于展示，仍由请求中的 model 决定
# - fixed_default_model: 每个导入 backend 固定绑定一个模型（推荐）
binding_mode = "fixed_default_model"
```

### 为什么要放到 `spearlet.ai.discovery.*`

- 语义清晰：`ai.backends[]` 是声明式配置；`ai.discovery.*` 是运行态发现。
- 易扩展：未来可以加入 `discovery.openai_compatible`, `discovery.k8s_service`, `discovery.file` 等。

## 行为语义（详细）

### 1）导入数据源

Ollama “正在 serve 的模型”建议通过 Ollama API `GET /api/ps` 获取。

- `scope=serving`：只导入 `/api/ps` 返回的模型。
- `scope=installed`（未来）：通过 `GET /api/tags` 导入已安装模型。

### 2）导入对象如何映射为 BackendInstance

每条导入记录生成一个 backend instance：

- `name = name_prefix + sanitize(model_name)`
  - `sanitize`：把 `:`, `@`, 空格等替换为 `-` 或 URL-safe 编码，保证可用于路由参数与 URL。
- `kind = "ollama_chat"`（建议新增一种 backend kind，**不需要 API key**）
- `base_url = discovery.ollama.base_url`
- `ops/features/transports/weight/priority` 使用 discovery 默认值

如果 `binding_mode=fixed_default_model`：

- 为该 backend 绑定 `default_model = model_name`
- adapter 调用 Ollama 时始终使用 `default_model`，不依赖 session 传入

如果 `binding_mode=backend_name`：

- backend 不绑定模型，仍由请求里的 `model` 决定
- 这只解决“展示可选模型列表”，不保证路由层面“每个模型一个 backend”语义

推荐 `fixed_default_model`，因为它让“导入=可路由”闭环成立。

### 3）冲突处理

当导入生成的 `name` 与静态配置 `ai.backends[].name` 重名时：

- 默认 `name_conflict=skip`：跳过导入项，并记录结构化日志（backend_name/model_name）。
- 不建议 `override`：容易让运维误以为仍在使用静态配置，实际已被导入覆盖。

### 4）刷新与一致性

`refresh_interval_secs` > 0 时：

- Spearlet 周期性刷新 `/api/ps`。
- 若某模型从 serving 列表消失：
  - 从导入集合移除
  - 触发 registry rebuild（见实现建议）

为了避免短暂抖动：

- 可选 `min_stable_cycles`（未来扩展）：连续 N 次不存在才删除。

### 5）失败处理与降级

- Ollama unreachable / timeout / 5xx：
  - 保留上一次成功的导入集合（stale），并在 metric/log 标记
  - 不影响静态 backends
- payload 解析失败：
  - 本次导入失败，保留上一次快照

## 实现建议（架构优雅）

### 1）新增 OllamaDiscoveryService

参考现有周期性服务（如节点上报 backends、MCP registry sync），新增一个 discovery service：

- 负责调用 Ollama HTTP API
- 产出 `Vec<AiBackendConfig>` 或更底层的 `Vec<BackendInstanceSpec>`
- 持有 `Arc<RwLock<DiscoveredBackends>>`

### 2）Registry 合并与热更新

当前 [builder.rs](../src/spearlet/execution/ai/router/builder.rs) 在启动时一次性构建 `BackendRegistry`。

要满足“serving models 随运行态变化”的需求，推荐引入 `RegistryHandle`：

- `ArcSwap<BackendRegistry>` 或 `RwLock<BackendRegistry>`
- router 在每次选择候选时读取 handle 的当前值

更新路径：

1. 静态配置 backends → 构建静态 instances
2. discovery backends → 构建动态 instances
3. 合并（按 name 去重 + 冲突策略）
4. 生成新 registry 并原子替换 handle

### 3）与节点上报/ Web Admin 的联动

节点上报 backends（方案 A）应上报“合并后的 registry 视图”，这样 Web Admin 的 Backends tab 会自然显示：

- 静态配置 backends
- Ollama 导入 backends

并且 `status_reason` 可以表达：

- `ollama: unreachable`
- `ollama: filtered by denylist`
- `ollama: conflict name, skipped`

### 4）安全策略（SSRF 防护）

默认：

- `allow_remote=false`
- 仅允许 `base_url` host 为 `localhost/127.0.0.1/[::1]`

当 `allow_remote=true` 时：

- 仍建议做 CIDR deny（例如拒绝 link-local、metadata IP）
- 限制 scheme 为 `http/https`

## 可观测性

建议新增指标：

- `ollama_discovery_refresh_total{result=success|error}`
- `ollama_discovery_models_imported`（gauge）
- `ollama_discovery_last_success_timestamp`
- `ollama_discovery_snapshot_hash`（用于排障）

日志（结构化）：

- refresh start/end + duration
- error 分类（connect/timeout/parse)
- 导入/删除的模型数量与样例

## 测试计划

- 配置解析测试：新增 `spearlet.ai.discovery.ollama` 能正确解析；unknown 字段应失败。
- 单元测试（sanitize/allowlist/denylist/conflict）：导入结果稳定可预测。
- 集成测试：mock 一个 Ollama `/api/ps`，验证 registry 会随返回值变化而更新。
- Web Admin：`GET /admin/api/backends` 能看到导入项（由节点上报快照驱动）。

## 分期落地建议

- Phase 0：只在启动时导入一次（`refresh_interval_secs=0`），先闭环“展示 + 路由可用”。
- Phase 1：周期刷新 + registry 热更新。
- Phase 2：支持 installed models（`/api/tags`）与更丰富的 capability（embeddings 等）。
