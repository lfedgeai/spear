# Documentation Index / 文档索引

## 文档分类 / Document Categories

### 🏗️ Architecture & Core Concepts / 架构与核心概念

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| System Architecture Diagram | - | - | 架构图：`docs/diagrams/spear-architecture.png` |
| Project Architecture Overview | [project-architecture-overview-en.md](./project-architecture-overview-en.md) | [project-architecture-overview-zh.md](./project-architecture-overview-zh.md) | 项目架构全面概述 |
| Task Execution Model | [task-execution-model-en.md](./task-execution-model-en.md) | [task-execution-model-zh.md](./task-execution-model-zh.md) | Task 执行模型与方案 A 约定 |
| SMS Terminology | [sms-terminology-en.md](./sms-terminology-en.md) | [sms-terminology-zh.md](./sms-terminology-zh.md) | SMS术语和架构说明 |
| AI Backends Configuration | [llm-backends-configuration-en.md](./llm-backends-configuration-en.md) | [llm-backends-configuration-zh.md](./llm-backends-configuration-zh.md) | AI backend/credentials 配置说明与示例 |
| AI Runtime Refactor Roadmap | [ai-runtime-refactor-roadmap-en.md](./ai-runtime-refactor-roadmap-en.md) | [ai-runtime-refactor-roadmap-zh.md](./ai-runtime-refactor-roadmap-zh.md) | AI backend/runtime/SMS/Web Admin 重构路线图 |
| ObjectRef API Removal | [objectref-api-removal-en.md](./objectref-api-removal-en.md) | [objectref-api-removal-zh.md](./objectref-api-removal-zh.md) | ObjectRef API移除文档 |
| MCP Integration Architecture | [mcp-integration-architecture-en.md](./mcp-integration-architecture-en.md) | [mcp-integration-architecture-zh.md](./mcp-integration-architecture-zh.md) | MCP 注册中心、注入与执行链路 |
| Task-level MCP Subset Design | [mcp-task-subset-design-en.md](./mcp-task-subset-design-en.md) | [mcp-task-subset-design-zh.md](./mcp-task-subset-design-zh.md) | Task 级 MCP 子集选择与治理 |

### 💾 Storage Layer / 存储层

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| KV Abstraction | [kv-abstraction-en.md](./kv-abstraction-en.md) | [kv-abstraction-zh.md](./kv-abstraction-zh.md) | KV抽象层设计与实现 |
| KV Factory Pattern | [kv-factory-pattern-en.md](./kv-factory-pattern-en.md) | [kv-factory-pattern-zh.md](./kv-factory-pattern-zh.md) | KV工厂模式实现 |
| Unified KV Architecture | [unified-kv-architecture-en.md](./unified-kv-architecture-en.md) | [unified-kv-architecture-zh.md](./unified-kv-architecture-zh.md) | 统一KV存储架构 |
| RocksDB Support | [rocksdb-support-en.md](./rocksdb-support-en.md) | [rocksdb-support-zh.md](./rocksdb-support-zh.md) | RocksDB集成支持 |
| KV Testing Architecture | [kv-testing-architecture-en.md](./kv-testing-architecture-en.md) | [kv-testing-architecture-zh.md](./kv-testing-architecture-zh.md) | KV测试架构设计 |

### ⚙️ Service Layer / 服务层

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Task API Refactor | [task-api-refactor-en.md](./task-api-refactor-en.md) | [task-api-refactor-zh.md](./task-api-refactor-zh.md) | Task API重构文档 |
| Task List Filtering | [task-filter-refactoring-en.md](./task-filter-refactoring-en.md) | [task-filter-refactoring-zh.md](./task-filter-refactoring-zh.md) | Task 列表过滤与分页实现说明 |
| Task Service Optimization | [task-service-optimization-en.md](./task-service-optimization-en.md) | [task-service-optimization-zh.md](./task-service-optimization-zh.md) | Task服务优化文档 |
| Resource Service Refactoring | [resource-service-refactoring-en.md](./resource-service-refactoring-en.md) | [resource-service-refactoring-zh.md](./resource-service-refactoring-zh.md) | 资源服务代码重构指南 |
| Config Service Refactoring | [config-service-refactoring-en.md](./config-service-refactoring-en.md) | [config-service-refactoring-zh.md](./config-service-refactoring-zh.md) | 配置服务代码重构指南 |
| Task Events Subscriber | [task-events-subscriber-en.md](./task-events-subscriber-en.md) | [task-events-subscriber-zh.md](./task-events-subscriber-zh.md) | 任务事件订阅器设计与实现 |

