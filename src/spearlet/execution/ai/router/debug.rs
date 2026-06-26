use serde::Serialize;

use crate::spearlet::execution::ai::ir::Operation;

use super::policy::SelectionPolicy;
use super::registry::{BackendInstance, Hosting};
use super::Router;

#[derive(Debug, Clone, Serialize)]
pub struct BackendDebugInfo {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub hosting: String,
    pub model: Option<String>,
    pub provider: String,
    pub origin: String,
    pub deployment_id: Option<String>,
    pub credential_ref: Option<String>,
    pub weight: u32,
    pub priority: i32,
    pub ops: Vec<String>,
    pub features: Vec<String>,
    pub transports: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouterDebugSnapshot {
    pub policy: String,
    pub static_backends: Vec<BackendDebugInfo>,
    pub managed_backends: Vec<BackendDebugInfo>,
    pub managed_backends_by_origin: std::collections::BTreeMap<String, Vec<BackendDebugInfo>>,
}

pub fn snapshot(router: &Router) -> RouterDebugSnapshot {
    let static_backends = router
        .registry
        .instances()
        .iter()
        .map(backend_to_debug)
        .collect::<Vec<_>>();

    let managed = router.dynamic_instances();
    let managed_backends = managed.iter().map(backend_to_debug).collect::<Vec<_>>();
    let mut managed_backends_by_origin: std::collections::BTreeMap<String, Vec<BackendDebugInfo>> =
        std::collections::BTreeMap::new();
    for b in managed_backends.iter().cloned() {
        managed_backends_by_origin
            .entry(b.origin.clone())
            .or_default()
            .push(b);
    }

    RouterDebugSnapshot {
        policy: selection_policy_to_str(&router.policy).to_string(),
        static_backends,
        managed_backends,
        managed_backends_by_origin,
    }
}

fn backend_to_debug(b: &BackendInstance) -> BackendDebugInfo {
    let origin = crate::proto::sms::BackendOrigin::try_from(b.spec.origin)
        .ok()
        .unwrap_or(crate::proto::sms::BackendOrigin::Unspecified)
        .as_str_name()
        .to_ascii_lowercase();
    BackendDebugInfo {
        name: b.spec.name.clone(),
        kind: b.spec.kind.clone(),
        base_url: b.spec.base_url.clone(),
        hosting: hosting_to_str(b.hosting).to_string(),
        model: if b.spec.model.trim().is_empty() {
            None
        } else {
            Some(b.spec.model.clone())
        },
        provider: b.spec.provider.clone(),
        origin,
        deployment_id: if b.spec.deployment_id.trim().is_empty() {
            None
        } else {
            Some(b.spec.deployment_id.clone())
        },
        credential_ref: if b.spec.credential_ref.trim().is_empty() {
            None
        } else {
            Some(b.spec.credential_ref.clone())
        },
        weight: b.spec.weight,
        priority: b.spec.priority,
        ops: b.capabilities.ops.iter().map(op_to_str).collect(),
        features: b.capabilities.features.clone(),
        transports: b.capabilities.transports.clone(),
    }
}

fn hosting_to_str(h: Hosting) -> &'static str {
    match h {
        Hosting::Unknown => "unknown",
        Hosting::Local => "local",
        Hosting::Remote => "remote",
    }
}

fn selection_policy_to_str(p: &SelectionPolicy) -> &'static str {
    match p {
        SelectionPolicy::WeightedRandom => "weighted_random",
    }
}

fn op_to_str(op: &Operation) -> String {
    match op {
        Operation::ChatCompletions => "chat_completions".to_string(),
        Operation::Embeddings => "embeddings".to_string(),
        Operation::ImageGeneration => "image_generation".to_string(),
        Operation::SpeechToText => "speech_to_text".to_string(),
        Operation::TextToSpeech => "text_to_speech".to_string(),
    }
}
