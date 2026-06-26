# Backend 编译裁剪、注册发现与配置

本文件聚焦“已编译启用的 backend 如何被发现并参与路由”。

## 1. 编译裁剪（Cargo features）

建议每个 backend 一个 feature，并将重依赖设为 `optional`：

- `backend-openai`（OpenAI-compatible HTTP）
- `backend-azure-openai`
- `backend-vllm`
- `backend-openai-realtime`（WebSocket）
- `backend-stub`

注册表构建通过 `#[cfg(feature = "backend-xxx")]` 注册对应 backend；未启用 feature 的 backend 不参与编译与链接。

## 2. BackendKind 与 BackendInstance

建议区分两层：

- `BackendKind`：实现类型（openai_chat_completion/azure/vllm/realtime...）
- `BackendInstance`：具体实例（base_url、region、权重、优先级、capabilities、limits）

路由选择对象是 instance。

## 3. Registry 与 CapabilityIndex

- `BackendRegistry`：持有所有启用的 instances、其 capabilities、权重、健康状态句柄
- `CapabilityIndex`：从 registry 派生索引（如 `Operation -> candidates[]`）

legacy 对齐：`GetAPIEndpointInfo` 通过 env key 是否存在过滤 endpoint（`legacy/spearlet/core/models.go`），新设计把这一逻辑收敛到 registry 构建与 discovery。

## 4. Discovery（发现接口）

这里的“discovery”是指“对外暴露当前进程内 registry 的可观测视图”，不是指 backend 之间必须通过网络去互相发现。

- 进程内：router/adapter 直接通过函数调用读取 `BackendRegistry`（这是默认路径，不需要任何 HTTP/gRPC）。
- 对外：可选提供 HTTP/gRPC 端点，用于运维/调试/UI/自动化检查，查看“已编译启用 + 已配置 + 当前健康”的 backend 与 capabilities。

建议提供两类：

1) 控制面（HTTP/gRPC）
- `GET /api/v1/backends`
- `GET /api/v1/capabilities`

2) 任务侧自适应（可选）
- 在 hostcall control 命令中提供 `GET_CAPABILITIES`，返回 JSON

## 5. 配置模型（示例）

示例（与当前 `spearlet` 配置结构一致）：

```toml
[spearlet.ai]
default_policy = "weighted_random"

[[spearlet.ai.credentials]]
name = "openai_chat"
kind = "env"
api_key_env = "OPENAI_CHAT_API_KEY"

[[spearlet.ai.credentials]]
name = "openai_realtime"
kind = "env"
api_key_env = "OPENAI_REALTIME_API_KEY"

[[spearlet.ai.backends]]
name = "openai-us"
kind = "openai_chat_completion"
base_url = "https://api.openai.com/v1"
hosting = "remote"
model = "gpt-4o-mini"
credential_ref = "openai_chat"
weight = 80
priority = 10
ops = ["chat_completions", "text_to_speech"]
features = ["supports_stream", "supports_tools", "supports_json_schema"]
transports = ["http"]

[[spearlet.ai.backends]]
name = "openai-realtime"
kind = "openai_realtime_ws"
base_url = "https://api.openai.com/v1"
hosting = "remote"
credential_ref = "openai_realtime"
weight = 100
priority = 20
ops = ["speech_to_text"]
features = []
transports = ["websocket"]
```

### 5.1 model 绑定与“按模型路由”

当某些 backend 实例配置了 `model`（例如本机 Ollama 导入的每模型一个 backend），路由可以仅通过请求中的 `model` 来选择实例：

- 若候选集中存在任何 `backend.model != None`，则可进一步用 `backend.model == request.model` 过滤候选
- 这样 guest 不必显式指定 `backend` 名称，只需设置 `model`

建议通过 `spearlet.ai.credentials[]` 集中管理 API key，并让 `spearlet.ai.backends[].credential_ref` 引用凭据，以支持不同 backend 使用不同 key。

hosting 约定：

- 每个 `[[spearlet.ai.backends]]` 都必须配置 `hosting`，且仅允许 `local` 或 `remote`。

详细设计与落地方案见：[llm-credentials-implementation-zh.md](../implementation/llm-credentials-implementation-zh.md)

## 6. Secret 与网络策略

- `ai.credentials[].api_key_env`、`ai.backends[].credential_ref` 与 base_url 必须由 host 配置提供；WASM 不可注入。
- backend allowlist/denylist 由 host 配置控制，请求侧只能收缩。

### 6.1 API key 的存储方式（建议）

建议只在配置里保存“环境变量名”，不在配置文件中保存明文 key。

- 在 `[[spearlet.ai.credentials]]` 中使用 `api_key_env = "OPENAI_API_KEY"`
- 在 `[[spearlet.ai.backends]]` 中使用 `credential_ref = "<credential_name>"`
- 在 spearlet 进程启动环境中注入 `OPENAI_API_KEY=...`

这样可以：

- 避免 key 进入仓库、配置分发链路与日志
- 便于按实例/按节点做差异化配置与 key 轮换

### 6.2 API key 的读取与使用（host-side）

