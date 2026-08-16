# AI Legacy Cleanup Summary

## 目标

本次清理的目标是把 Spearlet 节点侧 AI backend 的 legacy 运行时路径彻底移除，
将节点侧 AI backend 生命周期统一收敛到 assignment controller。

## 已删除

以下 legacy 运行时路径与入口已删除：

- remote legacy 运行时
  - `src/spearlet/ai/remote_backend_sync.rs`
  - `src/spearlet/ai/remote_backend_policy.rs`
- 顶层 legacy 入口
  - `src/spearlet/legacy.rs`
- local legacy 运行时与兼容壳
  - `src/spearlet/local_models/controller.rs`
  - `src/spearlet/local_models/legacy_local.rs`
- 仅为 local legacy 服务的辅助模块
  - `src/spearlet/local_models/cutover.rs`
  - `src/spearlet/local_models/llamacpp_reconcile.rs`
  - `src/spearlet/local_models/managed_backends.rs`
  - `src/spearlet/local_models/provider_reconcile.rs`
  - `src/spearlet/local_models/reconcile.rs`
  - `src/spearlet/local_models/registry.rs`
  - `src/spearlet/local_models/runtime.rs`
  - `src/spearlet/local_models/status.rs`
  - `src/spearlet/local_models/vllm_reconcile.rs`

## 已移除的配置与语义

以下 legacy 配置/语义已移除：

- `legacy_remote_backend_sync_enabled`
- `legacy_local_model_controller_enabled`
- `SPEARLET_AI_LEGACY_REMOTE_BACKEND_SYNC_ENABLED`
- `SPEARLET_AI_LEGACY_LOCAL_MODEL_CONTROLLER_ENABLED`
- dynamic backend source 中的 `LocalController`
- monitoring 中 legacy registration / cutover / active legacy components 展示

当前节点侧 AI backend 生命周期已统一收口到 assignment controller。

## 当前统一路径

节点侧当前统一 AI backend 控制路径为：

- `src/spearlet/ai/backend_assignment_controller.rs`

仍保留且被 unified 路径使用的本地模块：

- `src/spearlet/local_models/provider.rs`
- `src/spearlet/local_models/vllm.rs`
- `src/spearlet/local_models/llamacpp.rs`

这些模块不再属于 legacy runtime，而是 unified 路径仍需复用的本地 provider 支撑件。

## 代码结构结果

清理完成后，代码结构上的结论是：

- `src/` 中已无 legacy AI runtime 引用
- 节点侧 AI backend 生命周期仅由 unified assignment controller 承担
- legacy 相关残留仅存在于历史设计文档和“已移除”说明中

## 验证

本次清理后已完成的关键验证：

- `cargo test spearlet::config::tests --lib`
- `cargo test spearlet::backend_reporter::tests --lib`
- `cargo test spearlet::http_gateway::monitoring_handlers::tests --lib`
- `cargo test apps::spearlet::main::tests --lib`
- `cargo check --bin spearlet`

另外已做源码全局搜索，确认 `src/` 中不再包含以下 legacy runtime 引用：

- `LocalModelController`
- `legacy_local_model_controller_enabled`
- `remote_backend_sync`
- `register_legacy`
- `build_legacy`
- `ManagedBackendRegistry`
- `global_managed_backends`

## 后续可选项

如果后续仍要继续“清历史”，可作为新任务处理：

- 协议层历史枚举/字段清理
  - 例如 `BackendOrigin::LocalController`
- 历史设计文档的进一步重写
  - 将历史方案与现行方案彻底拆开

## 破坏式切换说明

当前已接受破坏式切换：

- 不迁移旧 `admin_backends` 数据
- 不迁移旧 `model_deployments` 数据
- 升级后仅识别 unified `ai_backends` / `placements` / `statuses`

这意味着升级后如果仍有历史数据残留在旧 KV key 中，它们将不会被新控制面读取。
