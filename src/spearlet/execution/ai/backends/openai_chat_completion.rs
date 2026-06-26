use serde_json::{json, Value};
use std::time::Duration;

use crate::spearlet::ai::credential_resolver::{CredentialResolution, CredentialResolver};
use crate::spearlet::execution::ai::backends::BackendAdapter;
use crate::spearlet::execution::ai::backends::http_json::{
    build_json_payload_response, ensure_chat_operation, filtered_chat_params,
    insert_tools_if_any, join_url, parse_json_response_body, post_json_blocking,
    require_non_empty_field, upstream_status_error,
};
use crate::spearlet::execution::ai::ir::{
    CanonicalError, CanonicalRequestEnvelope, CanonicalResponseEnvelope, Operation, Payload,
};

pub struct OpenAIChatCompletionBackendAdapter {
    name: String,
    base_url: String,
    static_api_key: Option<String>,
    credential_ref: Option<String>,
    credential_resolver: Option<CredentialResolver>,
    fixed_model: Option<String>,
}

impl OpenAIChatCompletionBackendAdapter {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        credential_ref: Option<String>,
        credential_resolver: Option<CredentialResolver>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            static_api_key: None,
            credential_ref: credential_ref.and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            }),
            credential_resolver,
            fixed_model: None,
        }
    }

    pub fn with_static_api_key(mut self, api_key: impl Into<String>) -> Self {
        let v = api_key.into();
        self.static_api_key = if v.trim().is_empty() { None } else { Some(v) };
        self
    }

    pub fn with_fixed_model(mut self, model: impl Into<String>) -> Self {
        let s = model.into();
        let t = s.trim().to_string();
        self.fixed_model = if t.is_empty() { None } else { Some(t) };
        self
    }

    fn build_chat_completions_body(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<Value, CanonicalError> {
        let Payload::ChatCompletions(p) = &req.payload else {
            return Err(CanonicalError {
                code: "payload_mismatch".to_string(),
                message: "expected chat_completions payload".to_string(),
                retryable: false,
                operation: Some(req.operation.clone()),
            });
        };

        let model = self
            .fixed_model
            .as_deref()
            .unwrap_or_else(|| p.model.as_str())
            .to_string();
        let model = require_non_empty_field(req.operation.clone(), &model, "model")?;

        if p.messages.is_empty() {
            return Err(CanonicalError {
                code: "invalid_request".to_string(),
                message: "missing messages".to_string(),
                retryable: false,
                operation: Some(req.operation.clone()),
            });
        }

        let messages_val = serde_json::to_value(&p.messages).map_err(|e| CanonicalError {
            code: "serialization".to_string(),
            message: e.to_string(),
            retryable: false,
            operation: Some(req.operation.clone()),
        })?;

        let mut body = json!({
            "model": model,
            "messages": messages_val,
        });

        if let Some(obj) = body.as_object_mut() {
            insert_tools_if_any(obj, &p.tools);
            for (k, v) in filtered_chat_params(&p.params) {
                obj.insert(k, v);
            }
        }

        Ok(body)
    }

    fn extract_openai_error_message(json: &Value) -> Option<String> {
        let e = json.get("error")?;
        let msg = e.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let ty = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let code_owned = if let Some(s) = e.get("code").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(n) = e.get("code").and_then(|v| v.as_i64()) {
            n.to_string()
        } else {
            String::new()
        };

        let mut parts: Vec<String> = Vec::new();
        if !ty.is_empty() {
            parts.push(ty.to_string());
        }
        if !code_owned.is_empty() {
            parts.push(code_owned);
        }
        if !msg.is_empty() {
            parts.push(msg.to_string());
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(": "))
        }
    }

    fn resolve_api_key_state(&self) -> CredentialResolution {
        if let Some(api_key) = self.static_api_key.clone() {
            if !api_key.trim().is_empty() {
                return CredentialResolution::Ready(api_key);
            }
        }
        let Some(resolver) = self.credential_resolver.as_ref() else {
            return CredentialResolution::Missing;
        };
        resolver.resolve_api_key_state(self.credential_ref.as_deref())
    }
}

