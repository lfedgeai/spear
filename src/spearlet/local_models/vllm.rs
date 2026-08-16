//! vLLM local runtime skeleton.
//! vLLM 本地运行时骨架。
//!
//! This module intentionally exposes only preparation logic for now. It gives
//! the unified control plane a stable place to grow real vLLM lifecycle
//! management later, without committing to process orchestration today.
//! 当前该模块只提供准备阶段逻辑；这样统一控制面后续可以在这里继续演进真实
//! 的 vLLM 生命周期管理，而无需现在就承诺实现进程编排。

use std::collections::HashMap;

use crate::proto::sms::BackendSpec;
use crate::spearlet::config::SpearletConfig;

/// Prepared outcome for one vLLM assignment.
/// 单个 vLLM assignment 的准备结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VllmPreparedAssignment {
    /// Use an already-running node-local HTTP endpoint.
    /// 使用节点上已经运行的本地 HTTP 端点。
    ExternalEndpoint,
    /// Process orchestration is intentionally not implemented yet.
    /// 进程编排目前刻意不实现。
    Placeholder { reason: String },
}

/// Lightweight vLLM supervisor skeleton.
/// 轻量级 vLLM supervisor 骨架。
#[derive(Debug, Clone, Default)]
pub struct VllmSupervisor;

impl VllmSupervisor {
    /// Create a new vLLM supervisor skeleton.
    /// 创建新的 vLLM supervisor 骨架。
    pub fn new(_config: &SpearletConfig) -> Self {
        Self
    }

    /// Prepare how one vLLM assignment should be handled.
    /// 准备单个 vLLM assignment 的处理方式。
    pub fn prepare_assignment(
        &self,
        spec: &BackendSpec,
        params: &HashMap<String, String>,
    ) -> VllmPreparedAssignment {
        let mode = resolve_vllm_assignment_mode(spec, params);
        match mode {
            VllmAssignmentMode::ExternalEndpoint => VllmPreparedAssignment::ExternalEndpoint,
            VllmAssignmentMode::Placeholder => VllmPreparedAssignment::Placeholder {
                reason: "placeholder: vLLM local backend process orchestration is not implemented yet; set metadata.mode=external_endpoint with spec.base_url to use an existing node-local vLLM service".to_string(),
            },
        }
    }
}

/// vLLM assignment execution mode resolved from backend metadata.
/// 从 backend metadata 解析出的 vLLM assignment 执行模式。
#[derive(Debug, Clone, PartialEq, Eq)]
enum VllmAssignmentMode {
    /// Use an already-running node-local HTTP endpoint.
    /// 使用节点上已经存在的 HTTP 服务端点。
    ExternalEndpoint,
    /// No concrete execution mode is available yet.
    /// 当前还没有可执行的具体模式。
    Placeholder,
}

/// Resolve the execution mode for a vLLM assignment.
/// 解析 vLLM assignment 的执行模式。
fn resolve_vllm_assignment_mode(
    spec: &BackendSpec,
    params: &HashMap<String, String>,
) -> VllmAssignmentMode {
    let mode = params
        .get("mode")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let managed_externally = params
        .get("managed_externally")
        .map(|value| parse_truthy(value))
        .unwrap_or(false);

    if mode == "external_endpoint" || managed_externally {
        return VllmAssignmentMode::ExternalEndpoint;
    }

    if !spec.base_url.trim().is_empty() && params.get("mode").is_none() {
        return VllmAssignmentMode::ExternalEndpoint;
    }

    VllmAssignmentMode::Placeholder
}

/// Parse a truthy string used by metadata params.
/// 解析 metadata 参数中使用的布尔真值字符串。
fn parse_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::{BackendHosting, BackendOrigin};

    fn sample_spec() -> BackendSpec {
        BackendSpec {
            name: "vllm-local".to_string(),
            kind: "vllm-openai".to_string(),
            operations: vec!["chat".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 1,
            priority: 0,
            base_url: String::new(),
            provider: "vllm".to_string(),
            model: "foo".to_string(),
            hosting: BackendHosting::NodeLocal as i32,
            credential_ref: String::new(),
            origin: BackendOrigin::Sms as i32,
            deployment_id: String::new(),
        }
    }

    #[test]
    fn prepare_assignment_returns_external_endpoint_when_requested() {
        let supervisor = VllmSupervisor::new(&SpearletConfig::default());
        let mut params = HashMap::new();
        params.insert("mode".to_string(), "external_endpoint".to_string());

        let prepared = supervisor.prepare_assignment(&sample_spec(), &params);

        assert_eq!(prepared, VllmPreparedAssignment::ExternalEndpoint);
    }

    #[test]
    fn prepare_assignment_returns_placeholder_without_endpoint_mode() {
        let supervisor = VllmSupervisor::new(&SpearletConfig::default());
        let prepared = supervisor.prepare_assignment(&sample_spec(), &HashMap::new());

        match prepared {
            VllmPreparedAssignment::Placeholder { reason } => {
                assert!(reason.contains("placeholder"));
            }
            other => panic!("expected placeholder, got {other:?}"),
        }
    }
}
