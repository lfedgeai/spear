pub mod candidate_explainer;
pub mod capabilities;
pub mod builder;
pub mod debug;
pub mod filter_decision;
pub mod filter_inflight;
pub mod filter_protocol;
pub mod filter_worker;
pub mod grpc_filter_stream;
pub mod policy;
pub mod registry;
pub mod selection;

use std::collections::HashMap;
use std::sync::Arc;

use crate::spearlet::ai::backend_assembly::{
    build_instance_from_spec,
};
use crate::spearlet::execution::ai::ir::{CanonicalError, CanonicalRequestEnvelope};
use crate::spearlet::execution::ai::router::candidate_explainer::build_no_candidate_error;
use crate::spearlet::execution::ai::router::filter_decision::{
    apply_filter_response, reject_error_from_response,
};
use crate::spearlet::execution::ai::router::policy::SelectionPolicy;
use crate::spearlet::execution::ai::router::registry::{BackendInstance, BackendRegistry};
use crate::spearlet::execution::ai::router::selection::{select_backend, snapshot_candidates};
use crate::spearlet::ai::dynamic_backend_registry::{
    global_dynamic_backends, DynamicBackendRegistry, DynamicBackendSource,
};
use parking_lot::RwLock;
use tracing::debug;

fn requested_model(req: &CanonicalRequestEnvelope) -> Option<&str> {
    match &req.payload {
        crate::spearlet::execution::ai::ir::Payload::ChatCompletions(p) => Some(p.model.as_str()),
        crate::spearlet::execution::ai::ir::Payload::Embeddings(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::ImageGeneration(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::SpeechToText(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::TextToSpeech(p) => p.model.as_deref(),
    }
}

fn fixed_model(inst: &BackendInstance) -> Option<&str> {
    let m = inst.spec.model.trim();
    if m.is_empty() {
        None
    } else {
        Some(m)
    }
}

#[derive(Clone)]
pub struct Router {
    registry: BackendRegistry,
    policy: SelectionPolicy,
    grpc_filter_stream: Option<Arc<grpc_filter_stream::RouterFilterStreamHub>>,
    dynamic_backends: DynamicBackendRegistry,
    dynamic_cache: Arc<RwLock<ManagedBackendCache>>,
}

struct ManagedBackendCache {
    revision: u64,
    instances: Arc<Vec<BackendInstance>>,
}

impl Router {
    pub fn new(registry: BackendRegistry, policy: SelectionPolicy) -> Self {
        Self {
            registry,
            policy,
            grpc_filter_stream: None,
            dynamic_backends: global_dynamic_backends(),
            dynamic_cache: Arc::new(RwLock::new(ManagedBackendCache {
                revision: 0,
                instances: Arc::new(Vec::new()),
            })),
        }
    }

    pub fn new_with_filter(
        registry: BackendRegistry,
        policy: SelectionPolicy,
        grpc_filter_stream: Option<Arc<grpc_filter_stream::RouterFilterStreamHub>>,
    ) -> Self {
        if let Some(h) = grpc_filter_stream.as_ref() {
            h.start_background();
        }
        Self {
            registry,
            policy,
            grpc_filter_stream,
            dynamic_backends: global_dynamic_backends(),
            dynamic_cache: Arc::new(RwLock::new(ManagedBackendCache {
                revision: 0,
                instances: Arc::new(Vec::new()),
            })),
        }
    }

    pub(crate) fn dynamic_instances(&self) -> Arc<Vec<BackendInstance>> {
        let rev = self.dynamic_backends.revision();
        {
            let c = self.dynamic_cache.read();
            if c.revision == rev {
                return c.instances.clone();
            }
        }

        let merged = self.dynamic_backends.list_merged_sorted(&[
            DynamicBackendSource::AiControlPlane,
        ]);
        let mut out: Vec<BackendInstance> = Vec::new();
        for b in merged.into_iter() {
            if let Some(inst) = managed_backend_info_to_instance(b) {
                out.push(inst);
            }
        }
        let instances = Arc::new(out);
        let mut c = self.dynamic_cache.write();
        c.revision = rev;
        c.instances = instances.clone();
        instances
    }

    pub fn route<'a>(
        &'a self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<BackendInstance, CanonicalError> {
        let dyns = self.dynamic_instances();
        let instances = self.collect_instances(&dyns);
        let mut candidates = self.collect_candidates(req, &instances);
        self.apply_routing_constraints(req, &mut candidates);
        let weight_overrides = self.apply_external_filter(req, &mut candidates)?;
        self.apply_model_binding(req, &mut candidates)?;

        if candidates.is_empty() {
            return Err(build_no_candidate_error(req, &instances));
        }

        let snapshot = snapshot_candidates(&candidates);
        let selected = select_backend(self.policy.clone(), req, candidates, &weight_overrides)?;
        self.log_selection(req, &selected, snapshot.count, &snapshot.names);
        Ok(selected)
    }

    /// Collect one merged view of static and dynamic backends.
    /// 收集静态与动态 backend 的合并视图。
    fn collect_instances<'a>(&'a self, dyns: &'a Arc<Vec<BackendInstance>>) -> Vec<&'a BackendInstance> {
        let mut by_name: HashMap<String, &BackendInstance> = HashMap::new();
        for inst in self.registry.instances().iter() {
            by_name.insert(inst.spec.name.clone(), inst);
        }
        for inst in dyns.iter() {
            by_name.insert(inst.spec.name.clone(), inst);
        }
        by_name.into_values().collect()
    }

    /// Apply operation/feature/transport filters to build the initial candidate set.
    /// 基于 operation/feature/transport 约束构建初始候选集合。
    fn collect_candidates<'a>(
        &self,
        req: &CanonicalRequestEnvelope,
        instances: &[&'a BackendInstance],
    ) -> Vec<&'a BackendInstance> {
        instances
            .iter()
            .copied()
            .filter(|inst| inst.capabilities.supports_operation(&req.operation))
            .filter(|inst| {
                req.requirements
                    .required_features
                    .iter()
                    .all(|f| inst.capabilities.has_feature(f))
            })
            .filter(|inst| {
                req.requirements
                    .required_transports
                    .iter()
                    .all(|t| inst.capabilities.transports.iter().any(|x| x == t))
            })
            .collect()
    }

    /// Apply request-level routing constraints such as explicit backend, allowlist, and denylist.
    /// 应用请求级路由约束，例如显式 backend、allowlist 与 denylist。
    fn apply_routing_constraints<'a>(
        &self,
        req: &CanonicalRequestEnvelope,
        candidates: &mut Vec<&'a BackendInstance>,
    ) {
        if let Some(name) = req.routing.backend.as_ref() {
            candidates.retain(|c| c.spec.name == *name);
        }

        if !req.routing.allowlist.is_empty() {
            candidates.retain(|c| req.routing.allowlist.iter().any(|x| x == &c.spec.name));
        }

        if !req.routing.denylist.is_empty() {
            candidates.retain(|c| !req.routing.denylist.iter().any(|x| x == &c.spec.name));
        }
    }

    /// Ask the optional external filter to refine candidates and provide weight overrides.
    /// 调用可选的外部 filter 对候选做进一步筛选，并返回权重覆盖值。
    fn apply_external_filter<'a>(
        &self,
        req: &CanonicalRequestEnvelope,
        candidates: &mut Vec<&'a BackendInstance>,
    ) -> Result<HashMap<String, u32>, CanonicalError> {
        let mut weight_overrides: HashMap<String, u32> = HashMap::new();
        let Some(hub) = self.grpc_filter_stream.as_ref() else {
            return Ok(weight_overrides);
        };

        let decision_budget_ms = req
            .timeout_ms
            .map(|t| t.min(hub.config.decision_timeout_ms))
            .unwrap_or(hub.config.decision_timeout_ms);
        match hub.try_filter_candidates_blocking(req, candidates, decision_budget_ms) {
            Ok((resp, _trace)) => {
                if let Some(err) = reject_error_from_response(&req.operation, &resp) {
                    return Err(err);
                }
                let outcome = apply_filter_response(&resp, candidates);
                for (name, weight) in outcome.weight_overrides {
                    weight_overrides.insert(name, weight);
                }
            }
            Err(e) => {
                if !hub.config.fail_open {
                    return Err(e);
                }
            }
        }
        Ok(weight_overrides)
    }

    /// Restrict candidates by the requested model when fixed-model backends are present.
    /// 当存在固定模型 backend 时，按请求模型进一步收紧候选集合。
    fn apply_model_binding<'a>(
        &self,
        req: &CanonicalRequestEnvelope,
        candidates: &mut Vec<&'a BackendInstance>,
    ) -> Result<(), CanonicalError> {
        let requested_model_val = requested_model(req)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let Some(model) = requested_model_val else {
            return Ok(());
        };
        if !candidates.iter().any(|c| fixed_model(c).is_some()) {
            return Ok(());
        }

        let exact: Vec<&BackendInstance> = candidates
            .iter()
            .copied()
            .filter(|c| fixed_model(c) == Some(model))
            .collect();
        if exact.is_empty() {
            let mut available_models: Vec<String> = candidates
                .iter()
                .filter_map(|c| fixed_model(c).map(|m| m.to_string()))
                .collect();
            available_models.sort();
            available_models.dedup();
            let msg = format!(
                "no candidate backend for model={:?}: available_models={:?}",
                model, available_models
            );
            return Err(CanonicalError {
                code: "no_candidate_backend".to_string(),
                message: msg,
                retryable: false,
                operation: Some(req.operation.clone()),
            });
        }
        *candidates = exact;
        Ok(())
    }

    /// Emit a compact routing decision log after the final backend has been chosen.
    /// 在最终 backend 选定后输出简洁的路由决策日志。
    fn log_selection(
        &self,
        req: &CanonicalRequestEnvelope,
        selected: &BackendInstance,
        candidate_count: usize,
        candidate_names: &[&str],
    ) {
        debug!(
            op = ?req.operation,
            model = requested_model(req),
            routing_backend = req.routing.backend.as_deref(),
            allowlist_len = req.routing.allowlist.len(),
            denylist_len = req.routing.denylist.len(),
            candidate_count,
            candidate_names = ?candidate_names,
            selected_backend = %selected.spec.name,
            selected_model = if selected.spec.model.trim().is_empty() { None } else { Some(selected.spec.model.as_str()) },
            "router selected backend"
        );
    }
}

