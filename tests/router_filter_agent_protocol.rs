//! Router filter protocol tests (server-side filter; Spearlet dials it).
//! Router filter 协议测试（Filter 作为服务端；Spearlet 主动连接）。

use std::sync::Arc;

use tonic::{Request, Response, Status};

use spear_next::proto::spearlet::router_filter_service_server::{
    RouterFilterService, RouterFilterServiceServer,
};
use spear_next::proto::spearlet::{
    CandidateDecision, DecisionAction, FilterRequest, FilterResponse,
};
use spear_next::spearlet::config::RouterGrpcFilterStreamConfig;
use spear_next::spearlet::execution::ai::backends::stub::StubBackendAdapter;
use spear_next::spearlet::execution::ai::ir::{
    CanonicalRequestEnvelope, ChatCompletionsPayload, ChatMessage, Operation, Payload, RoutingHints,
};
use spear_next::spearlet::execution::ai::router::capabilities::Capabilities;
use spear_next::spearlet::execution::ai::router::grpc_filter_stream::RouterFilterStreamHub;
use spear_next::spearlet::execution::ai::router::policy::SelectionPolicy;
use spear_next::spearlet::execution::ai::router::registry::{
    BackendInstance, BackendRegistry, Hosting,
};
use spear_next::spearlet::execution::ai::router::Router;

const BLACKLIST_KEYWORDS: &[&str] = &[
    "secret",
    "confidential",
    "apikey",
    "password",
    "机密",
    "敏感",
];

fn contains_blacklist_keyword(s: &str) -> Option<&'static str> {
    let lower = s.to_ascii_lowercase();
    for &kw in BLACKLIST_KEYWORDS {
        if kw.is_ascii() {
            if lower.contains(kw) {
                return Some(kw);
            }
        } else if s.contains(kw) {
            return Some(kw);
        }
    }
    None
}

#[derive(Debug, Clone)]
struct TestKeywordFilterSvc;

#[tonic::async_trait]
impl RouterFilterService for TestKeywordFilterSvc {
    async fn filter(
        &self,
        request: Request<FilterRequest>,
    ) -> Result<Response<FilterResponse>, Status> {
        let r = request.into_inner();
        let text = String::from_utf8_lossy(&r.request_payload).to_string();
        let hit = contains_blacklist_keyword(&text);
        let mut decisions = Vec::with_capacity(r.candidates.len());
        for c in r.candidates.iter() {
            let action = if hit.is_some() && !c.is_local {
                DecisionAction::Drop as i32
            } else {
                DecisionAction::Keep as i32
            };
            decisions.push(CandidateDecision {
                name: c.name.clone(),
                action,
                weight_override: None,
                priority_override: None,
                score: None,
                reason_codes: Vec::new(),
            });
        }
        let mut debug = std::collections::HashMap::new();
        if let Some(kw) = hit {
            debug.insert("keyword_filter_hit".to_string(), kw.to_string());
        }
        Ok(Response::new(FilterResponse {
            correlation_id: r.correlation_id,
            decision_id: "test_keyword_filter".to_string(),
            decisions,
            final_action: None,
            debug,
        }))
    }
}

#[tokio::test]
async fn protocol_keyword_filter_server_drops_remote_candidates() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let server = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(RouterFilterServiceServer::new(TestKeywordFilterSvc))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    let cfg = RouterGrpcFilterStreamConfig {
        enabled: true,
        addr: addr.to_string(),
        decision_timeout_ms: 1500,
        fail_open: false,
        content_fetch_enabled: true,
        content_fetch_max_bytes: 64 * 1024,
        ..Default::default()
    };
    let hub = Arc::new(RouterFilterStreamHub::new(cfg));
    hub.start_background();

    let local = BackendInstance {
        spec: spear_next::proto::sms::BackendSpec {
            name: "stub".to_string(),
            kind: "stub".to_string(),
            operations: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 100,
            priority: 0,
            base_url: String::new(),
            provider: "internal".to_string(),
            model: "gpt".to_string(),
            hosting: spear_next::proto::sms::BackendHosting::NodeLocal as i32,
            credential_ref: String::new(),
            origin: spear_next::proto::sms::BackendOrigin::StaticConfig as i32,
            deployment_id: String::new(),
        },
        hosting: Hosting::Local,
        capabilities: Capabilities {
            ops: vec![Operation::ChatCompletions],
            features: vec![],
            transports: vec!["http".to_string()],
        },
        adapter: Arc::new(StubBackendAdapter::new("stub")),
    };
    let remote = BackendInstance {
        spec: spear_next::proto::sms::BackendSpec {
            name: "openai".to_string(),
            kind: "stub".to_string(),
            operations: vec!["chat_completions".to_string()],
            features: vec![],
            transports: vec!["http".to_string()],
            weight: 100,
            priority: 0,
            base_url: "https://api.openai.com/v1".to_string(),
            provider: "openai".to_string(),
            model: "gpt".to_string(),
            hosting: spear_next::proto::sms::BackendHosting::Remote as i32,
            credential_ref: String::new(),
            origin: spear_next::proto::sms::BackendOrigin::StaticConfig as i32,
            deployment_id: String::new(),
        },
        hosting: Hosting::Remote,
        capabilities: Capabilities {
            ops: vec![Operation::ChatCompletions],
            features: vec![],
            transports: vec!["http".to_string()],
        },
        adapter: Arc::new(StubBackendAdapter::new("openai")),
    };

    let router = Router::new_with_filter(
        BackendRegistry::new(vec![local.clone(), remote]),
        SelectionPolicy::WeightedRandom,
        Some(hub),
    );

    let req = CanonicalRequestEnvelope {
        version: 1,
        request_id: "req-1".to_string(),
        operation: Operation::ChatCompletions,
        meta: std::collections::HashMap::new(),
        routing: RoutingHints::default(),
        requirements: Default::default(),
        timeout_ms: Some(1500),
        payload: Payload::ChatCompletions(ChatCompletionsPayload {
            model: "gpt".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String("my secret is 123".to_string()),
                tool_call_id: None,
                tool_calls: None,
                name: None,
            }],
            tools: vec![],
            params: std::collections::HashMap::new(),
        }),
        extra: std::collections::HashMap::new(),
    };

    let router = Arc::new(router);
    let req_cloned = req.clone();
    let router_cloned = router.clone();
    let inst = tokio::task::spawn_blocking(move || router_cloned.route(&req_cloned))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inst.spec.name, local.spec.name);

    server.abort();
}
