# Credential Control Plane 设计

本文档描述如何在当前 `spear` 架构基础上，为 AI backend 的 `credential_ref` 引入可运行时增删改查、可同步、可热更新的 credential control plane。

目标：

- 保持 `BackendSpec` 中只出现 `credential_ref`，不存明文 secret
- 复用现有 SMS `snapshot + revision + watch` 架构
- 复用 spearlet 侧 `list + watch + periodic refresh` 同步模式
- 支持 runtime 新增 / 更新 / 删除 key
- adapter 在请求时读取最新 credential，不要求重启 spearlet
- 尽量少引入新抽象，避免无用复杂度

---

## 1. 当前问题

当前 `credential_ref` 的真实语义是：

```text
BackendSpec.credential_ref
-> SpearletConfig.ai.credentials[].name
-> api_key_env
-> std::env::var(api_key_env)
```

这套方案的问题：

- 依赖进程环境变量，运行时更新不友好
- Docker Compose / 容器环境下动态加 key 需要重启或变通
- credential 生命周期和 backend 生命周期被耦合
- Web Admin 只能填 ref，不能管理 credential 本身
- backend 构建时静态解析 secret，不利于 key rotation

因此需要把 `credential_ref` 从“启动时配置解析”升级为“运行时可解析引用”。

---

## 2. 设计原则

### 2.1 继续保留 `credential_ref`

`BackendSpec` 中继续只保留：

- `credential_ref: String`

不在 backend spec 中直接存：

- 明文 API key
- env 名
- file path

这样可以保持 backend spec 是“资源引用”，而不是“secret carrier”。

### 2.2 复用现有控制面模式

优先复用以下已有模式：

- SMS:
  - `AdminBackendsState`
  - `RegistryWatchHub`
  - `KvStore`
- spearlet:
  - `RemoteBackendSyncService`
  - `DynamicBackendRegistry`

credential control plane 不应引入一套完全不同的同步模型。

### 2.3 adapter 在调用时解析 credential

不要在 adapter 构建时把 key 解析成静态值。

应改为：

- adapter 持有 `credential_ref`
- adapter 在发请求 / 建 websocket 前再从本地 credential store 读取当前值

这样 runtime 更新 key 后可以自然生效。

### 2.4 第一阶段先做最小可用能力

第一阶段只做：

- SMS 内部加密存储 credential
- Web Admin CRUD
- spearlet watch 同步
- adapter 运行时按 ref 取值

暂不做：

- Vault / KMS / 外部 secret provider
- 复杂 RBAC
- node placement
- per-node credential capability matrix
- 自动轮换策略

---

## 3. 整体架构

```text
Web Admin
  -> AdminCredentialService (SMS)
       -> AdminCredentialsState
            -> KV snapshot
            -> revision
            -> watch hub

Spearlet
  -> CredentialSyncService
       -> list credentials
       -> watch credentials
       -> periodic refresh
       -> DynamicCredentialStore

AI Adapter
  -> credential_ref
  -> CredentialProvider
  -> resolve current key at request/connect time
```

---

## 4. SMS 侧设计

### 4.1 新增状态模块：`AdminCredentialsState`

建议新增：

- `src/sms/admin_credentials.rs`

结构尽量对齐 `src/sms/admin_backends.rs`。

核心结构：

```rust
pub struct AdminCredentialsState {
    kv: Arc<dyn KvStore>,
    snapshot: RwLock<CredentialSnapshot>,
    watch: RegistryWatchHub<CredentialEvent>,
    cipher: Option<Arc<dyn SecretCipher>>,
}
```

说明：

- SMS 启动时允许暂时没有 `SMS_CREDENTIAL_MASTER_KEY`
- 当未配置 master key 时，credential 元信息列表仍可读取
- 只有涉及 secret 明文材料的操作，例如 `upsert` / `list_materials`，才返回明确的 `FailedPrecondition`
- 这样可以避免无关功能被 credential 加密配置硬阻塞，同时保持 secret 管理操作必须显式配置密钥

