# Spearlet gRPC 服务清理说明

## 范围

本次清理针对 spearlet 的 gRPC 服务注册路径：

- `src/spearlet/grpc_server.rs`
- `src/spearlet/object_service.rs`
- `src/spearlet/function_service.rs`
- `src/spearlet/instance_service.rs`
- `src/spearlet/grpc_server_test.rs`

## 架构调整

在本次清理前，`GrpcServer` 通过 `Arc<T>` 持有各个 service implementation，并且代码里还维护了额外的 tonic trait 实现，用于支持：

- `Arc<ObjectServiceImpl>`
- `Arc<FunctionServiceImpl>`
- `Arc<InstanceServiceImpl>`

这些转发 impl 本身并不承载业务价值，它们只是为了适配 `GrpcServer` 当时的注册方式而存在。

本次清理后：

- `GrpcServer` 直接持有可 clone 的 service implementation
- tonic server 直接注册具体 service 类型
- 低价值的 `Arc<T>` 转发 service impl 被删除
- `HealthService` 默认直接接收服务对象，但为了兼容测试与辅助代码，仍通过轻量转换支持 `Arc<T>` 输入

## 收益

- 减少了 spearlet 服务注册路径中的额外间接层
- 让 `GrpcServer` 更容易阅读，因为它现在持有的是真正的服务对象，而不是一层包装指针
- 删除了只负责转发调用的 trait 样板代码
- 在通过 `Clone` 保持廉价共享的同时，让所有权与职责边界更清晰

## 验证

- `cargo test grpc_server --lib`
- `cargo test http_gateway --lib`
- 已对以下文件执行定向诊断：
  - `src/spearlet/grpc_server.rs`
  - `src/spearlet/function_service.rs`
  - `src/spearlet/instance_service.rs`

`object_service.rs` 中仍有一个 rust-analyzer 的已知假阳性提示，但实际编译和测试均未复现对应问题。
