# 仓库类型模型重构路线图

## 目的

本文档用于沉淀一条仓库级的重构路线，聚焦当前在 `sms`、`spearlet` 与 `web-admin` 中反复出现的一类设计问题：

- 稀疏的“bag of options”结构体
- 依赖魔法字符串的 `metadata` / `config` key
- 将多种语义变体混装在一个扁平 DTO 中
- 对 `provider`、`hosting`、`origin` 等字段做重复字符串归一化

目标不是一次性重写所有代码，而是建立一条可分阶段、低风险落地的迁移路径，让代码边界更强类型、编排函数更短、长期维护成本更低。

## 为什么需要做

这类问题已经同时出现在多层：

- 配置加载
- Web Admin 写入模型
- SMS admin DTO
- runtime backend 装配
- local / remote preflight 请求与响应
- Web Admin 中的 task 配置映射

当同一种弱模型形状出现在多层时，每一层都会被迫补偿：

- 额外的校验分支
- 重复的字符串解析
- 重复的字段映射
- 更多 `Option<T>` 或 `undefined` 判空
- 手工保持表单字段与 metadata JSON 的同步

这会提高维护成本，也会让一层与另一层逐渐漂移。

## 问题定义

### 1. 一个结构同时承载多种变体

当前仍有多个文件使用一个大结构或大接口去表示本应显式区分的多个变体。

代表性文件：

- `src/spearlet/config.rs`
- `src/spearlet/ai/backend_assembly.rs`
- `src/sms/web_admin/ai_backend_admin.rs`
- `src/spearlet/http_gateway/backend_preflight_handlers.rs`
- `web-admin/src/api/ai-backends.ts`
- `web-admin/src/api/types.ts`

### 2. 魔法字符串跨层传播

有些领域概念并没有显式类型，而是仅作为 `metadata` 或 `config` map 里的字符串 key 存在。

代表性例子：

- `model_url`
- `model_path`
- `skip_download`
- `download_timeout_s`
- `mcp.enabled`
- `mcp.default_server_ids`
- `mcp.tool_allowlist`

这会迫使 UI、SMS 和 runtime 手工维持同一套隐式 schema。

### 3. 字符串字段在表达领域变体

像下面这些字段，目前仍在多个地方被重复解析或解释：

- `provider`
- `hosting`
- `origin`
- `desired_state`
- `backend_kind`

这让逻辑分散，也让编译器无法帮助我们尽早发现非法状态。

### 4. API envelope 重复表达状态

当前有些 admin 响应同时编码了：

- `success`
- `message`
- 可选 payload
- 额外的 ad hoc 标志位

这会让调用方需要检查多个字段，才能理解同一个结果状态。

## 重构原则

- 用 tagged enum 或 discriminated union 表达变体。
- 字符串解析只留在序列化边界。
- 优先使用显式 typed metadata 对象，而不是自由形态 JSON map。
- 将 identity、desired spec、observed status、transport DTO 分层建模。
- orchestration 代码尽量短小，字段映射放到专门的 mapper 中。
- 每个新的 typed mapper 或 enum 分支都必须配套聚焦测试。

## 优先区域

### Priority 1. Backend 配置与 runtime 装配

核心文件：

- `src/spearlet/config.rs`
- `src/spearlet/ai/backend_assembly.rs`

原因：

- 这里是多条弱模型问题向下游扩散的源头
- 先在这里收敛，能够减少后续大量字符串归一化与特判

### Priority 2. Web Admin 写入模型与 SMS 入口 DTO

核心文件：

- `src/sms/web_admin/ai_backend_admin.rs`
- `web-admin/src/features/ai-backends/AiBackendEditorForm.ts`
- `web-admin/src/features/ai-backends/AiBackendEditorDialog.tsx`

原因：

- 这里是 metadata 魔法字符串被放大的入口
- 这里直接决定用户看到和提交的 API 契约

### Priority 3. 共享 API 类型与 Admin Envelope

核心文件：

- `src/sms/ai_admin_api.rs`
- `web-admin/src/api/ai-backends.ts`
- `web-admin/src/api/types.ts`

原因：

- 这些文件决定了复杂度如何继续向页面组件和测试扩散