### 4.2 存储模型

```rust
struct CredentialRecord {
    name: String,
    provider_kind: String,
    encrypted_secret: Vec<u8>,
    version: u64,
    description: String,
    disabled: bool,
    created_at_ms: i64,
    updated_at_ms: i64,
}
```

```rust
struct CredentialSnapshot {
    revision: u64,
    credentials: Vec<CredentialRecord>,
}
```

说明：

- `name` 是控制面唯一引用名
- `provider_kind` 第一阶段建议固定为 `inline_encrypted`
- `encrypted_secret` 存加密后的 secret，不存明文
- `version` 用于单条 credential 的版本号
- `revision` 用于整个 credential registry 的同步版本

### 4.3 加密抽象

第一阶段建议新增简单抽象：

```rust
pub trait SecretCipher: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, SmsError>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<String, SmsError>;
}
```

推荐实现：

- `AesGcmSecretCipher`

密钥来源：

- SMS 进程环境变量中的 master key
- 当前实现允许 SMS 在未配置该变量时先启动，但真正管理 credential secret 时必须配置后再执行

这样做的好处：

- 比明文落 KV 安全得多
- 不需要一开始就引入外部 KMS
- 后续可平滑升级为 KMS-backed cipher

### 4.4 CRUD 行为

需要提供：

- `list()`
- `upsert(name, secret, description, disabled)`
- `delete(name)`
- `watch_credentials(since_revision)`

与 `AdminBackendsState` 一样：

- 修改后 revision 自增
- 写回 snapshot 到 KV
- 通过 `RegistryWatchHub` 推送增量事件

### 4.5 删除与引用关系

第一阶段建议：

- 删除时若仍被 backend 引用，直接拒绝删除

原因：

- 行为简单
- 避免 dangling `credential_ref`
- 易于理解

可在 SMS 里复用现有 admin backends snapshot 做引用检查。

---

## 5. proto / gRPC API 设计

建议新增独立服务，而不是塞进 `AdminAiConfigService`。

推荐：

- `AdminCredentialService`

proto 文件建议：

- `proto/sms/admin_credentials.proto`

### 5.1 消息定义

```proto
message CredentialInfo {
  string name = 1;
  string provider_kind = 2;
  uint64 version = 3;
  string description = 4;
  bool disabled = 5;
  int64 created_at_ms = 6;
  int64 updated_at_ms = 7;
}
```

说明：

- 不在 `CredentialInfo` 中返回 secret

### 5.2 管理接口

```proto
rpc ListCredentials(ListCredentialsRequest) returns (ListCredentialsResponse);
rpc UpsertCredential(UpsertCredentialRequest) returns (UpsertCredentialResponse);
rpc DeleteCredential(DeleteCredentialRequest) returns (DeleteCredentialResponse);
rpc WatchCredentials(WatchCredentialsRequest) returns (stream WatchCredentialsResponse);
```

### 5.3 spearlet 同步接口

第一阶段可直接复用上面的 list/watch。

如果后续需要做权限隔离，再拆成：

- Admin API
- Node sync API

当前先避免过早拆分。

---

## 6. Web Admin 设计

### 6.1 单独页面

建议新增独立页面，而不是把 credential CRUD 塞到 backend 弹窗里：

- `CredentialsListPage`
- `CredentialCreateDialog`
- `CredentialUpdateDialog`

当前实现状态：

- 已提供独立 Web Admin 路由：`/ai-models/credentials`
- Remote AI Models 页面已改为跳转到独立 credentials 页面

### 6.2 页面功能

支持：

- 列表
- 新建
- 更新 secret
- 更新描述
- 启用 / 禁用
- 删除
- 查看引用 backend 数量

当前实现状态：

- 已支持列表、搜索、状态筛选和最近更新时间展示
- 已支持创建 dialog
- 已支持编辑 dialog
- 编辑时 secret 可留空，表示保留当前密文 secret，仅更新描述或 disabled 状态
- 已支持删除确认