### 🚀 Runtime Layer / 运行时层

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Kubernetes Runtime Implementation | [kubernetes-runtime-implementation-en.md](./kubernetes-runtime-implementation-en.md) | [kubernetes-runtime-implementation-zh.md](./kubernetes-runtime-implementation-zh.md) | Kubernetes运行时实现文档 |
| WASM Runtime Usage | [wasm-runtime-usage-en.md](./wasm-runtime-usage-en.md) | [wasm-runtime-usage-zh.md](./wasm-runtime-usage-zh.md) | WASM 运行时使用与 SMS 文件协议说明 |
| Execution Mode Support | [execution-mode-support-en.md](./execution-mode-support-en.md) | [execution-mode-support-zh.md](./execution-mode-support-zh.md) | 函数调用执行模式（Sync/Async/Stream）支持 |

### 🧩 Hostcall API / Hostcall API

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Spear Hostcall Chat Completion | [api/spear-hostcall/chat-completion-en.md](./api/spear-hostcall/chat-completion-en.md) | [api/spear-hostcall/chat-completion-zh.md](./api/spear-hostcall/chat-completion-zh.md) | WASM hostcall 的 Chat Completion 设计 |
| CChat Function Call Design | [cchat-function-call-design-en.md](./cchat-function-call-design-en.md) | [cchat-function-call-design-zh.md](./cchat-function-call-design-zh.md) | Chat completion 的 Tool Calling（Function Call）闭环设计 |
| CChat Default Model Selection | [implementation/cchat-default-model-selection-en.md](./implementation/cchat-default-model-selection-en.md) | [implementation/cchat-default-model-selection-zh.md](./implementation/cchat-default-model-selection-zh.md) | CChat 默认模型选择策略设计 |
| fd/epoll + cchat Migration Plan | [implementation/fd-epoll-cchat-migration-plan-en.md](./implementation/fd-epoll-cchat-migration-plan-en.md) | [implementation/fd-epoll-cchat-migration-plan-zh.md](./implementation/fd-epoll-cchat-migration-plan-zh.md) | fd/epoll 子系统落地与 cchat 迁移实施计划 |
| mic_fd Implementation Notes | [implementation/mic-fd-implementation-en.md](./implementation/mic-fd-implementation-en.md) | [implementation/mic-fd-implementation-zh.md](./implementation/mic-fd-implementation-zh.md) | mic_fd 落地实现说明 |
| rtasr_fd Implementation Notes | [implementation/realtime-asr-implementation-en.md](./implementation/realtime-asr-implementation-en.md) | [implementation/realtime-asr-implementation-zh.md](./implementation/realtime-asr-implementation-zh.md) | rtasr_fd 落地实现说明 |
| Mic Device Capture (mic-device feature) | [mic-device-feature-en.md](./mic-device-feature-en.md) | [mic-device-feature-zh.md](./mic-device-feature-zh.md) | mic_fd 使用本机麦克风采集的可选编译特性 |

### 🌐 HTTP Layer / HTTP层

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| HTTP Refactor | [http-refactor-en.md](./http-refactor-en.md) | [http-refactor-zh.md](./http-refactor-zh.md) | HTTP层重构文档 |
| Handlers Architecture | [handlers-architecture-en.md](./handlers-architecture-en.md) | [handlers-architecture-zh.md](./handlers-architecture-zh.md) | 处理器架构设计 |
| Handlers to Services Refactor | [handlers-to-services-refactor-en.md](./handlers-to-services-refactor-en.md) | [handlers-to-services-refactor-zh.md](./handlers-to-services-refactor-zh.md) | 处理器到服务重构 |
| Web Admin Overview | [web-admin-overview-en.md](./web-admin-overview-en.md) | [web-admin-overview-zh.md](./web-admin-overview-zh.md) | 管理页面概览 |
| Web Admin UI Guide | [web-admin-ui-guide-en.md](./web-admin-ui-guide-en.md) | [web-admin-ui-guide-zh.md](./web-admin-ui-guide-zh.md) | 管理页面交互与使用指南 |
| SPEAR Console Overview | [spear-console-overview-en.md](./spear-console-overview-en.md) | [spear-console-overview-zh.md](./spear-console-overview-zh.md) | 用户前端 Console 概览 |
| SPEAR Console Voice Input Design | [spear-console-voice-input-design-en.md](./spear-console-voice-input-design-en.md) | [spear-console-voice-input-design-zh.md](./spear-console-voice-input-design-zh.md) | Console 按住说话语音输入设计（协议/前端/Agent） |

