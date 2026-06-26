# AI Credentials / credential_ref 设计与落地方案（详细）

本文给出一套可直接落地到本仓库的设计：在 `spearlet.ai` 配置中引入 `credentials`（集中管理凭据），并让每个 backend 通过 `credential_ref` 引用凭据，从而支持：

- 同一 provider（例如 OpenAI）下，不同 backend 使用不同 API Key（chat / realtime / embeddings 分开）
- 多个 backend 复用同一凭据（避免重复配置）
- 不在配置文件中直接写 key（只引用 env var / secret），便于轮换与审计

设计覆盖：schema、解析（TOML→struct）、registry 选择/过滤逻辑、文档与测试补齐。

## 0. 现状与问题

### 0.1 现状

当前 `spearlet/config.rs`：

- `AiConfig` 包含 `credentials` 与 `backends`，见 [config.rs](../../src/spearlet/config.rs)
- `AiBackendConfig` **不再支持** `api_key_env`；API key 通过 `credential_ref` 引用凭据（可选）
- `AiBackendConfig.hosting` 为必填，只允许 `local` 或 `remote`
- 已启用 `deny_unknown_fields`，配置里出现 `api_key_env` 会导致解析失败

当前 registry 构建逻辑：

- `credential_ref` 为可选：
  - 若配置了 `credential_ref`（非空）：解析出 credential 对应的 `api_key_env`，并在运行时环境缺失/为空时过滤该 backend（优先 `RuntimeConfig.global_environment`，再回退到进程环境变量）
  - 若未配置 `credential_ref`：视为“无需鉴权”（不会附加 API key header）

### 0.2 核心痛点

- 当你有多个 OpenAI backend（例如 chat 和 realtime ASR），希望各自使用不同的 key：
  - 需要一个“集中管理、复用、校验、轮换”的抽象层，避免在每个 backend 上重复配置