fn managed_backend_info_to_instance(b: crate::proto::sms::BackendInfo) -> Option<BackendInstance> {
    let spec = b.spec?;
    let resolver = crate::spearlet::ai::credential_resolver::global();
    build_instance_from_spec(spec, resolver.as_ref(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::config::RouterGrpcFilterStreamConfig;
    use crate::spearlet::execution::ai::backends::{
        stub::StubBackendAdapter, KIND_OLLAMA_CHAT, KIND_OPENAI_REALTIME_WS,
    };
    use crate::spearlet::execution::ai::ir::{
        ChatCompletionsPayload, ChatMessage, Operation, Payload, RoutingHints, SpeechToTextPayload,
    };
    use crate::spearlet::execution::ai::streaming::StreamingPlan;
    use crate::spearlet::execution::ai::router::capabilities::Capabilities;
    use crate::spearlet::execution::ai::router::grpc_filter_stream::RouterFilterStreamHub;
    use crate::spearlet::execution::ai::router::policy::SelectionPolicy;
    use crate::spearlet::execution::ai::router::registry::Hosting;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn mk_inst(
        name: &str,
        kind: &str,
        base_url: &str,
        hosting: Hosting,
        model: &str,
        provider: &str,
    ) -> BackendInstance {
        let hosting_proto = match hosting {
            Hosting::Local => crate::proto::sms::BackendHosting::NodeLocal as i32,
            Hosting::Remote => crate::proto::sms::BackendHosting::Remote as i32,
            Hosting::Unknown => crate::proto::sms::BackendHosting::Unspecified as i32,
        };
        BackendInstance {
            spec: crate::proto::sms::BackendSpec {
                name: name.to_string(),
                kind: kind.to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["http".to_string()],
                weight: 100,
                priority: 0,
                base_url: base_url.to_string(),
                provider: provider.to_string(),
                model: model.to_string(),
                hosting: hosting_proto,
                credential_ref: String::new(),
                origin: crate::proto::sms::BackendOrigin::StaticConfig as i32,
                deployment_id: String::new(),
            },
            hosting,
            capabilities: Capabilities {
                ops: vec![Operation::ChatCompletions],
                features: vec![],
                transports: vec!["http".to_string()],
            },
            adapter: Arc::new(StubBackendAdapter::new(name)),
        }
    }

    fn chat_req(model: &str) -> CanonicalRequestEnvelope {
        CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: Operation::ChatCompletions,
            meta: HashMap::new(),
            routing: RoutingHints::default(),
            requirements: Default::default(),
            timeout_ms: None,
            payload: Payload::ChatCompletions(ChatCompletionsPayload {
                model: model.to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: Value::String("hi".to_string()),
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

    fn stt_req(model: Option<&str>) -> CanonicalRequestEnvelope {
        CanonicalRequestEnvelope {
            version: 1,
            request_id: "r-stt".to_string(),
            operation: Operation::SpeechToText,
            meta: HashMap::new(),
            routing: RoutingHints::default(),
            requirements: crate::spearlet::execution::ai::ir::Requirements {
                required_features: vec![],
                required_transports: vec!["websocket".to_string()],
            },
            timeout_ms: None,
            payload: Payload::SpeechToText(SpeechToTextPayload {
                model: model.map(str::to_string),
            }),
            extra: HashMap::new(),
        }
    }

    #[test]
    fn managed_backend_info_supports_openai_realtime_ws() {
        let inst = managed_backend_info_to_instance(crate::proto::sms::BackendInfo {
            spec: Some(crate::proto::sms::BackendSpec {
                name: "rt-ws".to_string(),
                kind: KIND_OPENAI_REALTIME_WS.to_string(),
                operations: vec!["speech_to_text".to_string()],
                features: vec![],
                transports: vec!["websocket".to_string()],
                weight: 100,
                priority: 0,
                base_url: "https://api.openai.com/v1".to_string(),
                provider: String::new(),
                model: String::new(),
                hosting: crate::proto::sms::BackendHosting::Remote as i32,
                credential_ref: String::new(),
                origin: crate::proto::sms::BackendOrigin::Sms as i32,
                deployment_id: String::new(),
            }),
            status: crate::proto::sms::BackendStatus::Available as i32,
            status_reason: String::new(),
        })
        .expect("dynamic realtime backend should be supported");

        let plan = inst.adapter.streaming_plan(&stt_req(None)).unwrap();
        let StreamingPlan::Websocket(ws) = plan;
        assert!(ws.websocket.url.starts_with("wss://"));
        assert_eq!(inst.spec.kind, KIND_OPENAI_REALTIME_WS);
    }

    #[test]
    fn managed_backend_info_supports_ollama_chat() {
        let inst = managed_backend_info_to_instance(crate::proto::sms::BackendInfo {
            spec: Some(crate::proto::sms::BackendSpec {
                name: "ollama-dyn".to_string(),
                kind: KIND_OLLAMA_CHAT.to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["http".to_string()],
                weight: 100,
                priority: 0,
                base_url: "http://127.0.0.1:11434".to_string(),
                provider: String::new(),
                model: "llama3.1:8b".to_string(),
                hosting: crate::proto::sms::BackendHosting::NodeLocal as i32,
                credential_ref: String::new(),
                origin: crate::proto::sms::BackendOrigin::Sms as i32,
                deployment_id: String::new(),
            }),
            status: crate::proto::sms::BackendStatus::Available as i32,
            status_reason: String::new(),
        })
        .expect("dynamic ollama backend should be supported");

        assert_eq!(inst.adapter.name(), "ollama-dyn");
        assert_eq!(inst.hosting, Hosting::Local);
        assert_eq!(inst.spec.model, "llama3.1:8b");
        assert_eq!(inst.spec.kind, KIND_OLLAMA_CHAT);
    }

    #[test]
    fn test_route_prefers_model_bound_backend() {
        let a = mk_inst(
            "openai",
            "openai_chat_completion",
            "https://api.openai.com/v1",
            Hosting::Remote,
            "gpt-4o-mini",
            "openai",
        );
        let b = mk_inst(
            "ollama",
            "ollama_chat",
            "http://127.0.0.1:11434",
            Hosting::Local,
            "gemma3:1b",
            "ollama",
        );
        let router = Router::new(
            BackendRegistry::new(vec![a, b]),
            SelectionPolicy::WeightedRandom,
        );
        let req = chat_req("gemma3:1b");
        let inst = router.route(&req).unwrap();
        assert_eq!(inst.spec.name, "ollama");
    }

    #[test]
    fn test_route_errors_on_unknown_model_when_model_bound_exists() {
        let a = mk_inst(
            "ollama",
            "ollama_chat",
            "http://127.0.0.1:11434",
            Hosting::Local,
            "gemma3:1b",
            "ollama",
        );
        let router = Router::new(
            BackendRegistry::new(vec![a]),
            SelectionPolicy::WeightedRandom,
        );
        let req = chat_req("gpt-4o-mini");
        let err = match router.route(&req) {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert_eq!(err.code, "no_candidate_backend");
    }

    #[test]
    fn test_route_fail_open_when_filter_unavailable() {
        let a = mk_inst(
            "stub",
            "stub",
            "",
            Hosting::Local,
            "gpt-4o-mini",
            "internal",
        );

        let hub = Arc::new(RouterFilterStreamHub::new(RouterGrpcFilterStreamConfig {
            enabled: true,
            fail_open: true,
            ..Default::default()
        }));

        let router = Router::new_with_filter(
            BackendRegistry::new(vec![a]),
            SelectionPolicy::WeightedRandom,
            Some(hub),
        );
        let req = chat_req("gpt-4o-mini");
        let inst = router.route(&req).unwrap();
        assert_eq!(inst.spec.name, "stub");
    }

    #[test]
    fn test_route_fail_closed_when_filter_unavailable() {
        let a = mk_inst(
            "stub",
            "stub",
            "",
            Hosting::Local,
            "gpt-4o-mini",
            "internal",
        );

        let hub = Arc::new(RouterFilterStreamHub::new(RouterGrpcFilterStreamConfig {
            enabled: true,
            fail_open: false,
            ..Default::default()
        }));

        let router = Router::new_with_filter(
            BackendRegistry::new(vec![a]),
            SelectionPolicy::WeightedRandom,
            Some(hub),
        );
        let req = chat_req("gpt-4o-mini");
        let err = match router.route(&req) {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        assert_eq!(err.code, "router_filter_unavailable");
    }

    #[test]
    fn test_route_applies_backend_allowlist_and_denylist() {
        let a = mk_inst(
            "openai",
            "openai_chat_completion",
            "https://api.openai.com/v1",
            Hosting::Remote,
            "gpt-4o-mini",
            "openai",
        );
        let b = mk_inst(
            "ollama",
            "ollama_chat",
            "http://127.0.0.1:11434",
            Hosting::Local,
            "gemma3:1b",
            "ollama",
        );
        let router = Router::new(
            BackendRegistry::new(vec![a, b]),
            SelectionPolicy::WeightedRandom,
        );

        let mut req = chat_req("gemma3:1b");
        req.routing.allowlist = vec!["openai".to_string(), "ollama".to_string()];
        req.routing.denylist = vec!["openai".to_string()];
        req.routing.backend = Some("ollama".to_string());

        let inst = router.route(&req).unwrap();
        assert_eq!(inst.spec.name, "ollama");
    }
}