### 6.3 安全行为

必须遵循：

- 列表页不展示明文 secret
- 编辑时不能回显原 secret
- 更新 secret 采用覆盖式输入
- UI 可以展示 masked 提示，例如：
  - `configured`
  - `updated 2h ago`

### 6.4 backend 表单联动

`CreateRemoteBackendDialog` 中的 `credential_ref` 应从自由输入改为选择器：

- 下拉或 searchable select
- 数据来自 `/admin/api/credentials`

理由：

- `credential_ref` 本质上是资源引用
- 不应让用户自由拼写

---

## 7. spearlet 侧设计

### 7.1 新增 `DynamicCredentialStore`

建议新增：

- `src/spearlet/ai/dynamic_credential_store.rs`

结构参考 `DynamicBackendRegistry`，但不共享同一个 registry。

推荐结构：

```rust
pub struct DynamicCredentialStore {
    inner: Arc<RwLock<HashMap<String, CredentialValue>>>,
    revision: Arc<AtomicU64>,
}
```

```rust
pub struct CredentialValue {
    secret: secrecy::SecretString,
    version: u64,
    updated_at_ms: i64,
    disabled: bool,
}
```

### 7.2 新增 `CredentialSyncService`

建议新增：

- `src/spearlet/ai/credential_sync.rs`

结构尽量对齐 `RemoteBackendSyncService`：

- 启动先 `list`
- 再 `watch(since_revision)`
- watch 断线自动重连
- 周期 `list` refresh
- 应用到 `DynamicCredentialStore`

### 7.3 为什么复用这个模式

优点：

- 符合现有架构
- 容易理解
- 不需要引入新同步框架
- 节点控制面资源都可以遵循相同范式

---

## 8. adapter 运行时解析设计

### 8.1 当前问题

当前 backend 装配阶段就会尝试解析 credential。

这会导致：

- credential 尚未同步到本地时 backend 可能直接不可用
- runtime key rotation 需要重建 backend

### 8.2 推荐改法

新增抽象：

```rust
pub trait CredentialProvider: Send + Sync {
    fn resolve_api_key(&self, credential_ref: &str) -> Option<secrecy::SecretString>;
}
```

由 `DynamicCredentialStore` 提供实现。

### 8.3 adapter 保存什么

不再保存静态 key，改为保存：

- `credential_ref: Option<String>`
- `credential_provider: Arc<dyn CredentialProvider>`

### 8.4 调用时解析

- `openai_chat_completion`
  - 在请求发起前解析 api key
- `openai_realtime_ws`
  - 在建立 websocket 连接前解析 api key

### 8.5 行为建议

如果配置了 `credential_ref` 但本地尚未同步到 secret：

- backend 不要直接从 registry 消失
- 实际调用时返回清晰错误：
  - `credential_missing`
  - `credential_disabled`
  - `credential_not_synced`

当前实现状态：

- adapter 现在会区分 `credential_missing`、`credential_disabled`、`credential_not_synced`
- `backend_reporter` 已复用同一套 credential 解析逻辑，避免节点上报状态与真实调用语义不一致

这是比“backend 直接不注册”更稳定、更符合最终一致控制面的行为。

---

## 9. 与现有 backend sync 的关系

credential sync 和 backend sync 是两条独立同步流：

- backend sync:
  - 负责“有哪些 backend”
- credential sync:
  - 负责“credential_ref 当前能否解析到 secret”

两者分开有几个好处：

- secret 是敏感资源，权限边界独立
- backend 变更与 credential 变更频率不同
- 代码职责更清晰

但是两者应尽量复用同一同步模式：

- list
- watch
- revision
- periodic refresh

---

## 10. 状态与可观测性

第一阶段建议先做最小化可观测性，不引入过多逻辑。

### 10.1 spearlet 本地状态

