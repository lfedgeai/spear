//! Unified AI backend control-plane skeleton / 统一 AI backend 控制面骨架
//!
//! This module hosts the UUID-based AI backend control plane, including
//! typed models, validation, KV persistence, and read-model primitives.
//!
//! 本模块承载基于 UUID 的 AI backend 控制面，包括强类型模型、校验、
//! KV 持久化与 read model 基础能力。

pub mod model;
pub mod placement_service;
pub mod proto_conv;
pub mod read_model;
pub mod repository;
pub mod repository_kv;
pub mod service;
pub mod status_service;
pub mod validator;

pub use model::{
    AiBackendDesiredStateModel, AiBackendHostingModel, AiBackendManagementModeModel,
    AiBackendNodeRuntimeStatusModel, AiBackendNodeStatusRecordModel, AiBackendPlacementRecordModel,
    AiBackendRecordModel, AiBackendSpecModel, AiModelView, AiModelViewInstance,
    ResolvedBackendAssignment,
};
pub use placement_service::{AiBackendPlacementService, UpsertPlacementInput};
pub use repository::AiBackendRepository;
pub use repository_kv::KvAiBackendRepository;
pub use service::{AiBackendService, CreateAiBackendInput, UpdateAiBackendInput};
pub use status_service::AiBackendStatusService;
