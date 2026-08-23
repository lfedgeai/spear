pub mod backends;
pub mod engine_holder;
pub mod ir;
pub mod media_ref;
pub mod normalize;
pub mod router;
pub mod streaming;

use std::fmt;
use std::sync::Arc;

use crate::spearlet::execution::ai::ir::{
    CanonicalRequestEnvelope, CanonicalResponseEnvelope, Payload,
};
use crate::spearlet::execution::ai::router::debug::RouterDebugSnapshot;
use crate::spearlet::execution::ai::router::Router;
use crate::spearlet::execution::ai::streaming::StreamingInvocation;

#[derive(Clone)]
pub struct AiEngine {
    router: Arc<Router>,
}

fn has_missing_model(req: &CanonicalRequestEnvelope) -> bool {
    match &req.payload {
        Payload::ChatCompletions(p) => p.model.trim().is_empty(),
        Payload::Embeddings(p) => p
            .model
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true),
        Payload::ImageGeneration(p) => p
            .model
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true),
        Payload::SpeechToText(p) => p
            .model
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true),
        Payload::TextToSpeech(p) => p
            .model
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true),
    }
}

fn with_default_model(
    req: &CanonicalRequestEnvelope,
    default_model: Option<&str>,
) -> Option<CanonicalRequestEnvelope> {
    let m = default_model.map(|s| s.trim()).filter(|s| !s.is_empty())?;
    if !has_missing_model(req) {
        return None;
    }

    let mut out = req.clone();
    match &mut out.payload {
        Payload::ChatCompletions(p) => {
            if p.model.trim().is_empty() {
                p.model = m.to_string();
            }
        }
        Payload::Embeddings(p) => {
            if p.model.as_deref().map(|s| s.trim()).unwrap_or("") != m {
                p.model = Some(m.to_string());
            }
        }
        Payload::ImageGeneration(p) => {
            if p.model.as_deref().map(|s| s.trim()).unwrap_or("") != m {
                p.model = Some(m.to_string());
            }
        }
        Payload::SpeechToText(p) => {
            if p.model.as_deref().map(|s| s.trim()).unwrap_or("") != m {
                p.model = Some(m.to_string());
            }
        }
        Payload::TextToSpeech(p) => {
            if p.model.as_deref().map(|s| s.trim()).unwrap_or("") != m {
                p.model = Some(m.to_string());
            }
        }
    }
    Some(out)
}

impl fmt::Debug for AiEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiEngine").finish()
    }
}

impl AiEngine {
    pub fn new(router: Router) -> Self {
        Self {
            router: Arc::new(router),
        }
    }

    pub fn debug_snapshot(&self) -> RouterDebugSnapshot {
        crate::spearlet::execution::ai::router::debug::snapshot(self.router.as_ref())
    }

    pub fn invoke(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<CanonicalResponseEnvelope, crate::spearlet::execution::ExecutionError> {
        req.validate_basic().map_err(|e| {
            crate::spearlet::execution::ExecutionError::InvalidRequest { message: e.message }
        })?;
        let inst = self.router.route(req).map_err(|e| {
            crate::spearlet::execution::ExecutionError::NotSupported {
                operation: e.message,
            }
        })?;
        let req2 = with_default_model(
            req,
            if inst.spec.model.trim().is_empty() {
                None
            } else {
                Some(inst.spec.model.as_str())
            },
        );
        let req_used = req2.as_ref().unwrap_or(req);
        inst.adapter.invoke(req_used).map_err(|e| {
            crate::spearlet::execution::ExecutionError::RuntimeError { message: e.message }
        })
    }

    pub fn invoke_streaming(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<StreamingInvocation, crate::spearlet::execution::ExecutionError> {
        req.validate_basic().map_err(|e| {
            crate::spearlet::execution::ExecutionError::InvalidRequest { message: e.message }
        })?;
        let inst = self.router.route(req).map_err(|e| {
            crate::spearlet::execution::ExecutionError::NotSupported {
                operation: e.message,
            }
        })?;

        let req2 = with_default_model(
            req,
            if inst.spec.model.trim().is_empty() {
                None
            } else {
                Some(inst.spec.model.as_str())
            },
        );
        let req_used = req2.as_ref().unwrap_or(req);
        let plan = inst.adapter.streaming_plan(req_used).map_err(|e| {
            crate::spearlet::execution::ExecutionError::NotSupported {
                operation: e.message,
            }
        })?;
        Ok(StreamingInvocation {
            backend: inst.spec.name.clone(),
            plan,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::ai::backends::stub::StubBackendAdapter;
    use crate::spearlet::execution::ai::ir::{
        ChatCompletionsPayload, ChatMessage, Operation, Requirements, ResultPayload, RoutingHints,
    };
    use crate::spearlet::execution::ai::router::capabilities::Capabilities;
    use crate::spearlet::execution::ai::router::policy::SelectionPolicy;
    use crate::spearlet::execution::ai::router::registry::{
        BackendInstance, BackendRegistry, Hosting,
    };
    use crate::spearlet::execution::ai::router::Router;
    use serde_json::Value;
    use std::collections::HashMap;

    #[test]
    fn test_invoke_fills_default_model_from_backend_instance() {
        let inst = BackendInstance {
            spec: crate::proto::sms::BackendSpec {
                name: "stub".to_string(),
                kind: "stub".to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["in_process".to_string()],
                weight: 100,
                priority: 0,
                base_url: String::new(),
                provider: "internal".to_string(),
                model: "default-model".to_string(),
                hosting: crate::proto::sms::BackendHosting::NodeLocal as i32,
                credential_ref: String::new(),
                origin: crate::proto::sms::BackendOrigin::StaticConfig as i32,
                deployment_id: String::new(),
            },
            hosting: Hosting::Local,
            capabilities: Capabilities {
                ops: vec![Operation::ChatCompletions],
                features: vec![],
                transports: vec!["in_process".to_string()],
            },
            adapter: Arc::new(StubBackendAdapter::new("stub")),
        };

        let router = Router::new(
            BackendRegistry::new(vec![inst]),
            SelectionPolicy::WeightedRandom,
        );
        let ai = AiEngine::new(router);

        let req = CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: Operation::ChatCompletions,
            meta: HashMap::new(),
            routing: RoutingHints::default(),
            requirements: Requirements::default(),
            timeout_ms: None,
            payload: Payload::ChatCompletions(ChatCompletionsPayload {
                model: "".to_string(),
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
        };

        let resp = ai.invoke(&req).unwrap();
        let ResultPayload::Payload(v) = resp.result else {
            panic!("expected payload");
        };
        assert_eq!(
            v.get("model").and_then(|x| x.as_str()),
            Some("default-model")
        );
    }
}
