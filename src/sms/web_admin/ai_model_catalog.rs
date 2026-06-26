use std::collections::{BTreeSet, HashMap};

use serde_json::{json, Value};

use crate::proto::sms::{BackendHosting, BackendStatus, NodeBackendSnapshot};

#[derive(Default)]
struct AiModelAggregate {
    provider: String,
    model: String,
    hosting: String,
    operations: BTreeSet<String>,
    features: BTreeSet<String>,
    transports: BTreeSet<String>,
    available_nodes: i64,
    total_nodes: i64,
    instances: Vec<Value>,
}

fn availability_label_from_status(v: i32) -> &'static str {
    if v == BackendStatus::Available as i32 {
        "available"
    } else {
        "unavailable"
    }
}

fn hosting_label_from_proto(v: i32) -> &'static str {
    if v == BackendHosting::Remote as i32 {
        "remote"
    } else if v == BackendHosting::NodeLocal as i32 {
        "local"
    } else {
        "unknown"
    }
}

fn infer_provider_name(spec: &crate::proto::sms::BackendSpec) -> String {
    if !spec.provider.trim().is_empty() {
        spec.provider.clone()
    } else if spec.kind.starts_with("openai_") {
        "openai".to_string()
    } else if spec.kind == "ollama_chat" {
        "ollama".to_string()
    } else if spec.kind == "stub" {
        "internal".to_string()
    } else {
        "unknown".to_string()
    }
}

fn infer_model_name(spec: &crate::proto::sms::BackendSpec) -> String {
    let mut model = spec.model.clone();
    if model.trim().is_empty() {
        model = "(dynamic)".to_string();
        if spec.kind == "ollama_chat" && spec.name.contains('/') {
            if let Some((_, rest)) = spec.name.split_once('/') {
                if !rest.trim().is_empty() {
                    model = rest.to_string();
                }
            }
        }
    }
    model
}

fn extend_capability_sets_from_spec(
    operations: &mut BTreeSet<String>,
    features: &mut BTreeSet<String>,
    transports: &mut BTreeSet<String>,
    spec: &crate::proto::sms::BackendSpec,
) {
    for op in spec.operations.iter() {
        if !op.trim().is_empty() {
            operations.insert(op.clone());
        }
    }
    for f in spec.features.iter() {
        if !f.trim().is_empty() {
            features.insert(f.clone());
        }
    }
    for t in spec.transports.iter() {
        if !t.trim().is_empty() {
            transports.insert(t.clone());
        }
    }
}

pub(super) fn aggregate_models(
    snapshots: impl IntoIterator<Item = NodeBackendSnapshot>,
    provider_filter: Option<&str>,
    hosting_filter: Option<&str>,
    needle: Option<&str>,
) -> Vec<Value> {
    let mut models_by_key: HashMap<(String, String, String), AiModelAggregate> = HashMap::new();

    for snap in snapshots {
        let node_uuid = snap.node_uuid.clone();
        for b in snap.backends.into_iter() {
            let Some(spec) = b.spec.as_ref() else {
                continue;
            };

            let provider = infer_provider_name(spec);
            let model = infer_model_name(spec);
            let hosting = hosting_label_from_proto(spec.hosting).to_string();
            let status = availability_label_from_status(b.status);

            if let Some(pf) = provider_filter {
                if provider.to_ascii_lowercase() != pf {
                    continue;
                }
            }
            if let Some(hf) = hosting_filter {
                if hosting.to_ascii_lowercase() != hf {
                    continue;
                }
            }
            if let Some(n) = needle {
                let hay = format!(
                    "{} {} {} {} {}",
                    provider, model, spec.name, spec.kind, spec.base_url
                )
                .to_ascii_lowercase();
                if !hay.contains(n) {
                    continue;
                }
            }

            let key = (provider.clone(), model.clone(), hosting.clone());
            let entry = models_by_key.entry(key).or_insert_with(|| AiModelAggregate {
                provider: provider.clone(),
                model: model.clone(),
                hosting: hosting.clone(),
                ..Default::default()
            });

            extend_capability_sets_from_spec(
                &mut entry.operations,
                &mut entry.features,
                &mut entry.transports,
                spec,
            );

            entry.instances.push(json!({
                "node_uuid": node_uuid,
                "backend_name": spec.name,
                "kind": spec.kind,
                "base_url": spec.base_url,
                "status": status,
                "status_reason": b.status_reason,
                "weight": spec.weight,
                "priority": spec.priority,
                "provider": provider,
                "model": model,
                "hosting": hosting,
            }));
            entry.total_nodes += 1;
            if status == "available" {
                entry.available_nodes += 1;
            }
        }
    }

    let mut list = models_by_key
        .into_values()
        .map(|a| {
            json!({
                "provider": a.provider,
                "model": a.model,
                "hosting": a.hosting,
                "operations": a.operations.into_iter().collect::<Vec<_>>(),
                "features": a.features.into_iter().collect::<Vec<_>>(),
                "transports": a.transports.into_iter().collect::<Vec<_>>(),
                "available_nodes": a.available_nodes,
                "total_nodes": a.total_nodes,
                "instances": a.instances,
            })
        })
        .collect::<Vec<_>>();

    list.sort_by(|a, b| {
        let ap = a.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let bp = b.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let am = a.get("model").and_then(|v| v.as_str()).unwrap_or("");
        let bm = b.get("model").and_then(|v| v.as_str()).unwrap_or("");
        ap.cmp(bp).then_with(|| am.cmp(bm))
    });
    list
}
