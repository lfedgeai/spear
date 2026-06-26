# AI Backends 配置说明

本文说明如何配置 `spearlet` 的 AI backends 与凭证（credentials）。

## 配置入口

配置通过 `SPEAR_CONFIG`（TOML）加载；在 Kubernetes 场景下由 Helm 渲染为 `config.toml`。

## Credentials（凭证）

在 `[[spearlet.ai.credentials]]` 下定义密钥来源，通过环境变量引用，避免在配置文件中保存明文密钥。

```toml
[[spearlet.ai.credentials]]
name = "openai_default"
kind = "env"
api_key_env = "OPENAI_API_KEY"
```

## Backends（后端）

每个 backend 配置在 `[[spearlet.ai.backends]]` 下。

必填字段：

- `name`：backend 名称（唯一）
- `kind`：backend 实现类型（字符串）
- `base_url`：服务地址（http(s)）
- `hosting`：必填，只允许 `local` 或 `remote`
- `ops`：支持的操作
- `transports`：支持的传输方式

可选字段：

- `model`：固定模型（部分 backend 支持）
- `credential_ref`：可选的密钥引用（见下文）
- `features`, `weight`, `priority`

示例：

```toml
[[spearlet.ai.backends]]
name = "openai-chat"
kind = "openai_chat_completion"
base_url = "https://api.openai.com/v1"
hosting = "remote"
model = "gpt-4o-mini"
credential_ref = "openai_default"
ops = ["chat_completions"]
features = ["supports_tools", "supports_json_schema"]
transports = ["http"]
weight = 100
priority = 0

[[spearlet.ai.backends]]
name = "openai-realtime-asr"
kind = "openai_realtime_ws"
base_url = "https://api.openai.com/v1"
hosting = "remote"
credential_ref = "openai_default"
ops = ["speech_to_text"]
transports = ["websocket"]
weight = 100
priority = 0
```

## `hosting` 语义

`hosting` 完全以配置为准，主要用于上报与展示（Web Admin / SMS），并让多环境部署更清晰。

- `local`：节点本地（本地进程或本地服务）
- `remote`：远端服务（SaaS 或远端集群）

## `credential_ref` 语义

`credential_ref` 为可选：

- 若配置了 `credential_ref`（非空）：
  - 必须存在同名 credential
  - 对应的 `api_key_env` 必须在运行时环境中存在且非空（优先 `RuntimeConfig.global_environment`，再回退到进程环境变量），否则该 backend 会被视为不可用并被过滤
- 若未配置 `credential_ref`：
  - 视为“无需鉴权”（不会附加 API key header），适用于自建 OpenAI-compatible 代理等场景

## 常见 backend kind

本仓库常见 kind：

- `openai_chat_completion`（HTTP）
- `openai_realtime_ws`（WebSocket）
- `ollama_chat`（HTTP，节点本地）
- `stub`（测试用）

## Managed（本地模型）backends

部分 backends 不来自 `config.toml`，而是由本地模型控制器（例如 Web Admin 的 Local AI Models）创建并持续 reconcile。

路由行为：

- 静态配置 backends 构成基础 registry。
- managed backends 会在路由时合并进入候选集合，用于表达某节点上已部署/可用的本地模型实例。

## SMS 托管的 remote backends

通过 SMS / Web Admin 创建的 remote backends，会被 `spearlet` 里的 `RemoteBackendSyncService` 持续 watch，并合并进与本地控制器共用的动态 backend registry。

当前运行时已支持的 SMS 动态 backend kind：

- `openai_chat_completion`
- `openai_realtime_ws`
- `ollama_chat`
- `stub`

说明：

- 这里的“创建”是指在 `spearlet` 内动态生成可路由 backend 条目，不负责真正拉起远端服务实例。
- 如果配置了 `credential_ref`，运行时仍然必须能成功解析对应凭据，否则该 backend 会被过滤为不可用。
- 节点 backend 上报只表达“节点自身事实”：`spearlet` 仅上报静态 backends 与 local-controller backends，不会把 SMS 下发的 remote backends 再回显进 SMS 的 node snapshot。