### Priority 4. Task 配置映射

核心文件：

- `web-admin/src/features/tasks/TasksPage.tsx`

原因：

- MCP 配置路径和 AI backend metadata 存在同一种魔法字符串问题

## 阶段计划

## Phase 1. 引入 canonical enum 与显式变体边界

### 目标

停止在多个层级重复解析同一批领域字符串。

### 计划工作

- 为以下概念引入 canonical enum 或 typed wrapper：
  - `provider`
  - `hosting`
  - `origin`
  - `desired_state`
  - `backend_kind`
- 字符串转换只保留在 API / config 序列化边界。
- 让内部 mapper 消费 typed value，而不是裸字符串。

### 当前进度

Phase 1 的第一批改动已经落地。

目前已完成：

- 新增共享的 `src/ai_backend_types.rs` 辅助模块
- 为以下字段引入 canonical typed parsing：
  - `provider`
  - `hosting`
  - `backend_kind`
- 将同一套共享 typed helper 扩展到：
  - `origin`
  - `desired_state`
  - `management_mode`
- 更新 `backend_assembly`，通过共享 typed helper 完成 provider 推断与 kind/hosting 规范化
- 更新 SMS admin input mapping，在创建/更新映射和 preflight 路径中复用 typed selector
- 更新 backend validator，复用 typed kind 到 hosting 的规则，而不是继续维护裸字符串匹配列表
- 更新 admin desired-state / management-mode 解析，以及 proto 到 domain 的枚举转换路径，统一复用共享 typed helper

### 验收标准

- 新代码主要基于 enum 分支，而不是重复字符串比较。
- 非法组合能更早被拒绝。
- 外部边界行为保持不变。

## Phase 2. 用 typed variant 替代稀疏 backend config bag

### 目标

将 local 与 remote backend 配置拆成显式变体。

### 计划工作

- 将 `src/spearlet/config.rs` 中弱类型的大配置包替换为显式变体，例如：
  - `LocalBackendConfig`
  - `RemoteBackendConfig`
- 在需要时补充 provider-specific typed sub-config。
- 将校验规则内聚到构造器或 typed conversion helper 中。

### 当前进度

Phase 2 的第一批切口已经落地，但暂时还没有改动外部配置文件的形状。

目前已完成：

- 在 `src/spearlet/config.rs` 中引入 `AiBackendTypedView`
- 新增 `AiBackendConfig::typed_view()`，从当前稀疏配置包派生出内部强类型视图
- 更新 `validate_spearlet_config()`，通过 typed view 做 backend 配置校验，而不是继续直接写裸字符串判断
- 更新 `backend_assembly`，在把配置转换为 `BackendSpec` 时优先消费 typed config view
- 增加针对 alias 规范化与 `local + ollama_chat` 合法路径的聚焦测试
- 将 typed config view 从单一平面对象继续演进为显式的 `Local` / `Remote` 变体，并保留共享 common fields
- 更新 `backend_assembly`，通过独立的 local/remote 转换 helper 消费新的 config variant
- 将最小变体约束真正下沉到 typed conversion 本身，包括：
  - remote backend 必须提供 `base_url`
  - local 的 endpoint-style backend 必须提供 `base_url`
  - 托管型 `local + llamacpp` 仍允许不提供 `base_url`
  - 非法的 kind/hosting 组合由 typed config 层直接拒绝
- 在 `Local / Remote` 变体之上继续引入 provider-specific 子视图：
  - `LocalBackendProviderView`
  - `RemoteBackendProviderView`
  - `AiBackendProviderView`
