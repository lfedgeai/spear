use crate::spearlet::execution::ai::ir::{CanonicalError, CanonicalRequestEnvelope};
use crate::spearlet::execution::ai::router::registry::BackendInstance;

/// Build a detailed no-candidate routing error from the full instance view.
/// 基于完整实例视图构造详细的无候选路由错误。
pub fn build_no_candidate_error(
    req: &CanonicalRequestEnvelope,
    instances: &[&BackendInstance],
) -> CanonicalError {
    let mut supporting: Vec<String> = Vec::new();
    for inst in instances.iter().copied() {
        if !inst.capabilities.supports_operation(&req.operation) {
            continue;
        }
        supporting.push(format_support_entry(req, inst));
    }

    let msg = format!(
        "no candidate backend: op={:?} required_features={:?} required_transports={:?} routing_backend={:?} allowlist={:?} denylist={:?} backends={:?}",
        req.operation,
        req.requirements.required_features,
        req.requirements.required_transports,
        req.routing.backend,
        req.routing.allowlist,
        req.routing.denylist,
        supporting,
    );
    CanonicalError {
        code: "no_candidate_backend".to_string(),
        message: msg,
        retryable: false,
        operation: Some(req.operation.clone()),
    }
}

/// Format one backend support entry for no-candidate diagnostics.
/// 为无候选诊断格式化单个 backend 的支持信息。
fn format_support_entry(req: &CanonicalRequestEnvelope, inst: &BackendInstance) -> String {
    let mut missing_features: Vec<&str> = Vec::new();
    for f in req.requirements.required_features.iter() {
        if !inst.capabilities.has_feature(f) {
            missing_features.push(f);
        }
    }

    let mut missing_transports: Vec<&str> = Vec::new();
    for t in req.requirements.required_transports.iter() {
        if !inst.capabilities.transports.iter().any(|x| x == t) {
            missing_transports.push(t);
        }
    }

    format!(
        "{}(missing_features={:?}, missing_transports={:?}, features={:?}, transports={:?})",
        inst.spec.name,
        missing_features,
        missing_transports,
        inst.capabilities.features,
        inst.capabilities.transports
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::proto::sms::BackendSpec;
    use crate::spearlet::execution::ai::backends::stub::StubBackendAdapter;
    use crate::spearlet::execution::ai::ir::{
        CanonicalRequestEnvelope, ChatCompletionsPayload, ChatMessage, Operation, Payload,
        Requirements, RoutingHints,
    };
    use crate::spearlet::execution::ai::router::capabilities::Capabilities;
    use crate::spearlet::execution::ai::router::registry::{BackendInstance, Hosting};
    use serde_json::Value;

    fn mk_req() -> CanonicalRequestEnvelope {
        CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: Operation::ChatCompletions,
            meta: HashMap::new(),
            routing: RoutingHints::default(),
            requirements: Requirements {
                required_features: vec!["vision".to_string()],
                required_transports: vec!["websocket".to_string()],
            },
            timeout_ms: None,
            payload: Payload::ChatCompletions(ChatCompletionsPayload {
                model: "gpt-4o-mini".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: Value::String("hello".to_string()),
                    tool_call_id: None,
                    tool_calls: None,
                    name: None,
                }],
                tools: vec![],
                params: HashMap::new(),
            }),
            extra: HashMap::new(),
        }
    }

    fn mk_inst(name: &str) -> BackendInstance {
        BackendInstance {
            spec: BackendSpec {
                name: name.to_string(),
                kind: "stub".to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["http".to_string()],
                weight: 100,
                priority: 0,
                base_url: String::new(),
                provider: "internal".to_string(),
                model: String::new(),
                hosting: 0,
                credential_ref: String::new(),
                origin: 0,
                deployment_id: String::new(),
            },
            hosting: Hosting::Local,
            capabilities: Capabilities {
                ops: vec![Operation::ChatCompletions],
                features: vec![],
                transports: vec!["http".to_string()],
            },
            adapter: Arc::new(StubBackendAdapter::new(name)),
        }
    }

    #[test]
    fn build_no_candidate_error_includes_backend_support_details() {
        let req = mk_req();
        let inst = mk_inst("stub-a");
        let err = build_no_candidate_error(&req, &[&inst]);
        assert_eq!(err.code, "no_candidate_backend");
        assert!(err.message.contains("stub-a"));
        assert!(err.message.contains("missing_features"));
        assert!(err.message.contains("missing_transports"));
    }
}
