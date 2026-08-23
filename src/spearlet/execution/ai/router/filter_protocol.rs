use crate::proto::spearlet::{
    Candidate, CandidateRuntimeHints, DecisionAction, FilterRequest, FilterResponse,
    Operation as ProtoOperation, RequestSignals, Requirements, RoutingHints,
};
use crate::spearlet::config::RouterGrpcFilterStreamConfig;
use crate::spearlet::execution::ai::ir::{CanonicalRequestEnvelope, Operation};
use crate::spearlet::execution::ai::router::registry::{BackendInstance, Hosting};
use std::collections::HashMap;

fn to_proto_operation(op: &Operation) -> i32 {
    match op {
        Operation::ChatCompletions => ProtoOperation::ChatCompletions as i32,
        Operation::Embeddings => ProtoOperation::Embeddings as i32,
        Operation::ImageGeneration => ProtoOperation::ImageGeneration as i32,
        Operation::SpeechToText => ProtoOperation::SpeechToText as i32,
        Operation::TextToSpeech => ProtoOperation::TextToSpeech as i32,
    }
}

fn to_operation_name(op: &Operation) -> &'static str {
    match op {
        Operation::ChatCompletions => "chat_completions",
        Operation::Embeddings => "embeddings",
        Operation::ImageGeneration => "image_generation",
        Operation::SpeechToText => "speech_to_text",
        Operation::TextToSpeech => "text_to_speech",
    }
}

