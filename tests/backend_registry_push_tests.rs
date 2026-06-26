use tonic::Request;

use spear_next::proto::sms::{
    backend_registry_service_server::BackendRegistryService, BackendHosting, BackendInfo,
    BackendOrigin, BackendSpec, BackendStatus, GetNodeBackendsRequest,
    ListNodeBackendSnapshotsRequest, NodeBackendSnapshot, ReportNodeBackendsRequest,
};
use spear_next::sms::service::SmsServiceImpl;

#[tokio::test]
async fn backend_snapshot_report_and_query_works() {
    let svc = SmsServiceImpl::with_storage_config(&spear_next::config::base::StorageConfig {
        backend: "memory".to_string(),
        ..Default::default()
    })
    .await;

    let snapshot = NodeBackendSnapshot {
        node_uuid: "node-1".to_string(),
        revision: 1,
        reported_at_ms: 0,
        backends: vec![BackendInfo {
            spec: Some(BackendSpec {
                name: "openai-chat".to_string(),
                kind: "openai_chat_completion".to_string(),
                operations: vec!["chat_completions".to_string()],
                features: vec![],
                transports: vec!["http".to_string()],
                weight: 100,
                priority: 0,
                base_url: "https://api.openai.com/v1".to_string(),
                provider: "openai".to_string(),
                model: String::new(),
                hosting: BackendHosting::Remote as i32,
                credential_ref: String::new(),
                origin: BackendOrigin::StaticConfig as i32,
                deployment_id: String::new(),
            }),
            status: BackendStatus::Available as i32,
            status_reason: String::new(),
        }],
    };

    let resp = svc
        .report_node_backends(Request::new(ReportNodeBackendsRequest {
            snapshot: Some(snapshot),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.success);
    assert_eq!(resp.accepted_revision, 1);

    let got = svc
        .get_node_backends(Request::new(GetNodeBackendsRequest {
            node_uuid: "node-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(got.found);
    assert_eq!(got.snapshot.as_ref().unwrap().revision, 1);

    let listed = svc
        .list_node_backend_snapshots(Request::new(ListNodeBackendSnapshotsRequest {
            limit: 10,
            offset: 0,
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(listed.total_count, 1);
    assert_eq!(listed.snapshots.len(), 1);

    let resp2 = svc
        .report_node_backends(Request::new(ReportNodeBackendsRequest {
            snapshot: Some(NodeBackendSnapshot {
                node_uuid: "node-1".to_string(),
                revision: 0,
                reported_at_ms: 0,
                backends: vec![],
            }),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp2.accepted_revision, 1);

    let got2 = svc
        .get_node_backends(Request::new(GetNodeBackendsRequest {
            node_uuid: "node-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(got2.snapshot.as_ref().unwrap().revision, 1);
}
