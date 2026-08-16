# Backend Support Matrix

本文档描述 `spear` 当前代码里**已经落地**的 backend 能力矩阵。

目标是把三件事分开说明：

- IR / 协议层定义过哪些 operation
- 运行时内建 adapter 真正支持哪些 operation
- Web Admin 当前允许创建哪些 remote backend

这份矩阵只描述**当前已实现行为**，不把设计稿、历史预留字段或未来计划视为“已支持”。

## 设计原则

- `kind` 表示 backend adapter 类型，例如 `openai_chat_completion`
- `operations` 表示该 backend 对外声明的能力，例如 `chat_completions`
- backend 能否参与路由，不只取决于 `operations` 是否能解析，还取决于该 `kind` 是否真的实现了这些 operation
- 当前装配层已经对 `kind` 和 `operations` 做了一致性校验；不匹配的 backend 不会注册进运行时

## 当前内建支持矩阵

| Kind | Hosting | Transport | 当前支持的 operations | 说明 |
|---|---|---|---|---|
| `openai_chat_completion` | remote | `http` | `chat_completions` | 标准 OpenAI Chat Completions 风格 HTTP adapter |
| `ollama_chat` | local / node-local | `http` | `chat_completions` | Ollama 本地聊天 adapter |
| `openai_realtime_ws` | remote | `websocket` | `speech_to_text` | 当前只支持流式 ASR，不支持通用 realtime voice agent |
| `stub` | local / internal | in-process | `chat_completions` | 仅用于测试 / 开发，不是面向生产的 backend |

## Web Admin 当前创建能力

当前 Web Admin 已拆分为两条创建链路：

- `Create Remote Backend`
- `Create Local Backend`

### Remote

当前 remote 创建流支持的 provider / kind 组合：

| Hosting | Provider | Backend Kind | 默认 / 常见 operations | 说明 |
|---|---|---|---|---|
| `remote` | `openai` | `openai_chat_completion` | `chat_completions` | 标准 OpenAI HTTP chat backend |
| `remote` | `openai` | `openai_realtime_ws` | `speech_to_text` | 当前 realtime websocket ASR backend |
| `remote` | `ollama` | `ollama_chat` | `chat_completions` | 指向外部 Ollama-compatible endpoint |

### Local

当前 local 创建流支持的 provider / kind 组合：

| Hosting | Provider | Backend Kind | 当前能力 | 说明 |
|---|---|---|---|---|
| `local` | `llamacpp` | `llamacpp` | `chat_completions` | 节点本地 `llama-server` 管理模式 |
| `local` | `vllm` | `vllm` | scaffolded / external-endpoint oriented | 当前更偏脚手架与 external endpoint 场景，不是完整托管进程模式 |

也就是说，当前 Web Admin 不再只暴露 remote backend，而是已经把 local / remote 都纳入统一创建入口，并附带 placement 配置。

### Local llama.cpp 显式字段

对于 `local + llamacpp`，Web Admin 当前已显式暴露常用 runtime 参数，并在保存时自动写回 metadata：

- `model_url`
- `model_path`
- `skip_download`
- `download_timeout_s`
- `threads`
- `ctx_size`

## IR / 协议里已出现但当前未落地的 operation

下列 operation 仍可能存在于部分 IR、调试或 filter 协议代码里，但**当前没有任何内建 backend kind 对它们提供完整落地支持**：

- `embeddings`
- `image_generation`
- `text_to_speech`
- `realtime_voice`

其中：

- `realtime_voice` 当前已经不再接受为 `BackendSpec.operations` 的合法声明值
- 其余几个 operation 仍保留在 IR / 协议枚举里，主要是为了兼容已有抽象层与未来扩展，但当前没有内建 adapter 可直接执行

因此，判断“是否支持”时应以**运行时 adapter + kind-operation 校验**为准，而不是只看枚举里有没有这个名字。

## 当前推荐用法

- 如果要创建标准文本聊天 backend，使用：
  - `kind = openai_chat_completion`
  - `operations = ["chat_completions"]`
- 如果要创建 realtime ASR backend，使用：
  - `kind = openai_realtime_ws`
  - `operations = ["speech_to_text"]`

## 不推荐的配置

以下配置在当前代码里不应再被视为有效运行时配置：

- `openai_realtime_ws` + `realtime`
- `openai_realtime_ws` + `realtime_voice`
- `openai_chat_completion` + `speech_to_text`
- 任意 `kind` 声明一个该 adapter 并未实现的 operation

## 后续扩展建议

如果后续要正式支持新的 operation，建议按以下顺序推进：

1. 先落地对应 backend adapter 的实际实现
2. 再在装配层把该 operation 纳入该 `kind` 的支持矩阵
3. 最后再把它暴露到 Web Admin 的可选项中

不要反过来先把 operation 暴露给用户，否则很容易出现“UI 可选但运行时不可用”的伪能力。

## 相关当前文档

- [web-admin-overview-zh.md](./web-admin-overview-zh.md) 说明当前 Web Admin 页面与 API 范围
- [web-admin-ui-guide-zh.md](./web-admin-ui-guide-zh.md) 说明实际操作流程
- [ai-backend-unified-control-plane-design-zh.md](./ai-backend-unified-control-plane-design-zh.md) 说明 backend 标识、placement 与 status 背后的控制面模型