可以暴露：

- `credential_sync_revision`
- `credential_count`
- `last_success_at_ms`
- `last_error`
- `watch_connected`

当前实现状态：

- Spearlet 已提供本地只读接口：`/monitoring/ai/credentials`
- 当前返回字段包括 `status`、`started`、`watch_connected`、`applied_revision`、`local_store_epoch`、`credential_count`、`last_success_at_ms`、`last_error`
- SMS Web Admin 已可通过 `/admin/api/nodes/{uuid}/ai/credentials` 代理查看单节点状态

### 10.2 backend 调用错误

如果调用时找不到 credential：

- 返回结构化错误
- 不打印 secret
- 日志中仅记录 ref name

### 10.3 Web Admin 页面

credential 列表页可展示：

- 是否被引用
- 最近更新时间
- disabled 状态

当前实现状态：

- Credentials 页面已展示 `referenced_by_count`
- Node Detail 页面已新增 `Credential Sync` 卡片，用于展示节点本地同步状态

第一阶段暂不做：

- 节点维度 credential presence matrix
- node sync lag dashboard

---

## 11. 安全边界

### 必须做到

- SMS KV 中不存明文 secret
- Web Admin 列表 / API 不回传明文
- 日志不打印 secret
- debug/status 不回传 secret
- spearlet 本地只保存在内存

### 第一阶段可接受

- master key 来自 SMS 环境变量
- 本地内存态 `SecretString`

### 后续可扩展

- KMS-backed encryption
- 审计日志
- RBAC
- secret rotation history

---

## 12. 分阶段实施

### Phase 1：控制面最小闭环

- proto: credential CRUD + watch
- SMS: `AdminCredentialsState`
- SMS: encrypted KV storage
- spearlet: `DynamicCredentialStore`
- spearlet: `CredentialSyncService`
- adapter: runtime resolve by `credential_ref`

目标：

- 完整打通 runtime add/update/delete key 主链路

### Phase 2：Web Admin 产品化

- credentials 列表页面
- 创建/编辑/删除 dialog
- backend 表单改成 credential 选择器

当前实现状态：

- 已完成独立 credentials 页面
- 已完成创建/编辑/删除 dialog
- 已完成 remote backend 表单中的 credential 选择器

### Phase 3：可观测性与安全增强

- 引用关系查看
- 审计日志
- per-node sync 状态
- KMS / external secret provider

当前实现状态：

- 已完成引用关系查看的轻量版本：`referenced_by_count`
- 已完成 per-node sync 状态的最小可用版本：Node Detail 页面可查看节点本地 credential sync 状态
- 审计日志与 KMS / external secret provider 仍未实现

---

## 13. 不推荐的方案

以下方案不推荐作为正式方向：

### 13.1 继续依赖 env 热更新

问题：

- 容器环境变量不适合 runtime 更新
- Compose / systemd 都不优雅

### 13.2 backend spec 里直接存明文 key

问题：

- 控制面泄密风险高
- 审计和最小权限边界很差

### 13.3 adapter 构建时固化 key

问题：

- runtime add/update key 不自然
- rotation 需要重建 adapter 或重启进程

---

## 14. 推荐实施顺序

建议按下面顺序推进：

1. 新增 credential proto 与 SMS state
2. 做 spearlet 动态 credential store + sync
3. 改 adapter 为运行时解析
4. 做 Web Admin credentials 页面
5. 把 backend 表单里的 `credential_ref` 改为选择器

这条顺序能保证：

- 先打通后端基础设施
- 再补 UI
- 不会出现 UI 先开放但运行时还没准备好的状态

---

## 15. 一句话总结

最贴现有架构、最优雅也最可扩展的方案是：

**把 credential 做成和 remote backends 平行的一类控制面资源，由 SMS 用 snapshot + revision + watch 管理，spearlet 用 list + watch + refresh 同步到本地内存 store，adapter 在请求时按 `credential_ref` 动态取值。**