pub fn requested_model(req: &CanonicalRequestEnvelope) -> Option<&str> {
    match &req.payload {
        crate::spearlet::execution::ai::ir::Payload::ChatCompletions(p) => Some(p.model.as_str()),
        crate::spearlet::execution::ai::ir::Payload::Embeddings(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::ImageGeneration(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::SpeechToText(p) => p.model.as_deref(),
        crate::spearlet::execution::ai::ir::Payload::TextToSpeech(p) => p.model.as_deref(),
    }
}

fn build_signals(req: &CanonicalRequestEnvelope) -> RequestSignals {
    match &req.payload {
        crate::spearlet::execution::ai::ir::Payload::ChatCompletions(p) => {
            let mut approx: u32 = 0;
            for m in p.messages.iter() {
                if let Some(s) = m.content.as_str() {
                    approx = approx.saturating_add(s.len().min(u32::MAX as usize) as u32);
                }
            }
            let uses_tools = !p.tools.is_empty();
            let uses_json_schema = p
                .params
                .get("response_format")
                .and_then(|v| v.get("json_schema"))
                .is_some();
            RequestSignals {
                model: p.model.clone(),
                message_count: p.messages.len().min(u32::MAX as usize) as u32,
                approx_text_bytes: approx,
                uses_tools,
                uses_json_schema,
                ..Default::default()
            }
        }
        crate::spearlet::execution::ai::ir::Payload::SpeechToText(p) => RequestSignals {
            model: p.model.clone().unwrap_or_default(),
            ..Default::default()
        },
        _ => RequestSignals {
            model: requested_model(req).unwrap_or_default().to_string(),
            ..Default::default()
        },
    }
}

fn build_candidates(candidates: &[&BackendInstance], max_candidates_sent: usize) -> Vec<Candidate> {
    let mut proto_candidates: Vec<Candidate> = Vec::new();
    for c in candidates.iter().take(max_candidates_sent.max(1)) {
        let ops = c
            .capabilities
            .ops
            .iter()
            .map(to_operation_name)
            .map(|s| s.to_string())
            .collect();
        proto_candidates.push(Candidate {
            name: c.spec.name.clone(),
            kind: c.spec.kind.clone(),
            base_url: c.spec.base_url.clone(),
            model: c.spec.model.clone(),
            weight: c.spec.weight,
            priority: c.spec.priority,
            ops,
            features: c.capabilities.features.clone(),
            transports: c.capabilities.transports.clone(),
            is_local: c.hosting == Hosting::Local,
            runtime: Some(CandidateRuntimeHints::default()),
        });
    }
    proto_candidates
}

/// Build the proto filter request from canonical request and runtime candidates.
/// 基于规范化请求和运行时候选构建 proto filter request。
pub fn build_filter_request(
    req: &CanonicalRequestEnvelope,
    candidates: &[&BackendInstance],
    config: &RouterGrpcFilterStreamConfig,
    correlation_id: String,
    decision_timeout_ms: u64,
) -> FilterRequest {
    let routing = RoutingHints {
        backend: req.routing.backend.clone().unwrap_or_default(),
        allowlist: req.routing.allowlist.clone(),
        denylist: req.routing.denylist.clone(),
        requested_model: requested_model(req).unwrap_or_default().to_string(),
    };

    let requirements = Requirements {
        required_features: req.requirements.required_features.clone(),
        required_transports: req.requirements.required_transports.clone(),
    };

    let (content_type, payload) = if config.content_fetch_enabled {
        match serde_json::to_vec(&req.payload) {
            Ok(mut b) => {
                let max = config.content_fetch_max_bytes.max(1);
                if b.len() > max {
                    b.clear();
                    ("".to_string(), Vec::new())
                } else {
                    ("application/json".to_string(), b)
                }
            }
            Err(_) => ("".to_string(), Vec::new()),
        }
    } else {
        ("".to_string(), Vec::new())
    };

    FilterRequest {
        correlation_id,
        request_id: req.request_id.clone(),
        operation: to_proto_operation(&req.operation),
        decision_timeout_ms: decision_timeout_ms.min(u32::MAX as u64) as u32,
        meta: req.meta.clone(),
        routing: Some(routing),
        requirements: Some(requirements),
        signals: Some(build_signals(req)),
        candidates: build_candidates(candidates, config.max_candidates_sent),
        request_content_type: content_type,
        request_payload: payload,
    }
}

#[derive(Debug, Clone, Default)]
pub struct FilterTrace {
    pub decision_id: Option<String>,
    pub dropped: Vec<String>,
    pub weight_overrides: Vec<(String, u32)>,
    pub priority_overrides: Vec<(String, i32)>,
    pub reason_codes_by_candidate: HashMap<String, Vec<String>>,
    pub final_action: Option<FinalActionTrace>,
}

#[derive(Debug, Clone, Default)]
pub struct FinalActionTrace {
    pub reject_request: bool,
    pub reject_code: Option<String>,
    pub force_backend: Option<String>,
}

/// Build a compact trace from the filter response for logging/debugging.
/// 从 filter response 构建紧凑 trace，用于日志与调试。
pub fn trace_from_response(resp: &FilterResponse, max_debug_kv: usize) -> FilterTrace {
    let mut trace = FilterTrace::default();
    if !resp.decision_id.trim().is_empty() {
        trace.decision_id = Some(resp.decision_id.clone());
    }
    let mut reason_map: HashMap<String, Vec<String>> = HashMap::new();
    for d in resp.decisions.iter() {
        if d.action == DecisionAction::Drop as i32 {
            trace.dropped.push(d.name.clone());
        }
        if let Some(w) = d.weight_override {
            trace.weight_overrides.push((d.name.clone(), w));
        }
        if let Some(p) = d.priority_override {
            trace.priority_overrides.push((d.name.clone(), p));
        }
        if !d.reason_codes.is_empty() {
            reason_map.insert(d.name.clone(), d.reason_codes.clone());
        }
    }
    trace.reason_codes_by_candidate = reason_map;
    if let Some(fa) = resp.final_action.as_ref() {
        let mut fat = FinalActionTrace::default();
        fat.reject_request = fa.reject_request;
        if !fa.reject_code.trim().is_empty() {
            fat.reject_code = Some(fa.reject_code.clone());
        }
        if !fa.force_backend.trim().is_empty() {
            fat.force_backend = Some(fa.force_backend.clone());
        }
        trace.final_action = Some(fat);
    }
    let _ = max_debug_kv;
    trace
}
