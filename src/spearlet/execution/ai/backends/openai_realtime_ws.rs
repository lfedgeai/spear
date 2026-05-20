use crate::spearlet::execution::ai::backends::BackendAdapter;
use crate::spearlet::execution::ai::ir::{
    CanonicalError, CanonicalRequestEnvelope, CanonicalResponseEnvelope,
};
use crate::spearlet::execution::ai::streaming::{
    StreamingPlan, StreamingWebsocketPlan, WebsocketPlan,
};
use url::Url;

pub struct OpenAIRealtimeWsBackendAdapter {
    name: String,
    base_url: String,
    api_key_env: Option<String>,
}

impl OpenAIRealtimeWsBackendAdapter {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key_env: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            api_key_env: api_key_env.and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            }),
        }
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

        let model = match &req.payload {
            crate::spearlet::execution::ai::ir::Payload::SpeechToText(p) => p
                .model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini-transcribe".to_string()),
            _ => "gpt-4o-mini-transcribe".to_string(),
        };
        let ws_url = derive_openai_realtime_ws_url(&self.base_url)?;
        let mut headers = Vec::new();
        if let Some(api_key_env) = self.api_key_env.as_deref() {
            headers.push((
                "authorization".to_string(),
                format!("Bearer ${{env:{}}}", api_key_env),
            ));
        }

        Ok(StreamingPlan::Websocket(StreamingWebsocketPlan {
            prepare: Vec::new(),
            websocket: WebsocketPlan {
                url: ws_url,
                headers,
                client_events: vec![serde_json::json!({
                    "type": "session.update",
                    "session": {
                        "type": "realtime",
                        "output_modalities": ["text"],
                        "audio": {
                            "input": {
                                "format": { "type": "audio/pcm", "rate": 24000 },
                                "transcription": { "model": model },
                            }
                        }
                    }
                })],
                supports_turn_detection: true,
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
    let has_model = u.query_pairs().any(|(k, _)| k == "model");
    if !has_model {
        u.set_query(Some("model=gpt-realtime"));
    }
    Ok(u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::ai::ir::{Payload, SpeechToTextPayload};

    #[test]
    fn streaming_plan_allows_missing_api_key_env() {
        let adapter = OpenAIRealtimeWsBackendAdapter::new("b1", "https://api.openai.com", None);
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
        let StreamingPlan::Websocket(p) = plan else {
            panic!("expected websocket plan");
        };
        assert!(!p
            .websocket
            .headers
            .iter()
            .any(|(k, _)| k == "authorization"));
    }
}
