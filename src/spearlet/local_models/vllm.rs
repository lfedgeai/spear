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
    ExternalEndpoint(VllmExternalEndpoint),
    /// Process orchestration is intentionally not implemented yet.
    /// 进程编排目前刻意不实现。
    Placeholder { reason: String },
}

/// Typed vLLM external endpoint source.
/// 强类型 vLLM 外部端点来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VllmEndpointSource {
    /// Metadata explicitly requests external endpoint mode.
    /// metadata 显式要求 external endpoint 模式。
    MetadataMode,
    /// Metadata says the process is already managed externally.
    /// metadata 表示该进程已由外部托管。
    ManagedExternally,
    /// `spec.base_url` implies an existing node-local endpoint.
    /// `spec.base_url` 隐含已有节点本地端点。
    SpecBaseUrl,
}

/// Typed vLLM external endpoint resolved from spec and metadata.
/// 从 spec 与 metadata 解析出的强类型 vLLM 外部端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VllmExternalEndpoint {
    pub base_url: String,
    pub source: VllmEndpointSource,
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
        match resolve_vllm_external_endpoint(spec, params) {
            Some(endpoint) => VllmPreparedAssignment::ExternalEndpoint(endpoint),
            None => VllmPreparedAssignment::Placeholder {
                reason: "placeholder: vLLM local backend process orchestration is not implemented yet; set metadata.mode=external_endpoint with spec.base_url to use an existing node-local vLLM service".to_string(),
            },
        }
    }
}

/// vLLM assignment execution mode resolved from backend metadata.
/// 从 backend metadata 解析出的 vLLM assignment 执行模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VllmAssignmentMode {
    /// Use an already-running node-local HTTP endpoint.
    /// 使用节点上已经存在的 HTTP 服务端点。
    ExternalEndpoint,
    /// No concrete execution mode is available yet.
    /// 当前还没有可执行的具体模式。
    Placeholder,
}

/// Resolve the execution mode for a vLLM assignment.
/// 解析 vLLM assignment 的执行模式。
pub fn infer_vllm_assignment_mode(
    spec: &BackendSpec,
    params: &HashMap<String, String>,
) -> VllmAssignmentMode {
    if resolve_vllm_external_endpoint(spec, params).is_some() {
        VllmAssignmentMode::ExternalEndpoint
    } else {
        VllmAssignmentMode::Placeholder
    }
}

/// Resolve the typed vLLM external endpoint, if one is available.
/// 解析强类型 vLLM 外部端点；若当前没有可用端点则返回空。
pub fn resolve_vllm_external_endpoint(
    spec: &BackendSpec,
    params: &HashMap<String, String>,
) -> Option<VllmExternalEndpoint> {
    let base_url = spec.base_url.trim();
    if base_url.is_empty() {
        return None;
    }

    let mode = params
        .get("mode")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if mode == "external_endpoint" {
        return Some(VllmExternalEndpoint {
            base_url: base_url.to_string(),
            source: VllmEndpointSource::MetadataMode,
        });
    }

    let managed_externally = params
        .get("managed_externally")
        .map(|value| parse_truthy(value))
        .unwrap_or(false);
    if managed_externally {
        return Some(VllmExternalEndpoint {
            base_url: base_url.to_string(),
            source: VllmEndpointSource::ManagedExternally,
        });
    }

    if params.get("mode").is_none() {
        return Some(VllmExternalEndpoint {
            base_url: base_url.to_string(),
            source: VllmEndpointSource::SpecBaseUrl,
        });
    }

    None
}

/// Parse a truthy string used by metadata params.
/// 解析 metadata 参数中使用的布尔真值字符串。
fn parse_truthy(value: &str) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        _ => false,
    }
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
        let mut spec = sample_spec();
        spec.base_url = "http://127.0.0.1:8000/v1".to_string();

        let prepared = supervisor.prepare_assignment(&spec, &params);

        assert_eq!(
            prepared,
            VllmPreparedAssignment::ExternalEndpoint(VllmExternalEndpoint {
                base_url: "http://127.0.0.1:8000/v1".to_string(),
                source: VllmEndpointSource::MetadataMode,
            })
        );
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

    #[test]
    fn resolve_external_endpoint_uses_spec_base_url_implicitly() {
        let mut spec = sample_spec();
        spec.base_url = "http://127.0.0.1:8001/v1".to_string();

        let endpoint =
            resolve_vllm_external_endpoint(&spec, &HashMap::new()).expect("external endpoint");
        assert_eq!(endpoint.base_url, "http://127.0.0.1:8001/v1");
        assert_eq!(endpoint.source, VllmEndpointSource::SpecBaseUrl);
    }

    #[test]
    fn resolve_external_endpoint_uses_managed_externally_flag() {
        let mut spec = sample_spec();
        spec.base_url = "http://127.0.0.1:8002/v1".to_string();
        let mut params = HashMap::new();
        params.insert("managed_externally".to_string(), "true".to_string());

        let endpoint =
            resolve_vllm_external_endpoint(&spec, &params).expect("external endpoint");
        assert_eq!(endpoint.base_url, "http://127.0.0.1:8002/v1");
        assert_eq!(endpoint.source, VllmEndpointSource::ManagedExternally);
    }
}
