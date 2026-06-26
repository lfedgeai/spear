# Ollama 模型自动导入（Discovery）

本文档说明 SPEARlet 如何从 Ollama 自动导入模型，并在 Web Admin 的 AI Models（Local）页面展示。

## 背景

SPEARlet 的后端路由与执行使用 `spearlet.ai.backends`。为了让本地 Ollama 的模型可以像其它后端一样参与路由与观测，引入了 Ollama discovery：在 SPEARlet 启动时，从 Ollama API 拉取模型列表，并生成对应的 backend 条目。

## 行为概述

- 导入发生在 SPEARlet 启动阶段；会把模型映射成 `kind = "ollama_chat"` 的后端。
- 导入结果会写入运行时配置（内存），并参与 backend 上报与 Web Admin 展示。
- Backend 可用性目前只做“配置/环境”层面的检查（例如 OpenAI 会检查 api_key_env 是否存在）；不会主动探测 Ollama 网络连通性。

## 配置

配置段位于：`[spearlet.ai.discovery.ollama]`。

关键字段：

- `enabled`：是否开启导入。
- `scope`：导入范围：
  - `serving`：调用 `/api/ps`，仅导入“当前正在运行/正在被加载”的模型。
  - `installed`：调用 `/api/tags`，导入“本机已安装”的模型。
- `base_url`：Ollama 服务地址（默认 `http://127.0.0.1:11434`）。
- `allow_remote`：是否允许非 loopback 地址。开启会有 SSRF 风险，默认 `false`。
- `allow_models` / `deny_models`：模型名精确匹配白/黑名单。
- `max_models`：最多导入多少个模型。
- `name_prefix`：导入后的 backend 名称前缀（默认 `ollama/`）。
- `name_conflict`：命名冲突策略：`skip|overwrite`。

示例：

```toml
[spearlet.ai.discovery.ollama]
enabled = true
scope = "installed"
base_url = "http://127.0.0.1:11434"
allow_remote = false
timeout_ms = 1500
max_models = 32
allow_models = []
deny_models = []
name_prefix = "ollama/"
name_conflict = "skip"
default_weight = 100
default_priority = 0
default_ops = ["chat_completions"]
default_features = []
default_transports = ["http"]
```

## 导入后的 backend 形态

每个模型会被导入为一个 backend：

- `kind = "ollama_chat"`
- `base_url = <ollama base_url>`
- `hosting = "local"`
- `model = "<model_name>"`（固定绑定模型）
- `credential_ref = null`（不需要密钥）

当启用“按模型路由”时，guest 只需要设置 `model = "<model_name>"`，无需显式指定 `backend` 名称。

## Web Admin 查看

- AI Models（Local）页面会展示聚合后的模型（provider=ollama）。
- 点击某一行进入详情页，可查看该模型在各节点上的实例分布与 Raw JSON（用于排查路由能力、节点分布等）。

## 常见问题

### 开了 enabled 但没有导入任何 backend

最常见原因是 `scope` 不匹配：

- 你只有 `ollama pull`，但没有实际运行推理：`/api/ps` 可能为空，此时应使用 `scope = "installed"`。

### Web Admin 里显示 available，但调用失败

可用性不做网络探测；需要检查：

- Ollama 是否在对应 `base_url` 监听
- SPEARlet 进程是否可访问该地址
