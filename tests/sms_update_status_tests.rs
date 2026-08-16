use tonic::Request;

#[tokio::test]
async fn test_update_task_status_persists_and_returns_updated_task() {
    use spear_next::proto::sms::task_service_server::TaskService;
    use spear_next::proto::sms::{RegisterTaskRequest, TaskStatus, UpdateTaskStatusRequest};
    use spear_next::sms::service::SmsServiceImpl;

    // Create service with memory backend
    let svc = SmsServiceImpl::with_storage_config(&spear_next::config::base::StorageConfig {
        backend: "memory".to_string(),
        ..Default::default()
    })
    .await;

    // Register a task
    let req = RegisterTaskRequest {
        name: "t".to_string(),
        description: "d".to_string(),
        priority: 2,
        endpoint: "t".to_string(),
        version: "v1".to_string(),
        capabilities: vec![],
        metadata: std::collections::HashMap::new(),
        config: std::collections::HashMap::new(),
        executable: None,
        desired_replicas: 1,
        scheduling_strategy: spear_next::proto::sms::TaskSchedulingStrategy::Spread as i32,
    };
    let resp = svc
        .register_task(Request::new(req))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.success);
    let task_id = resp.task_id.clone();

    // Update status to Active
    let upd = UpdateTaskStatusRequest {
        task_id: task_id.clone(),
        status: TaskStatus::Active as i32,
        node_uuid: "node-1".to_string(),
        status_version: 1,
        updated_at: chrono::Utc::now().timestamp(),
        reason: "test".to_string(),
    };
    let resp = svc
        .update_task_status(Request::new(upd))
        .await
        .unwrap()
        .into_inner();
    assert!(resp.success);
    let updated = resp.task.unwrap();
    assert_eq!(updated.task_id, task_id);
    assert_eq!(updated.status, TaskStatus::Active as i32);
}
