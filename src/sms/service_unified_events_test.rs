#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use prost::Message;
    use tokio::sync::RwLock;
    use tokio_stream::StreamExt;
    use tonic::Request;

    use crate::proto::sms::{
        events_service_server::EventsService as EventsServiceTrait,
        node_service_server::NodeService as NodeServiceTrait, subscribe_events_selector::Selector,
        task_service_server::TaskService as TaskServiceTrait, AllEventsSelector, EventOp, Node,
        RegisterNodeRequest, RegisterTaskRequest, ResourceType, SubscribeEventsRequest,
        SubscribeEventsSelector, TaskPriority, UpdateTaskStatusRequest,
    };
    use crate::sms::config::SmsConfig;
    use crate::sms::service::SmsServiceImpl;
    use crate::sms::services::{node_service::NodeService, resource_service::ResourceService};

    fn make_register() -> RegisterTaskRequest {
        RegisterTaskRequest {
            name: "t".to_string(),
            description: "d".to_string(),
            priority: TaskPriority::Normal as i32,
            endpoint: "t".to_string(),
            version: "v1".to_string(),
            capabilities: vec![],
            metadata: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
            executable: None,
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        }
    }

    #[tokio::test]
    async fn test_subscribe_events_replay_for_task_create_and_update() {
        let svc = SmsServiceImpl::with_storage_config(&crate::config::base::StorageConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;

        let reg = TaskServiceTrait::register_task(&svc, Request::new(make_register()))
            .await
            .unwrap()
            .into_inner();
        let task_id = reg.task_id;
        let node_uuid = uuid::Uuid::new_v4().to_string();

        let req = SubscribeEventsRequest {
            selector: Some(SubscribeEventsSelector {
                selector: Some(Selector::ResourceType(ResourceType::Task as i32)),
            }),
            after_seq: 0,
            replay_limit: 100,
        };
        let mut stream = EventsServiceTrait::subscribe_events(&svc, Request::new(req))
            .await
            .unwrap()
            .into_inner();

        let first = stream.next().await.unwrap().unwrap();
        assert!(first.node_uuid.is_empty());
        assert_eq!(first.resource_id, task_id);
        let first_seq = first.seq;

        let upd = UpdateTaskStatusRequest {
            task_id: first.resource_id.clone(),
            status: 2,
            node_uuid: node_uuid.clone(),
            status_version: 1,
            updated_at: 0,
            reason: "test".to_string(),
        };
        let _ = TaskServiceTrait::update_task_status(&svc, Request::new(upd))
            .await
            .unwrap();

        let req2 = SubscribeEventsRequest {
            selector: Some(SubscribeEventsSelector {
                selector: Some(Selector::ResourceType(ResourceType::Task as i32)),
            }),
            after_seq: first_seq,
            replay_limit: 100,
        };
        let mut stream2 = EventsServiceTrait::subscribe_events(&svc, Request::new(req2))
            .await
            .unwrap()
            .into_inner();
        let second = stream2.next().await.unwrap().unwrap();
        assert!(second.seq > first_seq);
        assert_eq!(second.resource_id, task_id);
    }

    #[tokio::test]
    async fn test_subscribe_events_replay_for_node_register() {
        let svc = SmsServiceImpl::with_storage_config(&crate::config::base::StorageConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;

        let node_uuid = uuid::Uuid::new_v4().to_string();
        let node = Node {
            uuid: node_uuid.clone(),
            ip_address: "127.0.0.1".to_string(),
            port: 12345,
            http_port: 0,
            status: "online".to_string(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            registered_at: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
        };
        let _ = NodeServiceTrait::register_node(
            &svc,
            Request::new(RegisterNodeRequest { node: Some(node) }),
        )
        .await
        .unwrap();

        let req = SubscribeEventsRequest {
            selector: Some(SubscribeEventsSelector {
                selector: Some(Selector::NodeUuid(node_uuid.clone())),
            }),
            after_seq: 0,
            replay_limit: 100,
        };
        let mut stream = EventsServiceTrait::subscribe_events(&svc, Request::new(req))
            .await
            .unwrap()
            .into_inner();
        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.node_uuid, node_uuid);
        assert_eq!(first.resource_type, ResourceType::Node as i32);
        assert_eq!(first.op, EventOp::Create as i32);
        assert_eq!(first.resource_id, first.node_uuid);
    }

    #[tokio::test]
    async fn test_subscribe_events_by_resource_type_task() {
        let svc = SmsServiceImpl::with_storage_config(&crate::config::base::StorageConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;

        let reg = TaskServiceTrait::register_task(&svc, Request::new(make_register()))
            .await
            .unwrap()
            .into_inner();

        let req = SubscribeEventsRequest {
            selector: Some(SubscribeEventsSelector {
                selector: Some(Selector::ResourceType(ResourceType::Task as i32)),
            }),
            after_seq: 0,
            replay_limit: 100,
        };
        let mut stream = EventsServiceTrait::subscribe_events(&svc, Request::new(req))
            .await
            .unwrap()
            .into_inner();

        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.resource_type, ResourceType::Task as i32);
        assert_eq!(first.resource_id, reg.task_id);
    }

    #[tokio::test]
    async fn test_subscribe_events_all_selector() {
        let svc = SmsServiceImpl::with_storage_config(&crate::config::base::StorageConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;

        let reg = TaskServiceTrait::register_task(&svc, Request::new(make_register()))
            .await
            .unwrap()
            .into_inner();

        let req = SubscribeEventsRequest {
            selector: Some(SubscribeEventsSelector {
                selector: Some(Selector::All(AllEventsSelector {})),
            }),
            after_seq: 0,
            replay_limit: 100,
        };
        let mut stream = EventsServiceTrait::subscribe_events(&svc, Request::new(req))
            .await
            .unwrap()
            .into_inner();

        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(first.resource_id, reg.task_id);
    }

    #[tokio::test]
    async fn test_node_disconnect_marks_offline_and_emits_event() {
        let node_service = Arc::new(RwLock::new(NodeService::new()));
        let resource_service = Arc::new(ResourceService::new());
        let mut cfg = SmsConfig::default();
        cfg.cleanup_interval = 1;
        cfg.heartbeat_timeout = 1;
        let svc = SmsServiceImpl::new(node_service, resource_service, Arc::new(cfg)).await;

        let node_uuid = uuid::Uuid::new_v4().to_string();
        let node = Node {
            uuid: node_uuid.clone(),
            ip_address: "127.0.0.1".to_string(),
            port: 12345,
            http_port: 0,
            status: "online".to_string(),
            last_heartbeat: (chrono::Utc::now() - chrono::Duration::seconds(3600)).timestamp(),
            registered_at: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
        };
        let _ = NodeServiceTrait::register_node(
            &svc,
            Request::new(RegisterNodeRequest { node: Some(node) }),
        )
        .await
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let req = SubscribeEventsRequest {
            selector: Some(SubscribeEventsSelector {
                selector: Some(Selector::NodeUuid(node_uuid.clone())),
            }),
            after_seq: 0,
            replay_limit: 100,
        };
        let mut stream = EventsServiceTrait::subscribe_events(&svc, Request::new(req))
            .await
            .unwrap()
            .into_inner();

        let mut found_offline = false;
        for _ in 0..5 {
            let next = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
                .await
                .unwrap();
            let Some(Ok(ev)) = next else { break };
            if ev.resource_type != ResourceType::Node as i32 {
                continue;
            }
            if ev.op != EventOp::Update as i32 {
                continue;
            }
            let Some(any) = ev.payload else { continue };
            let n = Node::decode(any.value.as_slice()).unwrap();
            if n.status.to_ascii_lowercase() == "offline" {
                found_offline = true;
                break;
            }
        }
        assert!(found_offline);
    }
}
