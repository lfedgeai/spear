//! Tests for UpdateTaskStatus RPC in SMS service
use tonic::Request;

use crate::config::base::StorageConfig;
use crate::proto::sms::{
    execution_index_service_server::ExecutionIndexService as ExecutionIndexServiceTrait,
    instance_registry_service_server::InstanceRegistryService as InstanceRegistryServiceTrait,
    node_service_server::NodeService as NodeServiceTrait,
    task_placement_assignment_service_server::TaskPlacementAssignmentService as TaskPlacementAssignmentServiceTrait,
    task_service_server::TaskService as TaskServiceTrait, CompleteTaskDeletionRequest,
    DeleteTaskRequest, GetTaskRequest, Instance, InstanceStatus, ListNodeTaskAssignmentsRequest,
    ListTaskInstancesRequest, Node, RegisterNodeRequest, RegisterTaskRequest,
    ResolveEndpointRequest, TaskPriority, TaskStatus, UpdateTaskStatusRequest,
};
use crate::sms::service::SmsServiceImpl;
use uuid::Uuid;

async fn create_test_sms_service() -> SmsServiceImpl {
    let storage_config = StorageConfig {
        backend: "memory".to_string(),
        data_dir: "/tmp/test_sms".to_string(),
        max_cache_size_mb: 100,
        compression_enabled: false,
        pool_size: 10,
    };
    SmsServiceImpl::with_storage_config(&storage_config).await
}

#[tokio::test]
async fn test_update_task_status_active_then_inactive() {
    let sms_service = create_test_sms_service().await;

    let req = RegisterTaskRequest {
        name: "echo".to_string(),
        description: "simple echo".to_string(),
        priority: TaskPriority::Normal as i32,
        endpoint: "echo".to_string(),
        version: "1.0.0".to_string(),
        capabilities: vec!["echo".to_string()],
        metadata: std::collections::HashMap::new(),
        config: std::collections::HashMap::new(),
        executable: None,
        desired_replicas: 1,
        scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
    };
    let resp = TaskServiceTrait::register_task(&sms_service, Request::new(req))
        .await
        .unwrap();
    let task_id = resp.get_ref().task_id.clone();

    let update_req = UpdateTaskStatusRequest {
        task_id: task_id.clone(),
        status: TaskStatus::Registered as i32,
        node_uuid: Uuid::new_v4().to_string(),
        status_version: 1,
        updated_at: chrono::Utc::now().timestamp(),
        reason: "registered".to_string(),
    };
    let update_resp = TaskServiceTrait::update_task_status(&sms_service, Request::new(update_req))
        .await
        .unwrap();
    assert!(update_resp.get_ref().success);

    let get_resp = TaskServiceTrait::get_task(
        &sms_service,
        Request::new(GetTaskRequest {
            task_id: task_id.clone(),
        }),
    )
    .await
    .unwrap();
    let task = get_resp.get_ref().task.as_ref().unwrap();
    assert_eq!(task.status, TaskStatus::Registered as i32);

    let update_req2 = UpdateTaskStatusRequest {
        task_id: task_id.clone(),
        status: TaskStatus::Inactive as i32,
        node_uuid: Uuid::new_v4().to_string(),
        status_version: 2,
        updated_at: chrono::Utc::now().timestamp(),
        reason: "deactivate".to_string(),
    };
    let update_resp2 =
        TaskServiceTrait::update_task_status(&sms_service, Request::new(update_req2))
            .await
            .unwrap();
    assert!(update_resp2.get_ref().success);

    let get_resp2 = TaskServiceTrait::get_task(
        &sms_service,
        Request::new(GetTaskRequest {
            task_id: task_id.clone(),
        }),
    )
    .await
    .unwrap();
    let task2 = get_resp2.get_ref().task.as_ref().unwrap();
    assert_eq!(task2.status, TaskStatus::Inactive as i32);

    let update_req3 = UpdateTaskStatusRequest {
        task_id: task_id.clone(),
        status: TaskStatus::Active as i32,
        node_uuid: Uuid::new_v4().to_string(),
        status_version: 3,
        updated_at: chrono::Utc::now().timestamp(),
        reason: "activate".to_string(),
    };
    let update_resp3 =
        TaskServiceTrait::update_task_status(&sms_service, Request::new(update_req3))
            .await
            .unwrap();
    assert!(update_resp3.get_ref().success);

    let get_resp3 = TaskServiceTrait::get_task(
        &sms_service,
        Request::new(GetTaskRequest {
            task_id: task_id.clone(),
        }),
    )
    .await
    .unwrap();
    let task3 = get_resp3.get_ref().task.as_ref().unwrap();
    assert_eq!(task3.status, TaskStatus::Active as i32);
}