- 更新 typed config 构造逻辑，显式拒绝 `provider` 与 `kind` 语义不一致的配置，而不是继续容忍同一 backend 在两套字符串语义之间漂移
- 更新 local 变体校验逻辑，通过 provider-specific 子视图表达 endpoint-style local provider 的最小约束
- 更新 `backend_assembly`，先通过 provider-specific 分类 helper 进入 local/remote 的更细分转换路径，为后续 provider-specific 装配规则留出稳定边界
- 更新 `remote_preflight`，通过 typed provider family selector 对 `openai_compatible` / `ollama` 做统一分类，而不是继续直接返回魔法字符串
- 更新 `backend_assembly` 的 runtime adapter / capability 选择逻辑，通过 typed runtime family selector 统一消费 backend kind，而不是继续分散匹配字符串常量
- 更新 `local_models/provider` 与 `backend_assignment_controller`，通过共享 canonical helper 统一解析 local assignment driver，而不是继续手写本地 alias 表
- 更新 `backend_assignment_controller`，将 local assignment reconcile 入口收敛到 `LocalAssignmentRuntimeFamily`，让控制器分支开始直接消费 typed local runtime family
- 更新 `backend_assignment_controller` 的本地 provider-specific metadata 读取，先收敛为 `LlamaCppAssignmentParams` / `VllmAssignmentParams`，减少分支内部直接消费裸 `HashMap<String, String>`
- 更新 `local_models/llamacpp`，将 `server_mode / server_cmd / server_cmd_args / threads / ctx_size / ready_probe / skip_download / model_url` 收敛为 `LlamaCppLaunchOptions` 与 `LlamaCppModelSource`
- 更新 `local_models/vllm`，将 external endpoint 解析收敛为带 `base_url/source` 的 typed 结果，并让 `backend_assignment_controller` 直接消费该结果
- 更新节点侧 `local_model_handlers`，将 local-model preflight 的 provider 选择与 llama.cpp 参数拼装收敛为 typed provider family 与 typed params builder
- 更新节点侧 `local_model_handlers` 与 `sms/web_admin/ai_backend_admin` 的 local-model preflight DTO，将扁平 `source_kind/final_url/...` 响应收敛为 nested typed source variant
- 更新节点侧 `backend_preflight_handlers` 与 `sms/web_admin/ai_backend_admin` 的 remote preflight DTO，将扁平 `phase/error_code/checks/...` 响应收敛为 nested typed result variant
- 更新 remote preflight DTO，将通用 result 进一步细化为 provider-specific family（`open_ai_compatible / ollama / unknown`）加 typed outcome 的两层结构
- 继续细化 known remote provider family 的 success outcome：
  - `OpenAI-compatible` 收敛为 `ConnectivityVerified / AuthVerified / ModelAccessVerified`
  - `Ollama` 收敛为 `ConnectivityVerified / ModelAccessVerified`
  - `Unknown` 暂时保留 generic `Success / Failure`，避免在未知 provider 上过早引入伪精确语义
- 继续细化 known remote provider family 的 failure outcome：
  - `OpenAI-compatible` 收敛为 `ConnectivityFailed / AuthFailed / ModelAccessFailed`
  - `Ollama` 收敛为 `ConnectivityFailed / ModelAccessFailed`
  - 失败变体直接承载稳定的 provider-specific 布尔语义，而不是继续依赖 `phase + Option<bool>` 拼装
- 更新 `sms/web_admin/ai_backend_admin` 的 AI backend 写入边界：
  - 对 `llamacpp / vllm` 已知 metadata 字段先解析成 provider-specific typed input model
  - 引入与 repo 现有 `config typed view` 风格一致的 `AiBackendWriteTypedView + provider_view`，让 `create/update/preflight` 共享同一层 local/remote 与 provider-specific 解析
  - 为 local provider 新增显式的 request discriminated union：`local.provider_family + config`，并让 `llamacpp / vllm` 优先消费这层结构化输入
  - 为 remote provider 新增显式的 request discriminated union：`remote.provider_family + config`，并让 `OpenAI-compatible / Ollama` 优先消费这层结构化输入来解析有效的 `base_url / credential_ref / operations / features / transports`
  - 当结构化 `local/remote` 输入存在时，拒绝重复出现的 legacy `metadata / spec / credential_ref` 语义来源，避免双源配置继续扩散
  - 对已完成结构化建模的 known provider，进一步收窄为 required structured input：`llamacpp / vllm` 必须提供 `local`，`OpenAI-compatible / Ollama` 必须提供 `remote`
  - 再统一渲染为规范化 metadata JSON，避免 create/update/preflight 各自手写魔法字符串读取
  - 允许 admin 输入使用更自然的 JSON `bool/number`，并在边界层归一化为当前运行链路可消费的稳定字符串形态
