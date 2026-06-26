use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpJsonPlan {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    pub extract_json_path: String,
    pub extract_to_var: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum StreamingPrepareStep {
    HttpJson(HttpJsonPlan),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebsocketFeatures {
    #[serde(default)]
    pub turn_detection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsocketPlan {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub client_events: Vec<serde_json::Value>,
    #[serde(default)]
    pub features: WebsocketFeatures,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingWebsocketPlan {
    #[serde(default)]
    pub prepare: Vec<StreamingPrepareStep>,
    pub websocket: WebsocketPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamingPlan {
    Websocket(StreamingWebsocketPlan),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingInvocation {
    pub backend: String,
    pub plan: StreamingPlan,
}
