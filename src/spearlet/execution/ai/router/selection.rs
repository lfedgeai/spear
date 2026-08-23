use std::collections::HashMap;

use rand::Rng;

use crate::spearlet::execution::ai::ir::{CanonicalError, CanonicalRequestEnvelope};
use crate::spearlet::execution::ai::router::policy::SelectionPolicy;
use crate::spearlet::execution::ai::router::registry::BackendInstance;

#[derive(Debug, Clone)]
pub struct CandidateSnapshot<'a> {
    pub count: usize,
    pub names: Vec<&'a str>,
}

/// Build a compact candidate snapshot for routing logs.
/// 构建用于路由日志的紧凑候选快照。
pub fn snapshot_candidates<'a>(candidates: &[&'a BackendInstance]) -> CandidateSnapshot<'a> {
    CandidateSnapshot {
        count: candidates.len(),
        names: candidates
            .iter()
            .take(8)
            .map(|c| c.spec.name.as_str())
            .collect(),
    }
}

/// Select the final backend using the configured policy and optional weight overrides.
/// 使用配置的策略和可选权重覆盖选择最终 backend。
pub fn select_backend(
    policy: SelectionPolicy,
    req: &CanonicalRequestEnvelope,
    candidates: Vec<&BackendInstance>,
    weight_overrides: &HashMap<String, u32>,
) -> Result<BackendInstance, CanonicalError> {
    match policy {
        SelectionPolicy::WeightedRandom if !weight_overrides.is_empty() => {
            select_weighted_random_overrides(req, candidates, weight_overrides)
        }
        _ => policy.select(req, candidates).cloned(),
    }
}

/// Weighted-random selection that honors per-backend override weights.
/// 支持逐 backend 覆盖权重的加权随机选择。
fn select_weighted_random_overrides(
    req: &CanonicalRequestEnvelope,
    candidates: Vec<&BackendInstance>,
    weight_overrides: &HashMap<String, u32>,
) -> Result<BackendInstance, CanonicalError> {
    let total: u32 = candidates
        .iter()
        .map(|c| {
            weight_overrides
                .get(&c.spec.name)
                .copied()
                .unwrap_or(c.spec.weight)
                .max(1)
        })
        .sum();
    let mut rng = rand::thread_rng();
    let mut pick = rng.gen_range(0..total);
    for c in candidates {
        let w = weight_overrides
            .get(&c.spec.name)
            .copied()
            .unwrap_or(c.spec.weight)
            .max(1);
        if pick < w {
            return Ok(c.clone());
        }
        pick -= w;
    }
    Err(CanonicalError {
        code: "no_candidate_backend".to_string(),
        message: "no candidate backend".to_string(),
        retryable: false,
        operation: Some(req.operation.clone()),
    })
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
                required_features: vec![],
                required_transports: vec![],
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

    fn mk_inst(name: &str, weight: u32) -> BackendInstance {
        BackendInstance {
            spec: BackendSpec {
                name: name.to_string(),
                kind: "stub".to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["http".to_string()],
                weight,
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
    fn snapshot_candidates_limits_names_for_logs() {
        let a = mk_inst("a", 1);
        let b = mk_inst("b", 1);
        let cands = vec![&a, &b];
        let snapshot = snapshot_candidates(&cands);
        assert_eq!(snapshot.count, 2);
        assert_eq!(snapshot.names, vec!["a", "b"]);
    }

    #[test]
    fn select_backend_with_override_returns_the_only_candidate() {
        let req = mk_req();
        let a = mk_inst("a", 1);
        let mut overrides = HashMap::new();
        overrides.insert("a".to_string(), 999);
        let selected = select_backend(SelectionPolicy::WeightedRandom, &req, vec![&a], &overrides)
            .expect("selection should succeed");
        assert_eq!(selected.spec.name, "a");
    }
}