- 同步对齐 Web Admin 前端序列化层到同一份请求契约：
  - `web-admin/src/api/ai-backends.ts` 现在为 known provider 暴露 typed `local / remote` 写入输入
  - `AiBackendEditorForm.buildPayload()` 现在直接输出 known provider 的结构化 payload，而不是继续把语义塞进 legacy 顶层 `metadata / spec / credential_ref`
  - 新增聚焦 Vitest 覆盖，锁定 local `llamacpp`、remote `OpenAI-compatible` 的结构化 payload 形态，并拒绝 unsupported provider，而不是继续保留过渡期 payload
- 补充 remote preflight 聚焦测试，覆盖 `generic outcome -> provider-specific outcome` 转换，以及 admin 端到端 preflight JSON shape
- 继续扩展 admin preflight 集成覆盖，让 HTTP 边界也能在代理到节点前拒绝 known-provider 非法写入：
  - `llamacpp` 的 legacy-only local 输入会在 `/admin/api/ai-backends/preflight` 被直接拒绝
  - `OpenAI-compatible` 的 legacy-only remote 输入会在 `/admin/api/ai-backends/preflight` 被直接拒绝
  - `OpenAI-compatible` 的 structured + legacy 重复 remote 语义来源也会在同一 endpoint 被拒绝
- 继续扩展 admin create/update 集成覆盖，让写入 endpoint 也执行同样的 typed boundary 约束：
  - `/admin/api/ai-backends` 会拒绝 `llamacpp` 的 legacy-only local 输入
  - `/admin/api/ai-backends` 会拒绝 `OpenAI-compatible` 的 structured + legacy 重复 remote 语义来源
  - `/admin/api/ai-backends/:backend_id` 会拒绝 `OpenAI-compatible` 的 legacy-only remote 更新
- 继续把 admin 读侧对齐到同一份 typed provider 契约：
  - `AdminAiBackendRecordResponse` 现在会为 known provider 暴露结构化 `local / remote` 读视图
  - Web Admin `formFromBackend()` 现在会优先使用 typed 读字段回填 editor state，而不是继续依赖 legacy-only `metadata / spec / credential_ref`
  - 新增聚焦 Rust 与 Vitest 覆盖，锁定响应 shape 与编辑器回填路径
  - `AiBackendDetailPage` 现在也会优先从 typed 读字段解析 summary 与 provider-config 展示，并补了 known local / remote provider 的 helper 覆盖
  - admin mutation、list、detail 集成测试现在都会验证 known provider 的 typed `local / remote` 读响应
  - known provider 的前端读路径现在不再静默混用 typed 与 legacy 字段
  - known provider 的 editor metadata textarea 不会再把 legacy key 反向同步回 typed 表单字段；metadata 编辑现在只保留给额外的、未建模为 typed config 的记录数据
- 清理掉剩余的 admin 兼容写入路径：
  - admin create 与 preflight 现在会直接拒绝 `anthropic-compatible` 这类 unsupported provider，而不是继续保留 legacy 过渡 payload
  - Web Admin editor 现在会在提交前校验并拒绝 unsupported provider
  - local `llamacpp` 字段编辑不再向原始 metadata JSON 双写；typed config 成为单一语义源，metadata 仅在保存时做规范化渲染
- 同步清理相关的 SMS 路由结构测试，保证这批重构护栏既快速又易维护：
  - `src/sms/routes_test.rs` 现在通过共享 probe helper 统一路由存在性断言，而不是继续散落重复的请求构造与状态判断
  - 对于只需要验证路由命中的 POST / PUT 探测，请求体改为非法 JSON，让它们在 Axum extractor 层快速失败，避免意外依赖 lazy gRPC 的连接失败慢路径
  - 在保持 route-matched 断言语义不变的前提下，这组测试恢复为毫秒级执行，而不是等待下游超时链路

### 验收标准

- runtime config 不再依赖一大组互不相关的可选字段。
- 校验逻辑更短、更局部。
- 测试覆盖合法与非法配置组合。

## Phase 3. 为 AI Backend 表单引入 typed metadata 模型

### 目标