### 🔌 gRPC Layer / gRPC层

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| gRPC API Implementation | [grpc-api-implementation-en.md](./grpc-api-implementation-en.md) | [grpc-api-implementation-zh.md](./grpc-api-implementation-zh.md) | gRPC API实现文档 |
| Function Service Implementation | [function-service-implementation-en.md](./function-service-implementation-en.md) | [function-service-implementation-zh.md](./function-service-implementation-zh.md) | 函数服务gRPC实现文档 |
| gRPC Error Handling Fix | [grpc-error-handling-fix-en.md](./grpc-error-handling-fix-en.md) | [grpc-error-handling-fix-zh.md](./grpc-error-handling-fix-zh.md) | gRPC错误处理修复文档 |
| Registration.proto Removal Analysis | [registration-proto-removal-analysis-en.md](./registration-proto-removal-analysis-en.md) | [registration-proto-removal-analysis-zh.md](./registration-proto-removal-analysis-zh.md) | Registration.proto删除可行性分析 |
| Function Invocation Sync-Async Analysis | [function-invocation-sync-async-analysis-en.md](./function-invocation-sync-async-analysis-en.md) | [function-invocation-sync-async-analysis-zh.md](./function-invocation-sync-async-analysis-zh.md) | 同步异步支持现状分析 |
| Invocation/Execution Model Refactor | [invocation-execution-model-refactor-en.md](./invocation-execution-model-refactor-en.md) | [invocation-execution-model-refactor-zh.md](./invocation-execution-model-refactor-zh.md) | 调用模型（Invocation/Execution/Instance）重构设计 |

### 🧹 Code Cleanup & Maintenance / 代码清理与维护

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Constants Module Refactoring | [constants-refactoring-en.md](./constants-refactoring-en.md) | [constants-refactoring-zh.md](./constants-refactoring-zh.md) | Constants模块重构文档 |

### 🚢 Deployment / 部署

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Helm Deployment Guide | [helm-deployment-en.md](./helm-deployment-en.md) | [helm-deployment-zh.md](./helm-deployment-zh.md) | 使用 Helm 部署 SPEAR 集群 |

### 🔧 Troubleshooting & Operations / 故障排除与运维

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| gRPC Transport Error | [grpc-transport-error-troubleshooting-en.md](./grpc-transport-error-troubleshooting-en.md) | [grpc-transport-error-troubleshooting-zh.md](./grpc-transport-error-troubleshooting-zh.md) | gRPC传输错误故障排除指南 |
| MCP Troubleshooting | [mcp-troubleshooting-en.md](./mcp-troubleshooting-en.md) | [mcp-troubleshooting-zh.md](./mcp-troubleshooting-zh.md) | MCP 工具注入与执行排障 |
| Ollama Model Discovery | [ollama-discovery-en.md](./ollama-discovery-en.md) | [ollama-discovery-zh.md](./ollama-discovery-zh.md) | Ollama 模型导入与排障 |
| API Usage Guide | [api-usage-guide-en.md](./api-usage-guide-en.md) | [api-usage-guide-zh.md](./api-usage-guide-zh.md) | RESTful API使用指南 |
| WASM Runtime Usage | [wasm-runtime-usage-en.md](./wasm-runtime-usage-en.md) | [wasm-runtime-usage-zh.md](./wasm-runtime-usage-zh.md) | WASM运行时使用与错误行为说明 |
| Samples Build Guide | [samples-build-guide-en.md](./samples-build-guide-en.md) | [samples-build-guide-zh.md](./samples-build-guide-zh.md) | WASM示例构建指南（Makefile） |
| Swagger UI Fix | [swagger-ui-fix-en.md](./swagger-ui-fix-en.md) | [swagger-ui-fix-zh.md](./swagger-ui-fix-zh.md) | Swagger UI API路径修复指南 |
| Sled Build Fix | [sled-build-fix-en.md](./sled-build-fix-en.md) | [sled-build-fix-zh.md](./sled-build-fix-zh.md) | Sled构建错误修复指南 |
| RocksDB Compilation Fix | [rocksdb-compilation-fix-en.md](./rocksdb-compilation-fix-en.md) | [rocksdb-compilation-fix-zh.md](./rocksdb-compilation-fix-zh.md) | RocksDB编译问题修复指南 |