impl BackendAdapter for OpenAIChatCompletionBackendAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn invoke(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<CanonicalResponseEnvelope, CanonicalError> {
        ensure_chat_operation(req, "backend supports chat_completions only")?;

        let body_json = self.build_chat_completions_body(req)?;
        let body_bytes = serde_json::to_vec(&body_json).map_err(|e| CanonicalError {
            code: "serialization".to_string(),
            message: e.to_string(),
            retryable: false,
            operation: Some(req.operation.clone()),
        })?;

        let url = if self.base_url.contains("/v1") {
            join_url(&self.base_url, "chat/completions")
        } else {
            join_url(&self.base_url, "v1/chat/completions")
        };

        let timeout = req.timeout_ms.map(Duration::from_millis);
        let credential_state = self.resolve_api_key_state();
        let api_key = match credential_state {
            CredentialResolution::Ready(secret) => Some(secret),
            CredentialResolution::Disabled
            | CredentialResolution::NotSynced
            | CredentialResolution::Missing if self.credential_ref.is_some() => {
                return Err(CanonicalError {
                    code: credential_state.code().to_string(),
                    message: credential_state
                        .message(self.credential_ref.as_deref().unwrap_or_default()),
                    retryable: !matches!(credential_state, CredentialResolution::Disabled),
                    operation: Some(Operation::ChatCompletions),
                });
            }
            CredentialResolution::Disabled
            | CredentialResolution::NotSynced
            | CredentialResolution::Missing => None,
        };
        let resp = post_json_blocking(
            Operation::ChatCompletions,
            url,
            body_bytes,
            timeout,
            api_key.as_deref(),
        )?;

        let status_u16 = resp.status as u16;
        let ok = (200..300).contains(&status_u16);
        let parsed =
            parse_json_response_body(req.operation.clone(), resp.status, &resp.body)?;

        if !ok {
            let extra = Self::extract_openai_error_message(&parsed);
            return Err(upstream_status_error(
                req.operation.clone(),
                resp.status,
                extra,
            ));
        }

        Ok(build_json_payload_response(
            req,
            &self.name,
            parsed,
            resp.body,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::CredentialMaterial;
    use crate::spearlet::config::SpearletConfig;
    use crate::spearlet::execution::ai::ir::{ChatCompletionsPayload, ChatMessage, RoutingHints};
    use crate::spearlet::param_keys::{chat as chat_keys, mcp as mcp_keys};
    use std::collections::HashMap;

    #[test]
    fn test_build_body_does_not_require_api_key() {
        let adapter =
            OpenAIChatCompletionBackendAdapter::new(
                "openai",
                "https://api.openai.com/v1",
                None,
                None,
            );
        let req = CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: Operation::ChatCompletions,
            meta: HashMap::new(),
            routing: RoutingHints::default(),
            requirements: Default::default(),
            timeout_ms: None,
            payload: Payload::ChatCompletions(ChatCompletionsPayload {
                model: "gpt-test".to_string(),
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
        let body = adapter.build_chat_completions_body(&req).unwrap();
        assert_eq!(body.get("model").and_then(|v| v.as_str()), Some("gpt-test"));
    }

    #[test]
    fn test_join_url() {
        let adapter = OpenAIChatCompletionBackendAdapter::new(
            "openai",
            "https://api.openai.com/v1/",
            Some("k".to_string()),
            None,
        );
        assert_eq!(
            join_url(&adapter.base_url, "chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_params_cannot_override_messages_or_tools() {
        let adapter = OpenAIChatCompletionBackendAdapter::new(
            "openai",
            "https://api.openai.com/v1/",
            Some("k".to_string()),
            None,
        );

        let mut params = HashMap::new();
        params.insert(
            chat_keys::MESSAGES.to_string(),
            json!([{ "role": "user", "content": "override" }]),
        );
        params.insert(
            chat_keys::TOOLS.to_string(),
            json!([{ "type": "function", "function": {"name":"x"}}]),
        );
        params.insert(chat_keys::TOOL_ARENA_PTR.to_string(), json!(1234));
        params.insert(chat_keys::TOOL_ARENA_LEN.to_string(), json!(5678));
        params.insert(chat_keys::MAX_ITERATIONS.to_string(), json!(9));
        params.insert(chat_keys::MAX_TOTAL_TOOL_CALLS.to_string(), json!(99));
        params.insert(mcp_keys::param::ENABLED.to_string(), json!(true));
        params.insert(mcp_keys::param::SERVER_IDS.to_string(), json!(["fs"]));
        params.insert(
            mcp_keys::param::TOOL_ALLOWLIST.to_string(),
            json!(["read_*"]),
        );

        let req = CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: Operation::ChatCompletions,
            meta: HashMap::new(),
            routing: RoutingHints::default(),
            requirements: Default::default(),
            timeout_ms: None,
            payload: Payload::ChatCompletions(ChatCompletionsPayload {
                model: "gpt-test".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: Value::String("original".to_string()),
                    tool_call_id: None,
                    tool_calls: None,
                    name: None,
                }],
                tools: vec![json!({"type":"function","function":{"name":"y"}})],
                params,
            }),
            extra: HashMap::new(),
        };

        let body = adapter.build_chat_completions_body(&req).unwrap();
        assert_eq!(
            body.get("messages").unwrap()[0]["content"],
            Value::String("original".to_string())
        );
        assert_eq!(body.get("tools").unwrap()[0]["function"]["name"], "y");
        assert!(body.get(chat_keys::TOOL_ARENA_PTR).is_none());
        assert!(body.get(chat_keys::TOOL_ARENA_LEN).is_none());
        assert!(body.get(chat_keys::MAX_ITERATIONS).is_none());
        assert!(body.get(chat_keys::MAX_TOTAL_TOOL_CALLS).is_none());
        assert!(body.get(mcp_keys::param::ENABLED).is_none());
        assert!(body.get(mcp_keys::param::SERVER_IDS).is_none());
        assert!(body.get(mcp_keys::param::TOOL_ALLOWLIST).is_none());
    }

    #[test]
    fn test_invoke_returns_credential_disabled_when_dynamic_credential_is_disabled() {
        let _guard = crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials_test_lock()
            .lock()
            .expect("lock");
        let store = crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials();
        store.clear();
        store.set_credentials(vec![CredentialMaterial {
            name: "openai-default".to_string(),
            provider_kind: "inline_encrypted".to_string(),
            secret: "sk-test".to_string(),
            version: 1,
            disabled: true,
            updated_at_ms: 0,
        }]);

        let adapter = OpenAIChatCompletionBackendAdapter::new(
            "openai",
            "https://api.openai.com/v1",
            Some("openai-default".to_string()),
            Some(CredentialResolver::from_config(&SpearletConfig::default())),
        );
        let req = CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: Operation::ChatCompletions,
            meta: HashMap::new(),
            routing: RoutingHints::default(),
            requirements: Default::default(),
            timeout_ms: None,
            payload: Payload::ChatCompletions(ChatCompletionsPayload {
                model: "gpt-test".to_string(),
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

        let err = adapter.invoke(&req).expect_err("should fail");
        assert_eq!(err.code, "credential_disabled");
        store.clear();
    }
}
