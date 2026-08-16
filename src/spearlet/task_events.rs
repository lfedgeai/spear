use std::{sync::Arc, time::Duration};
use prost::Message;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use tonic::transport::Channel;

use crate::proto::sms::{
    events_service_client::EventsServiceClient, subscribe_events_selector::Selector, EventEnvelope,
    EventOp, ResourceType, SubscribeEventsRequest, SubscribeEventsSelector, TaskEvent, TaskEventKind,
};
#[cfg(test)]
use crate::proto::sms::Task;
use crate::spearlet::config::SpearletConfig;
use crate::spearlet::execution::{manager::TaskExecutionManager, ExecutionError};
use crate::spearlet::task_event_cursor::TaskEventCursorStore;
use tracing::{debug, info, warn};

pub struct TaskEventSubscriber {
    config: Arc<SpearletConfig>,
    sms_channel: Option<Channel>,
    last_event_seq: Arc<RwLock<u64>>,
    execution_manager: Arc<TaskExecutionManager>,
    cursor_store: TaskEventCursorStore,
}

impl TaskEventSubscriber {
    pub fn new(
        config: Arc<SpearletConfig>,
        sms_channel: Option<Channel>,
        execution_manager: Arc<TaskExecutionManager>,
    ) -> Self {
        let cursor_store = TaskEventCursorStore::new(&config);
        let last = cursor_store.load();
        Self {
            config,
            sms_channel,
            last_event_seq: Arc::new(RwLock::new(last)),
            execution_manager,
            cursor_store,
        }
    }

