# Documentation / 文档

This directory contains project documentation and examples for the spear-next project. These documents are created to facilitate knowledge transfer and help future developers understand and extend the project.

此目录包含为 spear-next 项目整理的文档与示例。这些文档旨在促进知识传递，帮助未来的开发者理解并扩展项目。

## Contents / 内容

### Current / 当前文档

- **[ai-backend-unified-control-plane-design-en.md](./ai-backend-unified-control-plane-design-en.md)** - English design for the unified AI backend control plane
- **[ai-backend-unified-control-plane-design-zh.md](./ai-backend-unified-control-plane-design-zh.md)** - 中文版统一 AI backend 控制面设计文档
- **[web-admin-overview-en.md](./web-admin-overview-en.md)** - Current Web Admin page and API overview / 当前 Web Admin 页面与 API 概览（英文）
- **[web-admin-overview-zh.md](./web-admin-overview-zh.md)** - 当前 Web Admin 页面与 API 概览
- **[web-admin-ui-guide-en.md](./web-admin-ui-guide-en.md)** - Current Web Admin usage guide / 当前 Web Admin 使用指南（英文）
- **[web-admin-ui-guide-zh.md](./web-admin-ui-guide-zh.md)** - 当前 Web Admin 使用指南
- **[backend-support-matrix-en.md](./backend-support-matrix-en.md)** - Current backend support matrix / 当前 backend 能力矩阵（英文）
- **[backend-support-matrix-zh.md](./backend-support-matrix-zh.md)** - 当前 backend 能力矩阵

### Architecture & Core Concepts / 架构与核心概念

#### SMS Terminology / SMS术语说明
- **[sms-terminology-en.md](./sms-terminology-en.md)** - English documentation explaining SMS terminology and architecture
- **[sms-terminology-zh.md](./sms-terminology-zh.md)** - 中文版SMS术语和架构说明文档

#### API Changes & Removals / API变更与移除
- **[objectref-api-removal-en.md](./objectref-api-removal-en.md)** - English documentation for ObjectRef API removal
- **[objectref-api-removal-zh.md](./objectref-api-removal-zh.md)** - 中文版ObjectRef API移除文档

### KV Abstraction Layer / KV抽象层

- **[kv-abstraction-en.md](./kv-abstraction-en.md)** - English documentation for the KV abstraction layer
- **[kv-abstraction-zh.md](./kv-abstraction-zh.md)** - 中文版KV抽象层文档
- **[kv-examples.rs](../examples/kv-examples.rs)** - Comprehensive usage examples for the KV abstraction layer / KV抽象层的综合使用示例

### KV Factory Pattern / KV工厂模式

- **[kv-factory-pattern-en.md](./kv-factory-pattern-en.md)** - English documentation for the KV factory pattern
- **[kv-factory-pattern-zh.md](./kv-factory-pattern-zh.md)** - 中文版KV工厂模式文档
- **[kv-factory-examples.rs](../examples/kv-factory-examples.rs)** - Factory pattern usage examples / 工厂模式使用示例

### Unified KV Storage Architecture / 统一KV存储架构

- **[unified-kv-architecture-en.md](./unified-kv-architecture-en.md)** - English documentation for the unified KV storage architecture refactoring
- **[unified-kv-architecture-zh.md](./unified-kv-architecture-zh.md)** - 中文版统一KV存储架构重构文档

### RocksDB Support / RocksDB支持

- **[rocksdb-support-en.md](./rocksdb-support-en.md)** - English documentation for RocksDB integration and usage
- **[rocksdb-support-zh.md](./rocksdb-support-zh.md)** - 中文版RocksDB集成和使用文档

### AI Backend Control Plane / AI Backend 控制面

