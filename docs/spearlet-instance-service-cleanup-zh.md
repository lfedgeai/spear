# Spearlet Instance Service 清理说明

## 范围

本次清理针对以下文件中的实例级命令路径：

- `src/spearlet/instance_service.rs`

## 架构调整

在本次清理前，`instance_service.rs` 虽然文件不大，但 handler 内部仍然直接混合了：

- execution manager 访问
- 可选字符串归一化
- 内部错误转换
- destroy 之后的 task 补偿式 reconcile

这会让它看起来更像一个直接透传层，而不是一个边界清晰的实例级编排服务。

本次清理后，围绕这些职责提炼出了更清晰的内部结构：

- `execution_manager()` 统一访问 execution manager 依赖
- `optional_non_empty()` 与 `task_filter()` 统一处理请求层的可选字符串归一化
- `map_internal_error()` 统一处理内部错误转换
- `reconcile_task_after_destroy()` 显式表达 destroy 之后的补偿步骤

调整后：

- `destroy_instance()` 更像一个实例级命令工作流
- `reconcile_task_assignments_now()` 更像一个单独的编排命令
- 文件自身更能表达它作为 instance service 边界的职责

## 收益

- 在不过度设计一个小服务的前提下提升了可读性
- 让实例级编排步骤更容易被识别
- 让归一化与错误处理规则保持一致
- 让该服务与 spearlet 其它模块正在形成的“薄 endpoint + 聚焦 helper”方向保持一致

## 验证

- `cargo test instance_service --lib`
- `cargo test grpc_server --lib`
- `cargo test http_gateway --lib`

新增了两个聚焦测试，用于覆盖：

- 可选字符串归一化
- task filter 归一化