#[tokio::test]
async fn register_task_generates_node_local_assignment() {
    let sms_service = create_test_sms_service().await;
    let node_uuid = Uuid::new_v4().to_string();
    NodeServiceTrait::register_node(
        &sms_service,
        Request::new(RegisterNodeRequest {
            node: Some(Node {
                uuid: node_uuid.clone(),
                ip_address: "127.0.0.1".to_string(),
                port: 12345,
                http_port: 0,
                status: "online".to_string(),
                last_heartbeat: chrono::Utc::now().timestamp(),
                registered_at: chrono::Utc::now().timestamp(),
                metadata: std::collections::HashMap::new(),
            }),
        }),
    )
    .await
    .unwrap();

    TaskServiceTrait::register_task(
        &sms_service,
        Request::new(RegisterTaskRequest {
            name: "echo".to_string(),
            description: "simple echo".to_string(),
            priority: TaskPriority::Normal as i32,
            endpoint: "echo".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec!["echo".to_string()],
            metadata: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
            executable: None,
            desired_replicas: 2,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        }),
    )
    .await
    .unwrap();

    let assignments = TaskPlacementAssignmentServiceTrait::list_node_task_assignments(
        &sms_service,
        Request::new(ListNodeTaskAssignmentsRequest { node_uuid }),
    )
    .await
    .unwrap()
    .into_inner()
    .assignments;
    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].desired_instances, 2);
}

#[tokio::test]
async fn test_update_task_status_nonexistent_task() {
    let sms_service = create_test_sms_service().await;

    let update_req = UpdateTaskStatusRequest {
        task_id: Uuid::new_v4().to_string(),
        status: TaskStatus::Active as i32,
        node_uuid: Uuid::new_v4().to_string(),
        status_version: 1,
        updated_at: chrono::Utc::now().timestamp(),
        reason: "activate".to_string(),
    };
    let update_resp = TaskServiceTrait::update_task_status(&sms_service, Request::new(update_req))
        .await
        .unwrap();
    assert!(!update_resp.get_ref().success);
}

#[tokio::test]
async fn test_register_task_sets_registered_status() {
    let sms_service = create_test_sms_service().await;

    let req = RegisterTaskRequest {
        name: "echo".to_string(),
        description: "simple echo".to_string(),
        priority: TaskPriority::Normal as i32,
        endpoint: "echo".to_string(),
        version: "1.0.0".to_string(),
        capabilities: vec!["echo".to_string()],
        metadata: std::collections::HashMap::new(),
        config: std::collections::HashMap::new(),
        executable: None,
        desired_replicas: 1,
        scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
    };
    let resp = TaskServiceTrait::register_task(&sms_service, Request::new(req))
        .await
        .unwrap();
    let registered = resp.get_ref();
    assert!(registered.success);
    let task_id = registered.task_id.clone();

    let get_resp = TaskServiceTrait::get_task(
        &sms_service,
        Request::new(GetTaskRequest {
            task_id: task_id.clone(),
        }),
    )
    .await
    .unwrap();
    let task = get_resp.get_ref().task.as_ref().unwrap();
    assert_eq!(task.status, TaskStatus::Registered as i32);
}