- **[ai-backend-unified-control-plane-design-en.md](./ai-backend-unified-control-plane-design-en.md)** - English design for the unified AI backend control plane
- **[ai-backend-unified-control-plane-design-zh.md](./ai-backend-unified-control-plane-design-zh.md)** - 中文版统一 AI backend 控制面设计文档
- **[web-admin-overview-en.md](./web-admin-overview-en.md)** - Current Web Admin page and API overview / 当前 Web Admin 页面与 API 概览（英文）
- **[web-admin-overview-zh.md](./web-admin-overview-zh.md)** - 当前 Web Admin 页面与 API 概览
- **[web-admin-ui-guide-en.md](./web-admin-ui-guide-en.md)** - Current Web Admin usage guide / 当前 Web Admin 使用指南（英文）
- **[web-admin-ui-guide-zh.md](./web-admin-ui-guide-zh.md)** - 当前 Web Admin 使用指南
- **[backend-support-matrix-en.md](./backend-support-matrix-en.md)** - Current backend support matrix / 当前 backend 能力矩阵（英文）
- **[backend-support-matrix-zh.md](./backend-support-matrix-zh.md)** - 当前 backend 能力矩阵

## KV Abstraction Layer Overview / KV抽象层概述

The KV abstraction layer is a key component of the spear-next project that provides:

KV抽象层是spear-next项目的关键组件，提供：

### Features / 特性

- **Unified Interface / 统一接口**: A single trait (`KvStore`) for different storage backends
- **Multiple Backends / 多种后端**: Support for in-memory and Sled database storage
- **Async Operations / 异步操作**: All operations are asynchronous for better performance
- **Type Safety / 类型安全**: Strong typing with `KvKey` and `KvValue`
- **Range Queries / 范围查询**: Support for prefix-based and range-based queries
- **Serialization Helpers / 序列化辅助**: Built-in JSON serialization utilities
- **Batch Operations / 批量操作**: Efficient batch put and delete operations

### Implementations / 实现

1. **MemoryKvStore**: In-memory storage for testing and development
   - Fast access / 快速访问
   - No persistence / 无持久化
   - Ideal for testing / 适合测试

2. **SledKvStore**: Persistent storage using Sled database
   - ACID transactions / ACID事务
   - Persistent storage / 持久化存储
   - Production-ready / 生产就绪

### Integration / 集成

The KV abstraction layer is integrated into the `NodeRegistry` through the `KvNodeRegistry` implementation, which provides:

KV抽象层通过`KvNodeRegistry`实现集成到`NodeRegistry`中，提供：

- Persistent node storage / 持久化节点存储
- Async node operations / 异步节点操作

## KV Factory Pattern Overview / KV工厂模式概述

The KV Factory Pattern provides a flexible and configurable way to create different types of key-value stores at runtime. This pattern enhances the KV abstraction layer with:

KV工厂模式提供了一种灵活且可配置的方式来在运行时创建不同类型的键值存储。此模式通过以下方式增强了KV抽象层：

### Features / 特性

- **Configuration-Driven Selection / 配置驱动选择**: Choose storage backend through configuration
- **Environment Variable Support / 环境变量支持**: Configure stores using environment variables
- **Runtime Backend Switching / 运行时后端切换**: Switch between backends based on conditions
- **Custom Factory Implementation / 自定义工厂实现**: Extend with custom factory logic
- **Validation and Error Handling / 验证和错误处理**: Comprehensive configuration validation

### Components / 组件

1. **KvStoreConfig**: Configuration structure for store creation / 存储创建的配置结构
2. **KvStoreFactory**: Factory trait for creating stores / 创建存储的工厂特征
3. **DefaultKvStoreFactory**: Default implementation / 默认实现
4. **Global Factory Functions**: Convenient helper functions / 便利的辅助函数

### Usage Patterns / 使用模式

- **Application Mode Selection / 应用模式选择**: Different stores for test/dev/prod
- **Environment-based Configuration / 基于环境的配置**: Load settings from environment variables
- **Custom Factory Logic / 自定义工厂逻辑**: Implement specialized creation logic
- Consistent API with the original `NodeRegistry` / 与原始`NodeRegistry`一致的API

## Usage Examples / 使用示例

The `examples/kv-examples.rs` file contains comprehensive examples covering:

`examples/kv-examples.rs`文件包含涵盖以下内容的综合示例：

