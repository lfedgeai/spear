use crate::spearlet::ai::credential_resolver::{CredentialResolution, CredentialResolver};
use crate::spearlet::execution::ai::backends::BackendAdapter;
use crate::spearlet::execution::ai::ir::{
    CanonicalError, CanonicalRequestEnvelope, CanonicalResponseEnvelope, Operation,
};
use crate::spearlet::execution::ai::streaming::{
    StreamingPlan, StreamingWebsocketPlan, WebsocketFeatures, WebsocketPlan,
};
use url::Url;

pub struct OpenAIRealtimeWsBackendAdapter {
    name: String,
    base_url: String,
    credential_ref: Option<String>,
    credential_resolver: Option<CredentialResolver>,
}

impl OpenAIRealtimeWsBackendAdapter {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        credential_ref: Option<String>,
        credential_resolver: Option<CredentialResolver>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            credential_ref: credential_ref.and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            }),
            credential_resolver,
        }
    }

    fn resolve_api_key_state(&self) -> CredentialResolution {
        let Some(resolver) = self.credential_resolver.as_ref() else {
            return CredentialResolution::Missing;
        };
        resolver.resolve_api_key_state(self.credential_ref.as_deref())
    }
}

impl BackendAdapter for OpenAIRealtimeWsBackendAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn invoke(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<CanonicalResponseEnvelope, CanonicalError> {
        Err(CanonicalError {
            code: "unsupported_operation".to_string(),
            message: format!(
                "openai_realtime_ws is streaming-only; invoke() is unsupported for {:?}",
                req.operation
            ),
            retryable: false,
            operation: Some(req.operation.clone()),
        })
    }

    fn streaming_plan(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<StreamingPlan, CanonicalError> {
        if req.operation != crate::spearlet::execution::ai::ir::Operation::SpeechToText {
            return Err(CanonicalError {
                code: "unsupported_operation".to_string(),
                message: "openai_realtime_ws only supports speech_to_text streaming".to_string(),
                retryable: false,
                operation: Some(req.operation.clone()),
            });
        }

        let transcription_model = match &req.payload {
            crate::spearlet::execution::ai::ir::Payload::SpeechToText(p) => p
                .model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini-transcribe".to_string()),
            _ => "gpt-4o-mini-transcribe".to_string(),
        };
        let ws_url = derive_openai_realtime_ws_url(&self.base_url)?;
        let ws_url = normalize_transcription_ws_url(&ws_url)?;
        let mut headers = Vec::new();
        let credential_state = self.resolve_api_key_state();
        let api_key = match credential_state {
            CredentialResolution::Ready(secret) => Some(secret),
            CredentialResolution::Disabled
            | CredentialResolution::NotSynced
            | CredentialResolution::Missing
                if self.credential_ref.is_some() =>
            {
                return Err(CanonicalError {
                    code: credential_state.code().to_string(),
                    message: credential_state
                        .message(self.credential_ref.as_deref().unwrap_or_default()),
                    retryable: !matches!(credential_state, CredentialResolution::Disabled),
                    operation: Some(Operation::SpeechToText),
                });
            }
            CredentialResolution::Disabled
            | CredentialResolution::NotSynced
            | CredentialResolution::Missing => None,
        };
        if let Some(api_key) = api_key.as_deref() {
            headers.push(("authorization".to_string(), format!("Bearer {}", api_key)));
        }

        Ok(StreamingPlan::Websocket(StreamingWebsocketPlan {
            prepare: Vec::new(),
            websocket: WebsocketPlan {
                url: ws_url,
                headers,
                client_events: vec![serde_json::json!({
                    "type": "session.update",
                    "session": {
                        "type": "transcription",
                        "audio": {
                            "input": {
                                "format": { "type": "audio/pcm", "rate": 24000 },
                                "transcription": { "model": transcription_model },
                            }
                        }
                    }
                })],
                features: WebsocketFeatures {
                    turn_detection: true,
                },
            },
        }))
    }
}

fn derive_openai_realtime_ws_url(base_url: &str) -> Result<String, CanonicalError> {
    let mut u = Url::parse(base_url).map_err(|e| CanonicalError {
        code: "invalid_configuration".to_string(),
        message: format!("invalid base_url: {e}"),
        retryable: false,
        operation: None,
    })?;

    let scheme = match u.scheme() {
        "https" => "wss",
        "http" => "ws",
        "wss" => "wss",
        "ws" => "ws",
        s => {
            return Err(CanonicalError {
                code: "invalid_configuration".to_string(),
                message: format!("unsupported base_url scheme: {s}"),
                retryable: false,
                operation: None,
            })
        }
    };
    u.set_scheme(scheme).map_err(|_| CanonicalError {
        code: "invalid_configuration".to_string(),
        message: "set scheme failed".to_string(),
        retryable: false,
        operation: None,
    })?;

    let mut path = u.path().trim_end_matches('/').to_string();
    if !path.ends_with("/realtime") {
        if path.ends_with("/v1") {
            path = format!("{path}/realtime");
        } else {
            path = format!("{path}/v1/realtime");
        }
        u.set_path(&path);
    }
    Ok(u.to_string())
}

fn normalize_transcription_ws_url(ws_url: &str) -> Result<String, CanonicalError> {
    let mut u = Url::parse(ws_url).map_err(|e| CanonicalError {
        code: "invalid_configuration".to_string(),
        message: format!("invalid ws url: {e}"),
        retryable: false,
        operation: None,
    })?;
    let mut pairs: Vec<(String, String)> = u
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let has_intent = pairs.iter().any(|(k, _)| k == "intent");
    if !has_intent {
        pairs.push(("intent".to_string(), "transcription".to_string()));
    }
    pairs.retain(|(k, _)| k != "model");
    u.query_pairs_mut()
        .clear()
        .extend_pairs(pairs.iter().map(|(k, v)| (&**k, &**v)));
    Ok(u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::CredentialMaterial;
    use crate::spearlet::ai::credential_resolver::CredentialResolver;
    use crate::spearlet::config::SpearletConfig;
    use crate::spearlet::execution::ai::ir::{Payload, SpeechToTextPayload};

    #[test]
    fn streaming_plan_allows_missing_api_key_env() {
        let adapter =
            OpenAIRealtimeWsBackendAdapter::new("b1", "https://api.openai.com", None, None);
        let req = CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: crate::spearlet::execution::ai::ir::Operation::SpeechToText,
            meta: Default::default(),
            routing: Default::default(),
            requirements: Default::default(),
            timeout_ms: None,
            payload: Payload::SpeechToText(SpeechToTextPayload { model: None }),
            extra: Default::default(),
        };
        let plan = adapter.streaming_plan(&req).unwrap();
        let StreamingPlan::Websocket(p) = plan;
        assert!(!p
            .websocket
            .headers
            .iter()
            .any(|(k, _)| k == "authorization"));
    }

    #[test]
    fn streaming_plan_returns_credential_disabled_for_disabled_dynamic_credential() {
        let _guard =
            crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials_test_lock()
                .lock()
                .expect("lock");
        let store = crate::spearlet::ai::dynamic_credential_store::global_dynamic_credentials();
        store.clear();
        store.set_credentials(vec![CredentialMaterial {
            name: "openai-realtime".to_string(),
            provider_kind: "inline_encrypted".to_string(),
            secret: "sk-test".to_string(),
            version: 1,
            disabled: true,
            updated_at_ms: 0,
        }]);

        let adapter = OpenAIRealtimeWsBackendAdapter::new(
            "rt",
            "https://api.openai.com/v1",
            Some("openai-realtime".to_string()),
            Some(CredentialResolver::from_config(&SpearletConfig::default())),
        );
        let req = CanonicalRequestEnvelope {
            version: 1,
            request_id: "r1".to_string(),
            operation: crate::spearlet::execution::ai::ir::Operation::SpeechToText,
            meta: Default::default(),
            routing: Default::default(),
            requirements: Default::default(),
            timeout_ms: None,
            payload: Payload::SpeechToText(SpeechToTextPayload {
                model: Some("gpt-4o-mini-transcribe".to_string()),
            }),
            extra: Default::default(),
        };

        let err = adapter.streaming_plan(&req).expect_err("should fail");
        assert_eq!(err.code, "credential_disabled");
        store.clear();
    }
}