### 🧪 Testing & Quality / 测试与质量

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Test Validation & Warning Cleanup | [test-validation-and-warning-cleanup-en.md](./test-validation-and-warning-cleanup-en.md) | [test-validation-and-warning-cleanup-zh.md](./test-validation-and-warning-cleanup-zh.md) | 测试验证和警告清理完整指南 |
| Kind Helm E2E | [kind-helm-e2e-en.md](./kind-helm-e2e-en.md) | [kind-helm-e2e-zh.md](./kind-helm-e2e-zh.md) | 使用 kind+Helm 运行端到端测试 |
| Test Fixes | [test-fixes-en.md](./test-fixes-en.md) | [test-fixes-zh.md](./test-fixes-zh.md) | 测试修复和改进 |
| Code Coverage Testing | [code-coverage-en.md](./code-coverage-en.md) | [code-coverage-zh.md](./code-coverage-zh.md) | 代码覆盖率测试指南 |
| Code Cleanup | [code-cleanup-en.md](./code-cleanup-en.md) | [code-cleanup-zh.md](./code-cleanup-zh.md) | 代码清理文档 |
| UI Tests Guide | [ui-tests-guide-en.md](./ui-tests-guide-en.md) | [ui-tests-guide-zh.md](./ui-tests-guide-zh.md) | 前端UI测试使用指南 |

### 📝 Code Examples / 代码示例

| 文件 / File | 描述 / Description |
|---|---|
| [kv-factory-examples.rs](../examples/kv-factory-examples.rs) | KV工厂模式使用示例 |

### 📚 Documentation Guidelines / 文档规范

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Documentation Guidelines | [documentation-guidelines-en.md](./documentation-guidelines-en.md) | [documentation-guidelines-zh.md](./documentation-guidelines-zh.md) | 文档命名与双语维护规范 |

### 🔄 Proto File Management / Proto文件管理

| 文档 / Document | 英文版 / English | 中文版 / Chinese | 描述 / Description |
|---|---|---|---|
| Proto File Regeneration | [proto-regeneration-en.md](./proto-regeneration-en.md) | [proto-regeneration-zh.md](./proto-regeneration-zh.md) | Proto文件重新生成记录 |

## 最新更新 / Latest Updates

### 2025年最新变更 / 2025 Latest Changes

1. **✅ ObjectRef API 完全移除** / **ObjectRef API Complete Removal**
   - 移除了所有 ObjectRef 相关的 proto 定义、服务实现、HTTP 处理器和测试
   - 系统架构简化，专注于核心 SMS 和 Task 功能
   - 所有测试通过，无破坏性变更

2. **🏗️ 架构重构完成** / **Architecture Refactoring Complete**
   - HTTP 层到服务层的完整重构
   - 统一的 KV 存储抽象层实现
   - gRPC 和 HTTP 双协议支持

3. **🧪 测试基础设施优化** / **Test Infrastructure Optimization**
   - 统一的测试日志配置
   - 集成测试覆盖率提升
   - 测试性能优化

4. **🔧 故障排除与运维文档** / **Troubleshooting & Operations Documentation**
   - 创建了 gRPC transport error 故障排除指南
   - 提供了完整的 RESTful API 使用指南
   - 新增 Swagger UI API 路径修复指南
   - 包含常见问题解决方案和最佳实践
   - 支持 Swagger UI 和 curl 命令行工具使用

5. **🚀 Kubernetes Runtime 实现** / **Kubernetes Runtime Implementation**
   - 完整实现了 Kubernetes 运行时支持
   - 支持 Kubernetes Jobs 的创建、管理和监控
   - 提供资源管理、健康检查和指标收集功能
   - 包含全面的错误处理和配置验证
   - 所有编译和测试通过，集成到运行时工厂

## 使用指南 / Usage Guide

### 对于开发者 / For Developers

1. **新功能开发** / **New Feature Development**
   - 参考相应层级的架构文档
   - 遵循现有的设计模式
   - 更新相关的文档

2. **问题排查** / **Troubleshooting**
   - 查看测试修复文档
   - 参考代码清理指南
   - 检查架构设计文档

3. **代码重构** / **Code Refactoring**
   - 参考重构文档中的最佳实践
   - 保持双语注释标准
   - 更新相关测试

### 对于 AI 工具 / For AI Tools

1. **上下文理解** / **Context Understanding**
   - 使用文档索引快速定位相关信息
   - 参考示例代码了解实现模式
   - 遵循项目的编码规范

2. **文档维护** / **Documentation Maintenance**
   - 保持中英文双语标准
   - 更新文档索引
   - 添加新的示例代码

## 文档规范 / Documentation Standards

### 命名规范 / Naming Convention

- 英文文档：`{topic}-en.md`
- 中文文档：`{topic}-zh.md`
- 示例代码：`{topic}-examples.rs`
- 摘要文档：`{topic}-summary-{lang}.md`

### 内容要求 / Content Requirements

- 所有文档必须提供中英文版本
- 代码注释使用中英文双语
- 包含实际的使用示例
- 保持文档的时效性

---

*此索引用于快速定位项目文档与示例*  
*This index helps locate project documentation and examples*
