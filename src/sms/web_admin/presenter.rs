use serde_json::{json, Value};

use crate::proto::sms::NodeBackendSnapshot;

pub(super) fn node_backends_snapshot_json(snapshot: Option<&NodeBackendSnapshot>) -> Value {
    let meta = snapshot.map(|s| {
        json!({
            "revision": s.revision,
            "reported_at_ms": s.reported_at_ms,
        })
    });
    let backends = snapshot
        .map(|s| {
            s.backends
                .iter()
                .filter_map(|b| {
                    b.spec.as_ref().map(|spec| {
                        json!({
                            "name": spec.name,
                            "kind": spec.kind,
                            "operations": spec.operations,
                            "features": spec.features,
                            "transports": spec.transports,
                            "weight": spec.weight,
                            "priority": spec.priority,
                            "base_url": spec.base_url,
                            "status": b.status,
                            "status_reason": b.status_reason,
                            "provider": spec.provider,
                            "model": spec.model,
                            "hosting": spec.hosting,
                        })
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "backends": backends,
        "snapshot": meta,
    })
}