把 Web Admin 与 SMS 写入链路中的 metadata 魔法字符串移除掉。

### 计划工作

- 为 local backend metadata 引入显式 typed model，例如：
  - `LlamaCppLocalModelSource`
  - `LlamaCppRuntimeOptions`
- 用 typed form state 替代当前扁平字段与 metadata 字符串的双向镜像。
- 如果仍需要保留原始 JSON 编辑，只将其作为清晰的 advanced mode。
- 将 metadata 到 wire shape 的转换集中到一个 mapper 模块。

### 验收标准

- `AiBackendEditorForm` 不再手工同步多个字段与一个字符串化 JSON blob。
- SMS 请求映射消费 typed metadata，而不是分散的 magic key。
- 新增一个 metadata 字段时，只需要修改一条 mapper 路径，而不是多个组件。

## Phase 4. 统一 Admin API 结果形状

### 目标

降低响应歧义，去掉重复状态编码。

### 计划工作

- 统一 admin endpoint 的 response envelope。
- 对像 preflight 这类场景保留 typed payload variant。
- 在一个 typed result model 足够表达时，减少 `success + optional payload + extra flags` 这种组合。

### 验收标准

- 客户端能够通过一致的结构理解结果状态。
- 测试更简单，不再需要覆盖大量 null 组合。
- API DTO 将公共头字段与变体细节清晰分层。

## Phase 5. 按层拆分前端 API 类型

### 目标

避免一个 TypeScript 文件同时承载 summary、detail、write、runtime、diagnostic 多层语义。

### 计划工作

- 将 `web-admin/src/api/ai-backends.ts` 拆成按层聚焦的类型模块。
- 将 `web-admin/src/api/types.ts` 拆成更小的 domain-focused 文件。
- 在确实需要多个变体时保留 discriminated union。

### 验收标准

- 页面组件导入的类型更窄。
- summary 与 detail 模型不会悄悄漂移。
- feature page 中的防御式 null 检查明显减少。

## Phase 6. 将 Task 配置从字符串 key map 中抽离

### 目标

把同样的 typed-config 纪律应用到 task 侧 MCP 配置。

### 计划工作

- 在 `TasksPage` 中引入显式 MCP form model。
- 将 `mcp.*` 的 wire mapping 抽到专门的 conversion helper。
- 将当前庞大的 task creation form 拆成更小、更聚焦的模块。

### 验收标准

- MCP 配置在序列化前由 typed object 表示。
- 新增 MCP 配置项时，不再需要在页面组件里分散更新字符串 key。
- task 创建 UI 更容易单独测试。

## 实施策略

- 优先从扇出最高的源头文件开始。
- 先引入附加型 typed wrapper，再逐步删除旧字段。
- 每次按一个垂直切片推进：
  - type
  - mapper
  - caller
  - tests
  - docs
- 避免长期保留两条同等级的“canonical”路径并存。

## 推荐的前三个工作项

1. 引入 typed backend enum，并在 backend assembly 与 admin input mapping 中使用。
2. 抽出 typed `llamacpp` metadata model，移除 Web Admin 表单里基于原始 key 的同步逻辑。
3. 将 `AiBackendConfig` 拆成显式的 local / remote 变体，并把校验下沉到 typed constructor。

## 非目标

- 一次性重写所有 DTO
- 移除仓库中所有 optional 字段
- 在没有明确契约变更决策的前提下修改外部 API 兼容性
- 仅因为代码在同一文件中，就顺手重构无关业务逻辑

## 测试要求

- 每个阶段都必须补充或更新聚焦测试。
- 当一个 wire shape 要映射到 typed internal model 时，必须有 mapper 测试。
- 集成测试需要确认 typed refactor 没有改变既有 runtime 行为。
- 每个完成的阶段都必须同步更新中英文文档。

## 成功标准

当出现以下结果时，这条路线图算成功：

- 新增 backend 或 provider 特性时，需要修改的分散文件更少
- UI、SMS 和 runtime 之间共享的 magic string 明显减少
- 内部代码主要基于 enum 或 variant 分支，而不是重复字符串判断
- API 与表单模型更容易阅读，不需要先扫过一大包 optional 字段
- 测试更局部、更少防御式判空
