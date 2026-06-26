use serde_json::{json, Value};
use std::time::Duration;

use crate::spearlet::execution::ai::backends::BackendAdapter;
use crate::spearlet::execution::ai::backends::http_json::{
    build_json_payload_response, ensure_chat_operation, filtered_chat_params,
    insert_tools_if_any, join_url, parse_json_response_body, post_json_blocking,
    require_non_empty_field, upstream_status_error,
};
use crate::spearlet::execution::ai::ir::{
    CanonicalError, CanonicalRequestEnvelope, CanonicalResponseEnvelope, Operation, Payload,
};

pub struct OllamaChatBackendAdapter {
    name: String,
    base_url: String,
    fixed_model: Option<String>,
}

impl OllamaChatBackendAdapter {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        fixed_model: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            fixed_model,
        }
    }

    fn build_chat_body(&self, req: &CanonicalRequestEnvelope) -> Result<Value, CanonicalError> {
        let Payload::ChatCompletions(p) = &req.payload else {
            return Err(CanonicalError {
                code: "invalid_request".to_string(),
                message: "expected chat_completions payload".to_string(),
                retryable: false,
                operation: Some(req.operation.clone()),
            });
        };

        let model = self
            .fixed_model
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| p.model.clone());

        let messages = p
            .messages
            .iter()
            .map(|m| {
                let content = if let Some(s) = m.content.as_str() {
                    s.to_string()
                } else {
                    serde_json::to_string(&m.content).unwrap_or_default()
                };
                json!({
                    "role": m.role,
                    "content": content,
                })
            })
            .collect::<Vec<_>>();

        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": false,
        });

        if let Some(obj) = body.as_object_mut() {
            insert_tools_if_any(obj, &p.tools);
            let options = filtered_chat_params(&p.params);
            if !options.is_empty() {
                obj.insert("options".to_string(), Value::Object(options));
            }
        }

        Ok(body)
    }

    fn to_openai_chat_completion(
        &self,
        req: &CanonicalRequestEnvelope,
        model: String,
        assistant_content: String,
    ) -> Value {
        json!({
            "id": req.request_id,
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": model,
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": assistant_content,
                    },
                    "finish_reason": "stop",
                }
            ]
        })
    }

    fn extract_error_message(v: &Value) -> Option<String> {
        v.get("error")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
    }
}

impl BackendAdapter for OllamaChatBackendAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn invoke(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<CanonicalResponseEnvelope, CanonicalError> {
        ensure_chat_operation(req, "ollama_chat supports chat_completions only")?;

        let body_json = self.build_chat_body(req)?;
        let model = body_json
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let model = require_non_empty_field(req.operation.clone(), &model, "model")?;

        let body_bytes = serde_json::to_vec(&body_json).map_err(|e| CanonicalError {
            code: "serialization".to_string(),
            message: e.to_string(),
            retryable: false,
            operation: Some(req.operation.clone()),
        })?;

        let url = join_url(&self.base_url, "api/chat");
        let timeout = req.timeout_ms.map(Duration::from_millis);
        let resp = post_json_blocking(
            Operation::ChatCompletions,
            url,
            body_bytes,
            timeout,
            None,
        )?;

        let status_u16 = resp.status as u16;
        let ok = (200..300).contains(&status_u16);
        let parsed =
            parse_json_response_body(req.operation.clone(), resp.status, &resp.body)?;

        if !ok {
            let extra = Self::extract_error_message(&parsed);
            return Err(upstream_status_error(
                req.operation.clone(),
                resp.status,
                extra,
            ));
        }

        let assistant_content = parsed
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let openai_like = self.to_openai_chat_completion(req, model, assistant_content);

        Ok(build_json_payload_response(
            req,
            &self.name,
            openai_like,
            resp.body,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Json, Router};
    use serde_json::json;
    use std::collections::HashMap;
    use tokio::net::TcpListener;
    use crate::spearlet::execution::ai::ir::ResultPayload;

    fn chat_req(model: &str) -> CanonicalRequestEnvelope {
        use crate::spearlet::execution::ai::ir::{
            ChatCompletionsPayload, ChatMessage, RoutingHints,
        };

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

    async fn start_mock_chat() -> String {
        let app = Router::new().route(
            "/api/chat",
            post(|Json(_v): Json<Value>| async move {
                Json(json!({
                    "model": "llama3",
                    "message": {"role": "assistant", "content": "ok"},
                    "done": true
                }))
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{}", addr)
    }

    #[test]
    fn test_missing_model_is_error() {
        let adapter = OllamaChatBackendAdapter::new("o", "http://127.0.0.1:11434", None);
        let mut req = chat_req("");
        if let Payload::ChatCompletions(p) = &mut req.payload {
            p.model = "".to_string();
        }
        let err = adapter.invoke(&req).unwrap_err();
        assert_eq!(err.code, "invalid_request");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_invoke_returns_openai_like_shape() {
        let base = start_mock_chat().await;
        let adapter = OllamaChatBackendAdapter::new("o", base, Some("llama3".to_string()));
        let req = chat_req("ignored");
        let resp = tokio::task::spawn_blocking(move || adapter.invoke(&req))
            .await
            .unwrap()
            .unwrap();
        let v = match resp.result {
            ResultPayload::Payload(v) => v,
            _ => panic!("unexpected"),
        };
        assert!(v.get("choices").is_some());
        assert_eq!(
            v["choices"][0]["message"]["content"],
            Value::String("ok".to_string())
        );
    }
}