#[tokio::test]
async fn test_delete_task_marks_deleting_and_hides_endpoint() {
    let sms_service = create_test_sms_service().await;

    let register_resp = TaskServiceTrait::register_task(
        &sms_service,
        Request::new(RegisterTaskRequest {
            name: "echo".to_string(),
            description: "simple echo".to_string(),
            priority: TaskPriority::Normal as i32,
            endpoint: "echo-delete".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec!["echo".to_string()],
            metadata: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
            executable: None,
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        }),
    )
    .await
    .unwrap();
    let task_id = register_resp.get_ref().task_id.clone();

    let delete_resp = TaskServiceTrait::delete_task(
        &sms_service,
        Request::new(DeleteTaskRequest {
            task_id: task_id.clone(),
            reason: "cleanup".to_string(),
            force: false,
        }),
    )
    .await
    .unwrap();
    assert!(delete_resp.get_ref().success);
    let deleted_task = delete_resp.get_ref().task.as_ref().unwrap();
    assert_eq!(deleted_task.status, TaskStatus::Deleting as i32);
    assert_eq!(deleted_task.deletion_reason, "cleanup");

    let get_resp = TaskServiceTrait::get_task(
        &sms_service,
        Request::new(GetTaskRequest {
            task_id: task_id.clone(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        get_resp.get_ref().task.as_ref().unwrap().status,
        TaskStatus::Deleting as i32
    );

    let resolve_resp = TaskServiceTrait::resolve_endpoint(
        &sms_service,
        Request::new(ResolveEndpointRequest {
            endpoint: "echo-delete".to_string(),
        }),
    )
    .await
    .unwrap();
    assert!(!resolve_resp.get_ref().found);

    let complete_resp = TaskServiceTrait::complete_task_deletion(
        &sms_service,
        Request::new(CompleteTaskDeletionRequest { task_id }),
    )
    .await
    .unwrap();
    assert!(complete_resp.get_ref().success);
}

#[tokio::test]
async fn complete_task_deletion_waits_for_active_instances_to_disappear() {
    let sms_service = create_test_sms_service().await;
    let register_resp = TaskServiceTrait::register_task(
        &sms_service,
        Request::new(RegisterTaskRequest {
            name: "task-delete-wait".to_string(),
            description: "task-delete-wait".to_string(),
            priority: TaskPriority::Normal as i32,
            endpoint: "task-delete-wait".to_string(),
            version: "v1".to_string(),
            capabilities: vec!["echo".to_string()],
            metadata: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
            executable: None,
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        }),
    )
    .await
    .unwrap();
    let task_id = register_resp
        .get_ref()
        .task
        .as_ref()
        .unwrap()
        .task_id
        .clone();
    let now_ms = chrono::Utc::now().timestamp_millis();

    InstanceRegistryServiceTrait::report_instance(
        &sms_service,
        Request::new(Instance {
            instance_id: Uuid::new_v4().to_string(),
            task_id: task_id.clone(),
            node_uuid: "node-a".to_string(),
            status: InstanceStatus::Running as i32,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            last_seen_ms: now_ms,
            current_execution_id: String::new(),
            metadata: std::collections::HashMap::new(),
        }),
    )
    .await
    .unwrap();

    TaskServiceTrait::delete_task(
        &sms_service,
        Request::new(DeleteTaskRequest {
            task_id: task_id.clone(),
            reason: "delete".to_string(),
            force: false,
        }),
    )
    .await
    .unwrap();

    let pending = TaskServiceTrait::complete_task_deletion(
        &sms_service,
        Request::new(CompleteTaskDeletionRequest {
            task_id: task_id.clone(),
        }),
    )
    .await
    .unwrap()
    .into_inner();
    assert!(!pending.success);

    let instance_id = ExecutionIndexServiceTrait::list_task_instances(
        &sms_service,
        Request::new(ListTaskInstancesRequest {
            task_id: task_id.clone(),
            limit: 10,
            page_token: String::new(),
        }),
    )
    .await
    .unwrap()
    .into_inner()
    .instances
    .first()
    .unwrap()
    .instance_id
    .clone();

    InstanceRegistryServiceTrait::delete_instance(
        &sms_service,
        Request::new(crate::proto::sms::DeleteInstanceRequest {
            instance_id,
            task_id: task_id.clone(),
            deleted_at_ms: now_ms + 1,
        }),
    )
    .await
    .unwrap();

    let complete = TaskServiceTrait::complete_task_deletion(
        &sms_service,
        Request::new(CompleteTaskDeletionRequest { task_id: task_id }),
    )
    .await
    .unwrap()
    .into_inner();
    assert!(complete.success);
}

#[tokio::test]
async fn test_deleting_task_preserves_status_on_runtime_updates() {
    let sms_service = create_test_sms_service().await;
    let node_uuid = Uuid::new_v4().to_string();

    let register_resp = TaskServiceTrait::register_task(
        &sms_service,
        Request::new(RegisterTaskRequest {
            name: "echo".to_string(),
            description: "simple echo".to_string(),
            priority: TaskPriority::Normal as i32,
            endpoint: "echo-preserve".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec!["echo".to_string()],
            metadata: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
            executable: None,
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        }),
    )
    .await
    .unwrap();
    let task_id = register_resp.get_ref().task_id.clone();

    TaskServiceTrait::delete_task(
        &sms_service,
        Request::new(DeleteTaskRequest {
            task_id: task_id.clone(),
            reason: "cleanup".to_string(),
            force: true,
        }),
    )
    .await
    .unwrap();

    let update_resp = TaskServiceTrait::update_task_status(
        &sms_service,
        Request::new(UpdateTaskStatusRequest {
            task_id: task_id.clone(),
            status: TaskStatus::Inactive as i32,
            node_uuid: node_uuid.clone(),
            status_version: 2,
            updated_at: chrono::Utc::now().timestamp(),
            reason: "runtime cleanup".to_string(),
        }),
    )
    .await
    .unwrap();
    assert!(update_resp.get_ref().success);
    assert_eq!(
        update_resp.get_ref().task.as_ref().unwrap().status,
        TaskStatus::Deleting as i32
    );

    let complete_resp = TaskServiceTrait::complete_task_deletion(
        &sms_service,
        Request::new(CompleteTaskDeletionRequest { task_id }),
    )
    .await
    .unwrap();
    assert!(complete_resp.get_ref().success);
}
