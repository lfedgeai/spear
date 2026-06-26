# CChat 模型选择与路由（实现说明）

本文档描述当前代码中 CChat（`cchat_*` hostcalls）如何通过 `model` 与可选的 backend 约束完成路由，以及如何排查“最终路由到了哪个 backend”。

## 1. 请求中的 model

在 CChat 中，`model` 来自会话参数：

- WASM guest 通过 `cchat_ctl(SET_PARAM)` 设置 `{"key":"model","value":"..."}`
- 或者 WASM-C 示例（`samples/wasm-c/chat_completion.c`）在编译期选择并调用 `sp_cchat_set_param_string(fd, "model", model)`

归一化逻辑会把 `model` 写入 `CanonicalRequestEnvelope.payload`，用于后续路由与 backend 调用。

## 2. backend.model 绑定（model-bound backend）

`[[spearlet.ai.backends]]` 支持可选字段：

```toml
[[spearlet.ai.backends]]
name = "openai-chat"
kind = "openai_chat_completion"
base_url = "https://api.openai.com/v1"
hosting = "remote"
model = "gpt-4o-mini"
...
```

语义：当某个 backend 配置了 `model`，它表示该 backend 与一个模型“绑定”。

路由实现要点：

- 先按 op/features/transports/routing allowlist/denylist 过滤候选 backends
- 如果请求里带 `model`，并且候选集中存在任何 `backend.model != None`：
  - 进一步只保留 `backend.model == request.model` 的候选
  - 若过滤后为空，会返回 `no_candidate_backend`，并列出当前可用的 `available_models`

这让你可以只通过 `model` 来“间接选择 backend”，而无需 guest 显式设置 `backend` 名称。

## 3. Ollama + gemma3 的路由方式

当开启 Ollama discovery 并导入 `gemma3:1b` 后：

- 请求只要设置 `model = "gemma3:1b"`
- 路由会优先匹配到 `backend.model = "gemma3:1b"` 的候选（即导入的 Ollama backend）

无需在 guest 侧显式声明 `backend`。

## 4. 如何确认最终路由到了哪个 backend

当前提供两种排查方式：

### 4.1 在响应 JSON 中查看 `_spear.backend`

`cchat_recv` 返回的 JSON 会在顶层附带：

```json
"_spear": {"backend": "...", "model": "..."}
```

WASM-C 示例会解析并打印：

- `debug_model=...`
- `debug_backend=...`

### 4.2 查看 Router 的 debug 日志

当启用 debug 日志时，Router 在选中 backend 后会输出一条 `router selected backend` 的 debug 日志，包含：

- `selected_backend` / `selected_model`
- 请求的 `op`、`model`、routing 限制与候选信息

## 5. 建议

- 如果希望一个 backend 支持多个 model（例如 OpenAI 多模型），不要为该 backend 配置 `model` 绑定；用 `backend`/allowlist/denylist 或其它策略进行选择。
- 如果你希望“按模型精确路由”（例如把某些模型固定到本机 Ollama），为对应 backend 配置 `model` 并让 guest 仅设置 `model` 即可。
