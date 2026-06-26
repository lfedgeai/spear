use std::time::Duration;

use tokio_stream::StreamExt;
use tonic::Request;

use spear_next::proto::sms::{
    ModelDeploymentPhase, ModelDeploymentRecord, ModelDeploymentSpec, ModelDeploymentStatus,
    ReportModelDeploymentStatusRequest, UpsertModelDeploymentRequest, WatchModelDeploymentsRequest,
};
use spear_next::sms::service::SmsServiceImpl;

#[tokio::test]
async fn test_watch_model_deployments_filters_upserts_by_target_node_uuid() {
    use spear_next::proto::sms::model_deployment_registry_service_server::ModelDeploymentRegistryService;

    let svc = SmsServiceImpl::with_storage_config(&spear_next::config::base::StorageConfig {
        backend: "memory".to_string(),
        ..Default::default()
    })
    .await;

    let mut stream = svc
        .watch_model_deployments(Request::new(WatchModelDeploymentsRequest {
            since_revision: 0,
            target_node_uuid: "node-a".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let resp_b = svc
        .upsert_model_deployment(Request::new(UpsertModelDeploymentRequest {
            record: Some(spear_next::proto::sms::ModelDeploymentRecord {
                deployment_id: String::new(),
                revision: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
                spec: Some(ModelDeploymentSpec {
                    target_node_uuid: "node-b".to_string(),
                    provider: "ollama".to_string(),
                    model: "llama3".to_string(),
                    params: std::collections::HashMap::new(),
                    backend: None,
                }),
                status: None,
            }),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp_b.deployment_id.is_empty());

    let no_msg = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;
    assert!(no_msg.is_err());

    let resp_a = svc
        .upsert_model_deployment(Request::new(UpsertModelDeploymentRequest {
            record: Some(spear_next::proto::sms::ModelDeploymentRecord {
                deployment_id: String::new(),
                revision: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
                spec: Some(ModelDeploymentSpec {
                    target_node_uuid: "node-a".to_string(),
                    provider: "ollama".to_string(),
                    model: "llama3".to_string(),
                    params: std::collections::HashMap::new(),
                    backend: None,
                }),
                status: None,
            }),
        }))
        .await
        .unwrap()
        .into_inner();

    let msg = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let ev = msg.event.unwrap();
    assert!(ev.upserts.iter().any(|id| id == &resp_a.deployment_id));
}

#[tokio::test]
async fn test_report_model_deployment_status_observed_revision_guard() {
    use spear_next::proto::sms::model_deployment_registry_service_server::ModelDeploymentRegistryService;
    use spear_next::proto::sms::{
        ListModelDeploymentsRequest, ReportModelDeploymentStatusResponse,
    };

    let svc = SmsServiceImpl::with_storage_config(&spear_next::config::base::StorageConfig {
        backend: "memory".to_string(),
        ..Default::default()
    })
    .await;

    let up = svc
        .upsert_model_deployment(Request::new(UpsertModelDeploymentRequest {
            record: Some(spear_next::proto::sms::ModelDeploymentRecord {
                deployment_id: String::new(),
                revision: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
                spec: Some(ModelDeploymentSpec {
                    target_node_uuid: "node-a".to_string(),
                    provider: "ollama".to_string(),
                    model: "llama3".to_string(),
                    params: std::collections::HashMap::new(),
                    backend: None,
                }),
                status: None,
            }),
        }))
        .await
        .unwrap()
        .into_inner();
    let deployment_id = up.deployment_id.clone();
    assert!(up.revision > 0);

    let r1: ReportModelDeploymentStatusResponse = svc
        .report_model_deployment_status(Request::new(ReportModelDeploymentStatusRequest {
            deployment_id: deployment_id.clone(),
            node_uuid: "node-a".to_string(),
            observed_revision: up.revision,
            status: Some(ModelDeploymentStatus {
                phase: ModelDeploymentPhase::Ready as i32,
                message: "first".to_string(),
                updated_at_ms: 0,
            }),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(r1.success);

    let r2: ReportModelDeploymentStatusResponse = svc
        .report_model_deployment_status(Request::new(ReportModelDeploymentStatusRequest {
            deployment_id: deployment_id.clone(),
            node_uuid: "node-a".to_string(),
            observed_revision: up.revision,
            status: Some(ModelDeploymentStatus {
                phase: ModelDeploymentPhase::Failed as i32,
                message: "second".to_string(),
                updated_at_ms: 0,
            }),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(r2.success);

    let list = svc
        .list_model_deployments(Request::new(ListModelDeploymentsRequest {
            limit: 100,
            offset: 0,
            target_node_uuid: "node-a".to_string(),
            provider: String::new(),
        }))
        .await
        .unwrap()
        .into_inner();
    let rec = list
        .records
        .iter()
        .find(|r| r.deployment_id == deployment_id)
        .unwrap();
    assert_eq!(rec.status.as_ref().unwrap().message.as_str(), "second");
    assert_eq!(
        rec.status.as_ref().unwrap().phase,
        ModelDeploymentPhase::Failed as i32
    );

    let up2 = svc
        .upsert_model_deployment(Request::new(UpsertModelDeploymentRequest {
            record: Some(ModelDeploymentRecord {
                deployment_id: deployment_id.clone(),
                revision: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
                spec: Some(ModelDeploymentSpec {
                    target_node_uuid: "node-a".to_string(),
                    provider: "vllm".to_string(),
                    model: "m2".to_string(),
                    params: Default::default(),
                    backend: None,
                }),
                status: None,
            }),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(up2.revision > up.revision);

    let r3: ReportModelDeploymentStatusResponse = svc
        .report_model_deployment_status(Request::new(ReportModelDeploymentStatusRequest {
            deployment_id: deployment_id.clone(),
            node_uuid: "node-a".to_string(),
            observed_revision: up.revision,
            status: Some(ModelDeploymentStatus {
                phase: ModelDeploymentPhase::Ready as i32,
                message: "stale".to_string(),
                updated_at_ms: 0,
            }),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!r3.success);
}
