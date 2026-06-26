use std::collections::HashMap;

use crate::proto::spearlet::{DecisionAction, FilterResponse};
use crate::spearlet::execution::ai::ir::{CanonicalError, Operation};
use crate::spearlet::execution::ai::router::registry::BackendInstance;

#[derive(Debug, Default)]
pub struct AppliedFilterDecision {
    pub weight_overrides: HashMap<String, u32>,
    pub dropped_names: Vec<String>,
}

/// Convert router-filter final-action rejection into a canonical routing error.
/// 将 router-filter 的 final-action 拒绝转换为统一的路由错误。
pub fn reject_error_from_response(
    operation: &Operation,
    resp: &FilterResponse,
) -> Option<CanonicalError> {
    let final_action = resp.final_action.as_ref()?;
    if !final_action.reject_request {
        return None;
    }
    let code = if final_action.reject_code.trim().is_empty() {
        "router_filter_rejected".to_string()
    } else {
        final_action.reject_code.clone()
    };
    let message = if final_action.reject_message.trim().is_empty() {
        "router filter rejected".to_string()
    } else {
        final_action.reject_message.clone()
    };
    Some(CanonicalError {
        code,
        message,
        retryable: false,
        operation: Some(operation.clone()),
    })
}

/// Apply filter-response candidate decisions in place and collect debug side effects.
/// 原地应用 filter-response 的候选决策，并收集调试/权重副作用。
pub fn apply_filter_response<'a>(
    resp: &FilterResponse,
    candidates: &mut Vec<&'a BackendInstance>,
) -> AppliedFilterDecision {
    if let Some(final_action) = resp.final_action.as_ref() {
        if !final_action.force_backend.trim().is_empty() {
            let forced = final_action.force_backend.trim();
            candidates.retain(|c| c.spec.name == forced);
        }
    }

    let mut decision_by_name = HashMap::new();
    for d in resp.decisions.iter() {
        decision_by_name.insert(d.name.as_str(), d);
    }

    candidates.retain(|c| {
        let Some(d) = decision_by_name.get(c.spec.name.as_str()) else {
            return true;
        };
        d.action != DecisionAction::Drop as i32
    });

    let mut outcome = AppliedFilterDecision::default();
    for (name, d) in decision_by_name {
        if d.action == DecisionAction::Drop as i32 {
            outcome.dropped_names.push(name.to_string());
        }
        if let Some(w) = d.weight_override {
            outcome.weight_overrides.insert(name.to_string(), w.min(10_000));
        }
    }
    outcome.dropped_names.sort();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::proto::sms::BackendSpec;
    use crate::proto::spearlet::{CandidateDecision, FinalAction};
    use crate::spearlet::execution::ai::backends::stub::StubBackendAdapter;
    use crate::spearlet::execution::ai::ir::Operation;
    use crate::spearlet::execution::ai::router::capabilities::Capabilities;
    use crate::spearlet::execution::ai::router::registry::Hosting;

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
    fn apply_filter_response_handles_force_drop_and_weight_override() {
        let a = mk_inst("a");
        let b = mk_inst("b");
        let mut candidates = vec![&a, &b];
        let resp = FilterResponse {
            correlation_id: "c1".to_string(),
            decision_id: "d1".to_string(),
            decisions: vec![
                CandidateDecision {
                    name: "a".to_string(),
                    action: DecisionAction::Drop as i32,
                    weight_override: Some(7),
                    priority_override: None,
                    score: None,
                    reason_codes: Vec::new(),
                },
                CandidateDecision {
                    name: "b".to_string(),
                    action: DecisionAction::Keep as i32,
                    weight_override: Some(11),
                    priority_override: None,
                    score: None,
                    reason_codes: Vec::new(),
                },
            ],
            final_action: Some(FinalAction {
                force_backend: "b".to_string(),
                reject_request: false,
                reject_code: String::new(),
                reject_message: String::new(),
            }),
            debug: HashMap::new(),
        };

        let outcome = apply_filter_response(&resp, &mut candidates);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].spec.name, "b");
        assert_eq!(outcome.dropped_names, vec!["a".to_string()]);
        assert_eq!(outcome.weight_overrides.get("a"), Some(&7));
        assert_eq!(outcome.weight_overrides.get("b"), Some(&11));
    }
}
