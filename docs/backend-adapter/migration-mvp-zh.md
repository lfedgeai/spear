# 与 legacy 的映射与分阶段落地（MVP→扩展）

## 1. legacy → 新设计映射

### 1.1 TransformRegistry → Capability Router

legacy：按 `input_types/output_types/operations` 子集匹配选择 transform（`legacy/spearlet/hostcalls/transform.go`）。

新设计：

- `operations` 对齐为 `Operation`
- `input/output types` 对齐为 `modalities`/`Feature`
- “最具体 transform” 对齐为“过滤后按 policy 选择”

### 1.2 APIEndpointMap → BackendRegistry

legacy：硬编码 `APIEndpointMap`（`legacy/spearlet/core/models.go`），并通过 env key 过滤可用 endpoint。

新设计：

- 配置驱动的 backend instances
- Cargo feature 编译裁剪 + registry 构建
- discovery API 输出“已启用实例 + capabilities”

### 1.3 rt-asr → stream 子系统

legacy：WebSocket session + append audio + delta events（`legacy/spearlet/stream/rt_asr.go`）。

新设计：

- 规划项：`Operation::realtime_voice` + `Transport::websocket|grpc`（当前未实现）
- stream 子系统与 router/registry 共用 capabilities

## 2. MVP 分阶段

### Phase 1：Chat (最小闭环)

- `cchat_*` 保持 ABI
- Normalize：session → `CanonicalRequestEnvelope(operation=chat_completions)`
- Router：实现最小策略（`weighted_random`）
- Backend：先实现 `backend-stub`，再实现一个 OpenAI-compatible HTTP backend
- Discovery：提供 `GET /capabilities`

### Phase 2：Embeddings / Image / ASR / TTS（逐操作扩展）

- 增加对应 payload 骨架与 capabilities
- 复用同一 router/registry/policy
- 引入 `MediaRef`（图像/音频）并统一对象存储交互

### Phase 3：Realtime Voice / Streaming（规划项，当前未实现）

- 引入 stream 生命周期与事件模型
- 能力约束（bidi stream、transport、会话上限）
- 可靠性：并发控制、断线重连策略（必要时）

### Phase 4：高级策略与治理

- 熔断/剔除
- cost-aware routing
- hedged/mirror（按 operation 与预算控制）
