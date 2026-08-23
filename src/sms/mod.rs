//! SMS (SPEAR Metadata Server) module
//! SMS（SPEAR元数据服务器）模块
//!
//! This module contains all SMS-related functionality including:
//! 此模块包含所有SMS相关功能，包括：
//!
//! - Node management and registration / 节点管理和注册
//! - Task scheduling and execution / 任务调度和执行
//! - Resource monitoring / 资源监控
//! - Configuration management / 配置管理
//! - HTTP and gRPC API handlers / HTTP和gRPC API处理器
//!
//! ## Architecture / 架构
//!
//! The SMS follows a microservice architecture with the following components:
//! SMS遵循微服务架构，包含以下组件：
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │   HTTP Gateway  │    │   gRPC Server   │    │   Node Service  │
//! │   HTTP网关      │    │   gRPC服务器    │    │   节点服务      │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//!          │                       │                       │
//!          └───────────────────────┼───────────────────────┘
//!                                  │
//!                         ┌─────────────────┐
//!                         │   Storage Layer │
//!                         │   存储层        │
//!                         └─────────────────┘
//! ```
//!
//! ## Module Structure / 模块结构
//!
//! - `config`: SMS-specific configuration / SMS特定配置
//! - `services`: Business logic services / 业务逻辑服务
//! - `handlers`: HTTP request handlers / HTTP请求处理器
//! - `grpc_server`: gRPC server implementation / gRPC服务器实现
//! - `http_gateway`: HTTP gateway implementation / HTTP网关实现
//! - `service`: Main SMS service implementation / 主要SMS服务实现

pub(crate) mod admin_api;
pub mod admin_credentials;
pub(crate) mod ai_admin_api;
mod ai_backend_rpc;
pub mod ai_backends;
pub mod config;
pub mod execution_logs;
pub mod gateway;
pub mod grpc_server;
pub mod handlers;
pub mod http_gateway;
pub mod instance_execution_index;
pub(crate) mod node_api;
pub mod placement;
pub mod projectors;
pub(crate) mod query_support;
pub mod registry;
pub mod registry_watch;
pub mod routes;
pub(crate) mod runtime;
pub(crate) mod runtime_api;
pub mod service;
pub mod services;
pub mod stream_mux;
pub(crate) mod task_api;
mod task_assignment_rpc;
mod task_rpc;
pub(crate) mod task_semantics;
pub mod types;
pub mod unified_events;
pub mod web_admin;

#[cfg(test)]
pub mod config_test;
#[cfg(test)]
pub mod file_service_test;
#[cfg(test)]
pub mod gateway_test;
#[cfg(test)]
pub mod grpc_server_test;
#[cfg(test)]
pub mod handlers_test;
#[cfg(test)]
pub mod http_gateway_test;
#[cfg(test)]
pub mod placement_penalty_test;
#[cfg(test)]
pub mod routes_test;
#[cfg(test)]
pub mod service_unified_events_test;
#[cfg(test)]
pub mod task_status_test;
#[cfg(test)]
pub mod unified_events_test;

// Re-export commonly used types / 重新导出常用类型
pub use grpc_server::GrpcServer;
pub use handlers::*;
pub use http_gateway::HttpGateway;
pub use service::SmsServiceImpl;
pub use services::*;
pub use types::*;