在当前 Rust 代码中：

- 根据 `credential_ref` 解析出 `api_key_env`
- 在 spearlet 进程启动环境中注入该 env，并在运行时加载到 `RuntimeConfig.global_environment`
- registry 使用解析到的 key 构造 backend adapter
- adapter 将其组装到 HTTP Header（例如 `Authorization: Bearer <key>`）
- 禁止打印/回传 key（包括错误日志与 `raw` 字段）

### 6.3 缺失 key 的行为（当前行为）

- 若配置了 `credential_ref`（非空）但 credential 不存在或对应环境变量不存在：
  - 视为该 backend instance 不可用（从 candidates 里过滤）
- 若未配置 `credential_ref`：
  - 视为“无需鉴权”（不会附加 API key header），适用于自建 OpenAI-compatible 代理等场景
- discovery 对外输出时：
  - 可以仅输出 `credential_ref`（以及其解析到的 env var 名称），不输出值

### 6.4 轮换与多 key

- 轮换：通过更新 spearlet 进程环境变量并滚动重启实现（MVP）；后续可加入热更新机制。
- 多 key：允许为不同 backend instance 配置不同 `credential_ref`（从而使用不同 env var）。

### 6.5 多 API key 的组织最佳实践

#### 6.5.1 命名与映射

- 按“提供方/区域/用途/实例”命名环境变量，避免复用同一个 key 覆盖多个实例：
  - 例如：`OPENAI_API_KEY_US_PRIMARY`、`OPENAI_API_KEY_US_FALLBACK`、`AZURE_OPENAI_KEY_EASTUS`、`VLLM_TOKEN_CLUSTER_A`
- 配置里只引用 env 名称：每个 `BackendInstance` 绑定一个 `credential_ref`，做到可追踪、可轮换、可审计。

#### 6.5.2 多 key 用于同一个 backend instance（key pool）

当同一个 endpoint 需要多个 key（配额拆分、限流分摊、灰度/AB）时，建议引入 “key pool” 的抽象（后续增强）：

- 配置（建议）：`credential_refs = ["openai_key_us_primary", "openai_key_us_2", ...]`
- 选择策略（按场景）：
  - `round_robin`：均匀摊分 QPS
  - `random`：实现简单
  - `least_errors`：对单 key 的封禁/失效更鲁棒（需要错误计数）
- 失败回退：遇到 `401/403/429` 时按策略切换 key 并进行短期熔断（避免打爆同一个 key）

MVP 可以先实现“一个 instance 一个 key”；key pool 建议作为 Phase 4+ 的增强。

#### 6.5.3 分权与最小权限

- 不同提供方/不同 project/不同权限域使用不同 key，不同 backend 不共用 key。
- 将 key 的用途与操作绑定（例如某些 key 只允许 `embeddings`），通过 router 的 allowlist/requirements 限制路由范围。

#### 6.5.4 性能与工程性

- 不要在每次请求都做昂贵的 secret 解析（如调用外部 secret manager）；优先在进程内缓存已解析的 key。
- 建议在初始化阶段完成 key 解析并缓存（前提是你接受“滚动重启生效”的轮换方式）。

#### 6.5.5 部署建议（Kubernetes）

- 使用 K8s Secret 注入 env（`envFrom`/`valueFrom.secretKeyRef`），并限制 RBAC。
- discovery/API 返回只暴露 `credential_ref`（或 env var 名称），不暴露值。

### 6.6 与 SMS Web Admin 的配合（建议）

SMS Web Admin 可以支持“API key 配置组件”，但最佳实践是把它做成“secret 引用管理”，而不是直接在 UI 里录入/存储明文 key。

推荐形态：

- Web Admin 管理的是：
  - backend instance 的配置（`base_url`、权重、能力、`credential_ref` 等）
  - secret 的引用（`credentials[]` 名称与其 env var 名称，或外部 secret manager 的引用 ID）
- Web Admin 不管理的是：
  - 明文 key 的值（不进入 SMS DB，不进入日志，不通过 API 回传）

与 spearlet 的配合方式：

- spearlet 进程启动时通过部署系统注入环境变量（K8s Secret/Vault Agent/systemd drop-in 等）
- spearlet 进程启动时加载 env 并构建 registry，用解析到的 key 进行请求签名
- SMS Web Admin 可以提供“校验/可观测”：
  - 仅验证 key 是否“存在/可用”（例如让 spearlet 在心跳 `health_info` 上报 `HAS_ENV:OPENAI_API_KEY_US_PRIMARY=true`）
  - 允许在 UI 上标记某个 instance 在某些 node 上缺失 key，但不展示 key 值

是否这是一个好的 key 组织方式：

- 是的（推荐）：UI 管理“映射与引用”，部署系统管理“secret 值”。这是最小权限、可审计、可轮换且安全边界清晰的拆分。
- 不推荐（除非有完备安全体系）：让 Web Admin 直接存储明文 key。除非你已经具备 KMS 加密、审计日志、细粒度 RBAC、密钥轮换与泄露应急流程，否则这会把 SMS 变成高风险 secret 仓库。
