#[cfg(test)]
mod tests {
    use super::super::unified_events::UnifiedEventBus;
    use crate::proto::sms::{
        EventOp, ResourceType, Task, TaskEvent, TaskEventKind, TaskPriority, TaskStatus,
    };
    use crate::storage::kv::MemoryKvStore;
    use prost::Message;
    use std::sync::Arc;

    fn sample_task(name: &str) -> Task {
        Task {
            task_id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: "".to_string(),
            status: TaskStatus::Registered as i32,
            priority: TaskPriority::Normal as i32,
            endpoint: "".to_string(),
            version: "v1".to_string(),
            capabilities: vec![],
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            metadata: Default::default(),
            config: Default::default(),
            executable: None,
            result_uris: Vec::new(),
            last_result_uri: String::new(),
            last_result_status: String::new(),
            last_completed_at: 0,
            last_result_metadata: Default::default(),
            deletion_requested_at: 0,
            deletion_reason: String::new(),
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        }
    }

    #[tokio::test]
    async fn test_publish_and_subscribe_task_event() {
        let kv: Arc<dyn crate::storage::kv::KvStore> = Arc::new(MemoryKvStore::new());
        let bus = UnifiedEventBus::new(kv);

        let stream = "type.task".to_string();
        let mut rx = bus.subscribe(&stream).await;

        let t = sample_task("t1");
        let seq = bus
            .publish_task_event(&t, TaskEventKind::Create)
            .await
            .unwrap();

        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.stream, stream);
        assert_eq!(ev.seq, seq);
        assert_eq!(ev.resource_type, ResourceType::Task as i32);
        assert_eq!(ev.resource_id, t.task_id);
        assert_eq!(ev.op, EventOp::Create as i32);

        let any = ev.payload.unwrap();
        let task_ev = TaskEvent::decode(any.value.as_slice()).unwrap();
        assert_eq!(task_ev.task_id, ev.resource_id);
        assert_eq!(task_ev.kind, TaskEventKind::Create as i32);
    }

    #[tokio::test]
    async fn test_multistream_write_all_and_type_and_resource() {
        let kv: Arc<dyn crate::storage::kv::KvStore> = Arc::new(MemoryKvStore::new());
        let bus = UnifiedEventBus::new(kv);

        let all_stream = "all".to_string();
        let type_stream = "type.task".to_string();

        let t = sample_task("t1");

        let mut rx_all = bus.subscribe(&all_stream).await;
        let mut rx_type = bus.subscribe(&type_stream).await;
        let mut rx_res = bus.subscribe(&format!("resource.task.{}", t.task_id)).await;

        let _ = bus
            .publish_task_event(&t, TaskEventKind::Create)
            .await
            .unwrap();

        let e_all = rx_all.recv().await.unwrap();
        let e_type = rx_type.recv().await.unwrap();
        let e_res = rx_res.recv().await.unwrap();

        assert_eq!(e_all.resource_id, t.task_id);
        assert_eq!(e_type.resource_id, t.task_id);
        assert_eq!(e_res.resource_id, t.task_id);
        assert_eq!(e_all.op, EventOp::Create as i32);
        assert_eq!(e_type.op, EventOp::Create as i32);
        assert_eq!(e_res.op, EventOp::Create as i32);
    }

    #[tokio::test]
    async fn test_durable_replay_since_seq() {
        let kv: Arc<dyn crate::storage::kv::KvStore> = Arc::new(MemoryKvStore::new());
        let bus = UnifiedEventBus::new(kv);

        let stream = "type.task".to_string();

        let t1 = sample_task("t1");
        let s1 = bus
            .publish_task_event(&t1, TaskEventKind::Create)
            .await
            .unwrap();
        let t2 = sample_task("t2");
        let s2 = bus
            .publish_task_event(&t2, TaskEventKind::Create)
            .await
            .unwrap();

        let replay = bus.replay_since(&stream, s1, 100).await.unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].seq, s2);
        assert_eq!(replay[0].resource_id, t2.task_id);
    }
}
