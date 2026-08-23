# Spearlet Function Service 清理说明

## 范围

本次清理针对以下文件中的 execution / invocation 可读性与内部职责结构：

- `src/spearlet/function_service.rs`

## 架构调整

在本次清理前，`function_service.rs` 的 gRPC handler 内部直接混合了多类职责：

- invoke 请求归一化
- execution 到 proto 的映射
- payload 组装
- execution 错误映射
- terminate 错误转换

这会让 handler 变得偏长，也更难快速扫出真正的业务动作，因为传输层映射细节和业务控制流交织在一起。

本次清理后，围绕这些职责提炼出了更薄的内部结构：

- `normalize_invoke_request()` 统一处理默认值与请求归一化
- `payload_with_content_type()` 统一处理输出 payload 组装
- `execution_error_to_proto()` 统一处理 execution 错误投影
- `invoke_response_from_execution()` 负责从执行结果构造 invoke 响应
- `execution_to_proto()` 负责统一构造 execution 响应
- `map_terminate_execution_error()` 统一处理 terminate 错误映射

调整后：

- `invoke`、`get_execution`、`list_executions`、`terminate_execution` 更专注于控制流
- 传输层映射规则收敛到小而清晰的 helper
- 不同 endpoint 之间的 execution 响应语义更加一致

## 收益

- 提升了 `function_service.rs` 命令路径的可读性
- 减少了重复的 execution/proto 映射代码
- 让未来继续增加 execution 相关接口时更容易保持一致
- 保持改动轻量、局部，不引入不必要的额外抽象层

## 验证

- `cargo test function_service --lib`
- `cargo test grpc_server --lib`
- `cargo test http_gateway --lib`

新增了两个聚焦测试，用于覆盖：

- invoke 请求默认值归一化
- 未请求输出时 execution 输出数据会被隐藏