- `RuntimeConfig.global_environment` 在默认构建路径中通常是空（例如 `FunctionServiceImpl::new` 默认用 `HashMap::new()`），见 [function_service.rs](../../src/spearlet/function_service.rs#L85-L95)
- 导致实际部署时常出现“配置写了 credential_ref，但 runtime 没注入 env”的断层

## 1. 目标与非目标

### 1.1 目标

- 引入 `spearlet.ai.credentials`：集中定义凭据（至少包含 `api_key_env`）
- backend 引用凭据：`spearlet.ai.backends[].credential_ref = "..."`
- 移除旧字段：`backends[].api_key_env` 不再支持（配置出现即解析失败）
- 在 registry 构建时做校验与过滤：引用不存在的 credential / env 缺失时，backend 不进入 registry，并输出清晰错误
- 提供一套明确的“运行时注入 global_environment 的最佳实践实现路径”

### 1.2 非目标（v1）

- 不在配置文件中直接存储明文 key（可以允许但不推荐；v1 不实现 `api_key_inline`）
- 不实现 secret store（Vault/KMS/K8s secret）拉取（只预留扩展点）
- 不实现 key 轮询/负载均衡（key-ring），只实现“一 backend → 一 credential 引用”

## 2. Schema 设计（TOML + Rust）

### 2.1 TOML 结构

新增：

```toml
[spearlet.ai]
default_policy = "weighted_random"

[[spearlet.ai.credentials]]
name = "openai_chat"
kind = "env"                # v1 固定为 env
api_key_env = "OPENAI_CHAT_API_KEY"

[[spearlet.ai.credentials]]
name = "openai_realtime"
kind = "env"
api_key_env = "OPENAI_REALTIME_API_KEY"

[[spearlet.ai.backends]]
name = "openai-chat"
kind = "openai_chat_completion"
base_url = "https://api.openai.com/v1"
hosting = "remote"
credential_ref = "openai_chat"
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
credential_ref = "openai_realtime"
ops = ["speech_to_text"]
transports = ["websocket"]
weight = 100
priority = 0
```

### 2.2 Rust struct 变更（src/spearlet/config.rs）

对现有结构扩展：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AiConfig {
    pub default_policy: Option<String>,
    pub credentials: Vec<AiCredentialConfig>,
    pub backends: Vec<AiBackendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiCredentialConfig {
    pub name: String,
    pub kind: String,                 // v1: "env"
    pub api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiBackendConfig {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub hosting: Option<String>,
    pub model: Option<String>,
    pub credential_ref: Option<String>,
    pub weight: u32,
    pub priority: i32,
    pub ops: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
}
```

默认值：

- `AiConfig.credentials` 默认空数组
- `AiCredentialConfig.kind` 默认 `"env"`

### 2.3 兼容性（破坏性变更）

- `backends[].api_key_env` 已移除，不再支持。
- `spearlet.ai` 相关 struct 已启用 `deny_unknown_fields`：配置文件里出现 `api_key_env` 会直接解析失败。
- `hosting` 为必填，且仅允许 `local|remote`（启动期会直接报错）。
- `credential_ref` 为可选：仅当配置了 `credential_ref`（非空）时才要求对应 credential/env 存在；未配置则视为“无需鉴权”。

## 3. 凭据解析与校验（解析层）

### 3.1 TOML → struct

解析依赖 serde + toml，现有 `AppConfig::load_with_cli` 直接 `toml::from_str`，因此只要 struct 增字段即可解析，不需要手写 parser。

建议在 `SpearletConfig` 加载后做一次“LLM 配置归一化与校验”（不必在 toml 解析阶段强制失败）。

### 3.2 归一化：构建 Credential 索引

新增一个纯函数（建议位置：`src/spearlet/config.rs` 或 `src/spearlet/execution/ai/router/builder.rs` 附近的 helper）：

```text
fn build_credential_index(cfg: &AiConfig) -> HashMap<String, AiCredentialConfig>
```

校验规则：

- `credentials[].name` 必须非空
- `credentials[].name` 必须唯一
- `credentials[].kind` v1 必须是 `env`
- `credentials[].api_key_env` 必须非空

处理方式（best practice）：

- 发现错误：记录错误并跳过该 credential（或直接让整个 LLM 部分无效，取决于你们启动策略）

### 3.3 backend → credential 解析

新增解析函数（建议位置：registry 构建处）：

```text
fn resolve_backend_api_key_env(
  backend: &AiBackendConfig,
  cred_index: &HashMap<String, AiCredentialConfig>
) -> Result<String, ResolveError>
```

返回值语义：

- `Ok(env_name)`：该 backend 最终使用的 env var 名（由 `credential_ref` 解析得到）
- `Err(...)`：配置错误（credential_ref 缺失/不存在等）

## 4. RuntimeConfig.global_environment 注入策略（平台 best practice）

### 4.1 问题

在沙箱/隔离运行时（例如 WASM hostcall）里，`RuntimeConfig.global_environment` 通常为空（见 [function_service.rs](../../src/spearlet/function_service.rs)），这会导致（当 backend 配置了 `credential_ref` 时）：

- registry 过滤掉需要 key 的 backend
- 或者 streaming websocket header 模板无法展开 `${env:...}`

当前查找顺序：

- 优先从 `RuntimeConfig.global_environment` 读取（适用于沙箱运行时）
- 若未命中则回退到进程环境变量（适用于非沙箱运行时）

### 4.2 v1 推荐实现：按需从 OS env 收集

新增一个“LLM 环境变量白名单收集器”，只收集 LLM 相关且被引用到的 env：

```text
fn collect_required_env_vars(cfg: &SpearletConfig) -> HashSet<String>
```

收集来源：

- `ai.credentials[].api_key_env`
- `ai.backends[].credential_ref` 所引用到的 credential

在 runtime config 初始化阶段（建议位置：构造 RuntimeConfig 的地方，如 FunctionServiceImpl::new 或 RuntimeFactory 初始化路径）执行：

- `for env_name in required_envs { if let Ok(v) = std::env::var(&env_name) { global_environment.insert(env_name, v) } }`

安全性：

- 不要把所有 `std::env::vars()` 都塞进 `global_environment`
- 只按配置引用的 key 名精确读取，避免泄露其它敏感变量

### 4.3 未来扩展：secret store

保留 `credentials[].kind` 作为扩展点：

- `env`：从 OS env 读取
- `file`：从文件读取（K8s secret mount）
- `kms/vault`：运行时拉取（需要异步与缓存策略）

## 5. Registry 构建逻辑改造（execution/ai/router/builder.rs）

### 5.1 变更点

当前 registry 构建时只看 backend.credential_ref（并通过它解析到 env）。

改造后流程：

1) `cred_index = build_credential_index(cfg.ai.credentials)`
2) 遍历 backends：
   - 解析 `ops`（保持不变）
   - 解析 `api_key_env_resolved = resolve_backend_api_key_env(backend, cred_index)`
   - 若缺失 `credential_ref` / 引用不存在：backend 不注册
   - 若 `runtime_config.global_environment` 不含该 env：backend 不注册
   - 构造 adapter：
- openai_chat_completion：从 `global_environment[api_key_env_resolved]` 取值并传入 api_key
     - openai_realtime_ws：传入 `api_key_env_resolved`（adapter 内会把它放进 ws headers 模板）

### 5.2 错误处理策略

best practice（更利于运维）：

- 配置错误（credential_ref 缺失/未找到等）：backend 不注册，并打印结构化 warn/error（包含 backend.name）
- env 缺失：backend 不注册，并打印 warn（包含 env_name 与 backend.name）

## 6. Backend adapter 行为约束

### 6.1 openai_chat_completion

目前 adapter 直接持有 api_key（由 registry 从 `global_environment` 中解析得到），见 [openai_chat_completion.rs](../../src/spearlet/execution/ai/backends/openai_chat_completion.rs)。

改造后：registry 负责完成 env name → api_key 的解析，adapter 只消费 api_key。

### 6.2 openai_realtime_ws

目前 adapter 生成 ws plan 时写入 header 模板：`Bearer ${env:...}`，见 [openai_realtime_ws.rs](../../src/spearlet/execution/ai/backends/openai_realtime_ws.rs#L72-L85)。

改造后：只需把 resolved 的 env name 传进去。

## 7. 文档更新清单（补齐文档）

### 7.1 配置示例

- 更新 `config/spearlet/config.toml`：补充 `[spearlet.ai]`、`credentials` 与 `credential_ref` 示例

### 7.2 backend adapter 文档

- 在 `docs/backend-adapter/backends-zh.md` 增加一节 “凭据配置与 credential_ref”
- 指向本文（设计文档）

### 7.3 realtime asr 文档

- 在 `docs/implementation/realtime-asr-implementation-zh.md` 的 backend 配置说明里补充：
  - realtime ws backend 可用 `credential_ref` 指定不同 key

## 8. 测试计划（补齐测试）

### 8.1 配置解析测试（spearlet/config_test.rs）

新增用例：

- TOML 含 `[[spearlet.ai.credentials]]` 与 `[[spearlet.ai.backends]]`，能成功解析
- `credential_ref` 引用不存在：解析成功，但在 registry 构建时 backend 被过滤并产生错误
- `[[spearlet.ai.backends]]` 中出现 `api_key_env`：解析失败（deny_unknown_fields）

### 8.2 registry 构建测试（execution/ai/router/builder.rs 或 host_api/tests.rs）

新增用例：

- 给两条 backend 分别引用不同 credential，且 `RuntimeConfig.global_environment` 提供两个 env 值：两条 backend 都被注册
- 缺少其中一个 env：仅对应 backend 被过滤

### 8.3 realtime ws header 模板测试（host_api/tests.rs）

已有 websocket 相关测试可以扩展：

- 当 backend 使用 `credential_ref`（解析后 api_key_env 为 `OPENAI_REALTIME_API_KEY`），ws plan 里的 header 模板应为 `${env:OPENAI_REALTIME_API_KEY}`

## 9. 迁移方案

### 9.1 v1 兼容策略

- 不再兼容 `backends[].api_key_env`：配置里出现该字段会直接解析失败
- 迁移方式：把 key 的 env var 名放到 `credentials[].api_key_env`，backend 改为 `credential_ref`

### 9.2 提示与文档

- 文档示例统一切换到 `credentials + credential_ref`
- `api_key_env` 仅作为 credential 的字段存在（`credentials[].api_key_env`）

## 10. 代码落地点（实现 checklist）

- `src/spearlet/config.rs`
  - 扩展 `AiConfig`，新增 `AiCredentialConfig` 与 `credential_ref`
- `src/spearlet/execution/ai/router/builder.rs`
  - 引入 credential index
  - backend→env 解析与冲突校验
  - env 缺失过滤逻辑改为使用 resolved env
- `src/spearlet/function_service.rs` / runtime 初始化路径
  - 从 OS env 按需收集 required env，填充 `RuntimeConfig.global_environment`
- 文档：
  - `config/spearlet/config.toml`
  - `docs/backend-adapter/backends-zh.md`
  - `docs/implementation/realtime-asr-implementation-zh.md`
- 测试：
  - `src/spearlet/config_test.rs`
  - `src/spearlet/execution/host_api/tests.rs`