1. **Basic Operations / 基本操作**: CRUD operations, existence checks
2. **Serialization / 序列化**: Working with complex data types
3. **Range Queries / 范围查询**: Prefix-based and range-based queries
4. **Batch Operations / 批量操作**: Efficient bulk operations
5. **NodeRegistry Integration / NodeRegistry集成**: Using KV with node management
6. **Persistent Storage / 持久化存储**: Working with Sled backend
7. **Error Handling / 错误处理**: Proper error handling patterns

## Testing / 测试

To run the KV abstraction layer tests:

运行KV抽象层测试：

```bash
# Basic tests / 基本测试
cargo test storage

# Tests with Sled feature / 带Sled功能的测试
cargo test storage --features sled

# NodeRegistry integration tests / NodeRegistry集成测试
cargo test common::node --features sled
```

## Future Enhancements / 未来增强

Planned improvements for the KV abstraction layer:

KV抽象层的计划改进：

- **Redis Backend / Redis后端**: Distributed caching support
- **Compression / 压缩**: Optional value compression for large data
- **Encryption / 加密**: At-rest encryption for sensitive data
- **Metrics / 指标**: Built-in performance and usage metrics
- **Backup/Restore / 备份/恢复**: Automated backup and restore functionality

## 测试状态 / Test Status

### 集成测试 / Integration Tests
- ✅ Task API 集成测试 / Task API Integration Tests
- ✅ ObjectRef API 集成测试 / ObjectRef API Integration Tests
- ✅ KV存储集成测试 / KV Storage Integration Tests
- ✅ HTTP网关集成测试 / HTTP Gateway Integration Tests

### 单元测试 / Unit Tests
- ✅ 存储层单元测试 / Storage Layer Unit Tests
- ✅ 服务层单元测试 / Service Layer Unit Tests
- ✅ HTTP处理器单元测试 / HTTP Handler Unit Tests

### 最近修复 / Recent Fixes
- ✅ 为所有集成测试添加了统一的日志初始化函数，过滤掉嘈杂的HTTP/2协议调试日志 / Added unified logging initialization for all integration tests, filtering out noisy HTTP/2 protocol debug logs
- ✅ 修复了ObjectRef集成测试中的查询参数问题，使用`add_query_param`方法而不是直接拼接URL / Fixed query parameter issues in ObjectRef integration tests by using `add_query_param` method instead of direct URL concatenation
- ✅ 修复了ObjectRef集成测试中的JSON数据格式问题，确保与API处理器期望的数据结构匹配 / Fixed JSON data format issues in ObjectRef integration tests to match expected data structures in API handlers
- ✅ 修复了引用操作（add_ref, pin_object）的测试，添加了必需的JSON请求体 / Fixed reference operation tests (add_ref, pin_object) by adding required JSON request bodies

### 测试日志优化 / Test Logging Optimization
- 为所有集成测试添加了统一的日志初始化函数
- 过滤掉了嘈杂的HTTP/2协议调试日志（h2::codec::framed_read等）
- 过滤掉了应用程序级别的info日志（spear_next::services::objectref等）
- 设置日志级别为：`spear_next=warn,h2=warn,hyper=warn,tower=warn,axum=warn`
- 使用 `std::sync::Once` 确保日志只初始化一次
- 测试输出现在只显示编译警告和测试结果，非常清洁

## Contributing / 贡献

When extending the KV abstraction layer:

扩展KV抽象层时：

1. Follow the existing patterns and conventions / 遵循现有模式和约定
2. Add comprehensive tests for new functionality / 为新功能添加全面测试
3. Update documentation in both English and Chinese / 更新英文和中文文档
4. Include usage examples for new features / 包含新功能的使用示例
5. Update this README with new content / 使用新内容更新此README

## AI Development Notes / AI开发说明

This documentation was created to facilitate AI-assisted development. When working with AI tools on this project:

此文档旨在促进AI辅助开发。在此项目上使用AI工具时：

- Reference these documents for context about the KV abstraction layer / 参考这些文档了解KV抽象层的上下文
- Use the examples as templates for new functionality / 使用示例作为新功能的模板
- Maintain the bilingual documentation standard / 保持双语文档标准
- Update docs when making significant changes / 进行重大更改时更新 docs

---

*Generated by AI for AI-assisted development / 由AI生成，用于AI辅助开发*