    pub async fn start(self) {
        let cfg = self.config.clone();
        let sms_channel = self.sms_channel.clone();
        let exec_mgr = self.execution_manager.clone();
        let last_event_seq = self.last_event_seq.clone();
        let cursor_store = self.cursor_store.clone();
        tokio::spawn(async move {
            let node_uuid = cfg.compute_node_uuid();
            info!(node_uuid = %node_uuid, sms_grpc_addr = %cfg.sms_grpc_addr, "TaskEventSubscriber starting");
            let Some(channel) = sms_channel else {
                warn!("TaskEventSubscriber disabled: sms_channel is not initialized");
                return;
            };
            loop {
                let mut events_client = EventsServiceClient::new(channel.clone());
                let last = *last_event_seq.read().await;
                let req = SubscribeEventsRequest {
                    selector: Some(SubscribeEventsSelector {
                        selector: Some(Selector::ResourceType(ResourceType::Task as i32)),
                    }),
                    after_seq: last,
                    replay_limit: 1000,
                };
                debug!(node_uuid = %node_uuid, after_seq = last, "Subscribing to global task events");
                let per_attempt = Duration::from_millis(cfg.sms_connect_timeout_ms)
                    .min(Duration::from_secs(5))
                    .max(Duration::from_millis(1));
                let mut stream = match tokio::time::timeout(
                    per_attempt,
                    events_client.subscribe_events(req),
                )
                .await
                {
                    Ok(Ok(r)) => r.into_inner(),
                    Ok(Err(e)) => {
                        warn!(error = %e, "SubscribeEvents RPC failed, retrying");
                        tokio::time::sleep(Duration::from_millis(cfg.sms_connect_retry_ms)).await;
                        continue;
                    }
                    Err(_) => {
                        warn!("SubscribeEvents RPC timeout, retrying");
                        tokio::time::sleep(Duration::from_millis(cfg.sms_connect_retry_ms)).await;
                        continue;
                    }
                };
                loop {
                    match stream.next().await {
                        Some(Ok(env)) => {
                            debug!(
                                event_id = %env.event_id,
                                seq = env.seq,
                                resource_type = env.resource_type,
                                resource_id = %env.resource_id,
                                node_uuid = %env.node_uuid,
                                "Received unified event"
                            );
                            let seq = env.seq;
                            let event_id = env.event_id.clone();
                            match Self::handle_envelope(&cfg, &exec_mgr, env).await {
                                Ok(()) => {
                                    *last_event_seq.write().await = seq;
                                    cursor_store.store(seq);
                                }
                                Err(error) => {
                                    warn!(event_id = %event_id, seq, error = %error, "Task event handling failed before cursor commit; reconnecting for replay");
                                    break;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            warn!(error = %e, "Event stream error, reconnecting");
                            break;
                        }
                        None => {
                            break;
                        }
                    }
                }
                debug!(
                    delay_ms = cfg.sms_connect_retry_ms,
                    "Reconnect delay before resubscribing"
                );
                tokio::time::sleep(Duration::from_millis(cfg.sms_connect_retry_ms)).await;
            }
        });
    }

    async fn handle_envelope(
        _cfg: &SpearletConfig,
        mgr: &Arc<TaskExecutionManager>,
        env: EventEnvelope,
    ) -> Result<(), String> {
        if env.resource_type != ResourceType::Task as i32 {
            debug!(event_id = %env.event_id, resource_type = env.resource_type, "Ignoring non-task unified event");
            return Ok(());
        }
        if !matches!(env.op, x if x == EventOp::Create as i32 || x == EventOp::Cancel as i32 || x == EventOp::Update as i32) {
            debug!(event_id = %env.event_id, op = env.op, "Ignoring unsupported task unified event op");
            return Ok(());
        }
        let Some(payload) = env.payload else {
            debug!(event_id = %env.event_id, "Ignoring unified task event without payload");
            return Ok(());
        };
        if payload.type_url != "type.googleapis.com/sms.TaskEvent" {
            debug!(event_id = %env.event_id, type_url = %payload.type_url, "Ignoring unified task event with unexpected payload type");
            return Ok(());
        }
        let Ok(event) = TaskEvent::decode(payload.value.as_slice()) else {
            warn!(event_id = %env.event_id, "Failed to decode TaskEvent payload from unified event");
            return Ok(());
        };
        Self::dispatch_task_event(_cfg, mgr, event).await
    }

    async fn dispatch_task_event(
        _cfg: &SpearletConfig,
        mgr: &Arc<TaskExecutionManager>,
        ev: TaskEvent,
    ) -> Result<(), String> {
        if ev.kind == TaskEventKind::Create as i32 {
            match mgr.materialize_local_task_from_sms_create_event(&ev.task_id).await {
                Ok(_) => {}
                Err(ExecutionError::TaskNotFound { .. }) => {
                    warn!(
                        event_id = ev.event_id,
                        task_id = %ev.task_id,
                        "Ignoring stale task create event for missing task"
                    );
                }
                Err(error) => return Err(error.to_string()),
            }
        } else if ev.kind == TaskEventKind::Update as i32 {
            match mgr.fetch_and_materialize_local_task_by_id(&ev.task_id).await {
                Ok(_) => {}
                Err(ExecutionError::TaskNotFound { .. }) => {
                    warn!(
                        event_id = ev.event_id,
                        task_id = %ev.task_id,
                        "Ignoring stale task update event for missing task"
                    );
                }
                Err(error) => return Err(error.to_string()),
            }
        } else if ev.kind == TaskEventKind::Cancel as i32 {
            mgr.delete_task_runtime(&ev.task_id, Some("task delete event".to_string()))
                .await
                .map_err(|error| error.to_string())?;
        } else {
            debug!(event_id = ev.event_id, kind = ev.kind, task_id = %ev.task_id, "Unhandled TaskEvent kind, ignoring");
        }
        Ok(())
    }

    #[cfg(test)]
    async fn sync_task_from_snapshot_for_test(
        mgr: &Arc<TaskExecutionManager>,
        task: &Task,
    ) -> crate::spearlet::execution::ExecutionResult<()> {
        let spear_task = mgr.materialize_local_task_from_sms_snapshot(task).await?;
        spear_task.set_status(crate::spearlet::execution::task::TaskStatus::Ready);
        Ok(())
    }

    #[cfg(test)]
    pub async fn handle_event_for_test(&self, ev: TaskEvent, task: Option<Task>) {
        if ev.kind == TaskEventKind::Create as i32 {
            if let Some(t) = task {
                Self::sync_task_from_snapshot_for_test(&self.execution_manager, &t)
                    .await
                    .unwrap();
            }
        } else if ev.kind == TaskEventKind::Cancel as i32 {
            self.execution_manager
                .delete_task_runtime(&ev.task_id, Some("task delete event".to_string()))
                .await
                .unwrap();
        } else {
            tracing::debug!(event_id = ev.event_id, kind = ev.kind, task_id = %ev.task_id, "Unhandled TaskEvent kind in test");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::sms::{
        task_service_server::TaskServiceServer,
        Task, TaskEvent, TaskEventKind, TaskExecutable, TaskPriority, TaskStatus,
    };
    use crate::sms::service::SmsServiceImpl;
    use crate::spearlet::execution::instance;
    use crate::spearlet::execution::runtime::{Runtime, RuntimeCapabilities, RuntimeType};
    use crate::spearlet::execution::TaskExecutionManagerConfig;
    use async_trait::async_trait;
    use sha2::Digest;
    use std::collections::HashMap as StdHashMap;
    use tokio::net::TcpListener;
    use tonic::transport::{Channel, Server};

    struct DummyRuntime {
        ty: RuntimeType,
    }

    async fn start_task_sms_grpc() -> (tokio::task::JoinHandle<()>, String, Channel) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let sms_service =
            SmsServiceImpl::with_storage_config(&crate::config::base::StorageConfig {
                backend: "memory".to_string(),
                ..Default::default()
            })
            .await;

        let handle = tokio::spawn(async move {
            Server::builder()
                .add_service(TaskServiceServer::new(sms_service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        let sms_addr = format!("127.0.0.1:{}", addr.port());
        let channel = Channel::from_shared(format!("http://{}", sms_addr))
            .unwrap()
            .connect()
            .await
            .unwrap();
        (handle, sms_addr, channel)
    }

    #[async_trait]
    impl Runtime for DummyRuntime {
        fn runtime_type(&self) -> RuntimeType {
            self.ty
        }
        async fn create_instance(
            &self,
            config: &instance::InstanceConfig,
        ) -> crate::spearlet::execution::ExecutionResult<Arc<instance::TaskInstance>> {
            Ok(Arc::new(instance::TaskInstance::new(
                config.task_id.clone(),
                config.clone(),
            )))
        }
        async fn start_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> crate::spearlet::execution::ExecutionResult<()> {
            Ok(())
        }
        async fn stop_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> crate::spearlet::execution::ExecutionResult<()> {
            Ok(())
        }
        async fn execute(
            &self,
            _instance: &Arc<instance::TaskInstance>,
            _context: crate::spearlet::execution::runtime::ExecutionContext,
        ) -> crate::spearlet::execution::ExecutionResult<
            crate::spearlet::execution::runtime::RuntimeExecutionResponse,
        > {
            Ok(crate::spearlet::execution::runtime::RuntimeExecutionResponse::default())
        }
        async fn health_check(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> crate::spearlet::execution::ExecutionResult<bool> {
            Ok(true)
        }
        async fn get_metrics(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> crate::spearlet::execution::ExecutionResult<StdHashMap<String, serde_json::Value>>
        {
            Ok(StdHashMap::new())
        }
        async fn scale_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
            _new_limits: &instance::InstanceResourceLimits,
        ) -> crate::spearlet::execution::ExecutionResult<()> {
            Ok(())
        }
        async fn cleanup_instance(
            &self,
            _instance: &Arc<instance::TaskInstance>,
        ) -> crate::spearlet::execution::ExecutionResult<()> {
            Ok(())
        }
        fn validate_config(
            &self,
            _config: &instance::InstanceConfig,
        ) -> crate::spearlet::execution::ExecutionResult<()> {
            Ok(())
        }
        fn get_capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities::default()
        }
    }

    #[tokio::test]
    async fn test_event_existing_task_uses_task_id_and_checksum_artifact_id() {
        let mut rm = crate::spearlet::execution::runtime::RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DummyRuntime {
                ty: RuntimeType::Process,
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);
        let mgr = TaskExecutionManager::new(
            TaskExecutionManagerConfig::default(),
            rm,
            Arc::new(SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let mut cfg = SpearletConfig::default();
        cfg.node_name = uuid::Uuid::new_v4().to_string();
        let sub = TaskEventSubscriber::new(Arc::new(cfg.clone()), None, mgr.clone());

        let mut meta = std::collections::HashMap::new();
        meta.insert("version".to_string(), "v1".to_string());
        let sms_task = Task {
            task_id: "task-x".to_string(),
            name: "t".to_string(),
            description: String::new(),
            status: TaskStatus::Registered as i32,
            priority: TaskPriority::Normal as i32,
            endpoint: String::new(),
            version: "v1".to_string(),
            capabilities: vec![],
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            metadata: meta,
            config: std::collections::HashMap::new(),
            executable: Some(TaskExecutable {
                r#type: 5,
                uri: "http://example/bin".to_string(),
                name: String::new(),
                checksum_sha256: "deadbeef".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
            }),
            result_uris: Vec::new(),
            last_result_uri: String::new(),
            last_result_status: String::new(),
            last_completed_at: 0,
            last_result_metadata: std::collections::HashMap::new(),
            deletion_requested_at: 0,
            deletion_reason: String::new(),
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        };
        let ev = TaskEvent {
            event_id: 1,
            ts: chrono::Utc::now().timestamp(),
            task_id: "task-x".to_string(),
            kind: TaskEventKind::Create as i32,
            execution_id: None,
        };
        sub.handle_event_for_test(ev, Some(sms_task)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let local_task = mgr.get_task_by_id("task-x").expect("task should exist");
        assert_eq!(local_task.status(), crate::spearlet::execution::task::TaskStatus::Ready);
        assert!(mgr.get_artifact_by_id("deadbeef").is_some());
    }

    #[tokio::test]
    async fn test_event_artifact_id_hashes_uri_when_no_checksum() {
        let mut rm = crate::spearlet::execution::runtime::RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DummyRuntime {
                ty: RuntimeType::Process,
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);
        let mgr = TaskExecutionManager::new(
            TaskExecutionManagerConfig::default(),
            rm,
            Arc::new(SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let mut cfg = SpearletConfig::default();
        cfg.node_name = uuid::Uuid::new_v4().to_string();
        let sub = TaskEventSubscriber::new(Arc::new(cfg.clone()), None, mgr.clone());

        let uri = "http://example/abc";
        let sms_task = Task {
            task_id: "task-y".to_string(),
            name: "t".to_string(),
            description: String::new(),
            status: TaskStatus::Registered as i32,
            priority: TaskPriority::Normal as i32,
            endpoint: String::new(),
            version: "v1".to_string(),
            capabilities: vec![],
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
            executable: Some(TaskExecutable {
                r#type: 5,
                uri: uri.to_string(),
                name: String::new(),
                checksum_sha256: String::new(),
                args: vec![],
                env: std::collections::HashMap::new(),
            }),
            result_uris: Vec::new(),
            last_result_uri: String::new(),
            last_result_status: String::new(),
            last_completed_at: 0,
            last_result_metadata: std::collections::HashMap::new(),
            deletion_requested_at: 0,
            deletion_reason: String::new(),
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        };
        let ev = TaskEvent {
            event_id: 2,
            ts: chrono::Utc::now().timestamp(),
            task_id: "task-y".to_string(),
            kind: TaskEventKind::Create as i32,
            execution_id: None,
        };
        sub.handle_event_for_test(ev, Some(sms_task)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let d = sha2::Sha256::digest(uri.as_bytes());
        let expected: String = d.iter().map(|b| format!("{:02x}", b)).collect();
        assert!(mgr.get_task_by_id("task-y").is_some());
        assert!(mgr.get_artifact_by_id(&expected).is_some());
    }

    #[tokio::test]
    async fn test_cancel_event_removes_local_task_runtime() {
        let mut rm = crate::spearlet::execution::runtime::RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DummyRuntime {
                ty: RuntimeType::Process,
            }),
        )
        .unwrap();
        let rm = Arc::new(rm);
        let mgr = TaskExecutionManager::new(
            TaskExecutionManagerConfig::default(),
            rm,
            Arc::new(SpearletConfig::default()),
            None,
        )
        .await
        .unwrap();

        let mut cfg = SpearletConfig::default();
        cfg.node_name = uuid::Uuid::new_v4().to_string();
        let sub = TaskEventSubscriber::new(Arc::new(cfg.clone()), None, mgr.clone());
        let sms_task = Task {
            task_id: "task-z".to_string(),
            name: "t".to_string(),
            description: String::new(),
            status: TaskStatus::Registered as i32,
            priority: TaskPriority::Normal as i32,
            endpoint: String::new(),
            version: "v1".to_string(),
            capabilities: vec![],
            registered_at: chrono::Utc::now().timestamp(),
            last_heartbeat: chrono::Utc::now().timestamp(),
            metadata: std::collections::HashMap::new(),
            config: std::collections::HashMap::new(),
            executable: Some(TaskExecutable {
                r#type: 5,
                uri: "http://example/delete".to_string(),
                name: String::new(),
                checksum_sha256: "deadbeef2".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
            }),
            result_uris: Vec::new(),
            last_result_uri: String::new(),
            last_result_status: String::new(),
            last_completed_at: 0,
            last_result_metadata: std::collections::HashMap::new(),
            deletion_requested_at: 0,
            deletion_reason: String::new(),
            desired_replicas: 1,
            scheduling_strategy: crate::proto::sms::TaskSchedulingStrategy::Spread as i32,
        };
        sub.handle_event_for_test(
            TaskEvent {
                event_id: 3,
                ts: chrono::Utc::now().timestamp(),
                task_id: "task-z".to_string(),
                kind: TaskEventKind::Create as i32,
                execution_id: None,
            },
            Some(sms_task),
        )
        .await;
        assert!(mgr.get_task_by_id("task-z").is_some());

        sub.handle_event_for_test(
            TaskEvent {
                event_id: 4,
                ts: chrono::Utc::now().timestamp(),
                task_id: "task-z".to_string(),
                kind: TaskEventKind::Cancel as i32,
                execution_id: None,
            },
            None,
        )
        .await;
        assert!(mgr.get_task_by_id("task-z").is_none());
    }

    #[tokio::test]
    async fn test_stale_update_event_for_missing_task_is_ignored() {
        let (sms_handle, sms_addr, sms_channel) = start_task_sms_grpc().await;
        let mut rm = crate::spearlet::execution::runtime::RuntimeManager::new();
        rm.register_runtime(
            RuntimeType::Process,
            Box::new(DummyRuntime {
                ty: RuntimeType::Process,
            }),
        )
        .unwrap();
        let mut cfg = SpearletConfig::default();
        cfg.sms_grpc_addr = sms_addr;
        let mgr = TaskExecutionManager::new(
            TaskExecutionManagerConfig::default(),
            Arc::new(rm),
            Arc::new(cfg.clone()),
            Some(sms_channel),
        )
        .await
        .unwrap();

        let result = TaskEventSubscriber::dispatch_task_event(
            &cfg,
            &mgr,
            TaskEvent {
                event_id: 5,
                ts: chrono::Utc::now().timestamp(),
                task_id: "missing-task".to_string(),
                kind: TaskEventKind::Update as i32,
                execution_id: None,
            },
        )
        .await;

        assert!(result.is_ok());
        assert!(mgr.get_task_by_id("missing-task").is_none());
        sms_handle.abort();
    }
}
