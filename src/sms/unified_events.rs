use crate::proto::sms::{
    EventEnvelope, EventOp, Execution, Instance, Node, ResourceType, Task, TaskEvent,
    TaskEventKind, TaskPlacementAssignment,
};
use crate::sms::services::error::SmsError;
use crate::storage::kv::{serialization, KvPair, KvStore};
use prost::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

const OUTBOX_PREFIX: &str = "events:";
const COUNTER_PREFIX: &str = "events_counter:";
const MAX_EVENTS_PER_STREAM: u64 = 10_000;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct StoredEnvelope {
    event_id: String,
    ts_ms: i64,
    stream: String,
    seq: u64,
    resource_type: i32,
    resource_id: String,
    op: i32,
    schema_version: u32,
    node_uuid: String,
    correlation_id: String,
    headers: HashMap<String, String>,
    payload_type_url: String,
    payload_value: Vec<u8>,
    payload_bytes: Vec<u8>,
    content_type: String,
}

#[derive(Debug, Clone)]
pub struct UnifiedEventBus {
    kv: Arc<dyn KvStore>,
    channels: Arc<tokio::sync::RwLock<HashMap<String, broadcast::Sender<EventEnvelope>>>>,
    counters: Arc<tokio::sync::RwLock<HashMap<String, u64>>>,
}

impl UnifiedEventBus {
    pub fn new(kv: Arc<dyn KvStore>) -> Self {
        Self {
            kv,
            channels: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            counters: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    fn node_stream(node_uuid: &str) -> String {
        format!("node.{}", node_uuid)
    }

    fn all_stream() -> &'static str {
        "all"
    }

    fn type_stream(resource_type: ResourceType) -> String {
        match resource_type {
            ResourceType::Task => "type.task".to_string(),
            ResourceType::Node => "type.node".to_string(),
            ResourceType::Artifact => "type.artifact".to_string(),
            ResourceType::Instance => "type.instance".to_string(),
            ResourceType::Execution => "type.execution".to_string(),
            ResourceType::TaskAssignment => "type.task_assignment".to_string(),
            ResourceType::Unknown => "type.unknown".to_string(),
        }
    }

    fn resource_stream(resource_type: ResourceType, resource_id: &str) -> String {
        match resource_type {
            ResourceType::Task => format!("resource.task.{}", resource_id),
            ResourceType::Node => format!("resource.node.{}", resource_id),
            ResourceType::Artifact => format!("resource.artifact.{}", resource_id),
            ResourceType::Instance => format!("resource.instance.{}", resource_id),
            ResourceType::Execution => format!("resource.execution.{}", resource_id),
            ResourceType::TaskAssignment => format!("resource.task_assignment.{}", resource_id),
            ResourceType::Unknown => format!("resource.unknown.{}", resource_id),
        }
    }

    async fn next_seq(&self, stream: &str) -> u64 {
        let mut map = self.counters.write().await;
        if !map.contains_key(stream) {
            let key = format!("{}{}", COUNTER_PREFIX, stream);
            let v = self
                .kv
                .get(&key)
                .await
                .ok()
                .flatten()
                .and_then(|b| String::from_utf8(b).ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            map.insert(stream.to_string(), v);
        }
        let e = map.get_mut(stream).unwrap();
        *e = e.saturating_add(1);
        *e
    }

    async fn get_sender(&self, stream: &str) -> broadcast::Sender<EventEnvelope> {
        let mut chans = self.channels.write().await;
        if let Some(tx) = chans.get(stream) {
            return tx.clone();
        }
        let (tx, _rx) = broadcast::channel(1024);
        chans.insert(stream.to_string(), tx.clone());
        tx
    }

    pub async fn subscribe(&self, stream: &str) -> broadcast::Receiver<EventEnvelope> {
        let tx = self.get_sender(stream).await;
        tx.subscribe()
    }

    pub async fn replay_since(
        &self,
        stream: &str,
        after_seq: u64,
        limit: usize,
    ) -> Result<Vec<EventEnvelope>, SmsError> {
        let prefix = format!("{}{}:", OUTBOX_PREFIX, stream);
        let all = self.kv.scan_prefix(&prefix).await?;
        let mut events: Vec<EventEnvelope> = Vec::new();
        for KvPair { key, value } in all.into_iter() {
            if let Some(seq_str) = key.strip_prefix(&prefix) {
                if let Ok(seq) = seq_str.parse::<u64>() {
                    if seq > after_seq {
                        let se: StoredEnvelope = serialization::deserialize(&value)?;
                        events.push(Self::stored_to_envelope(se));
                    }
                }
            }
        }
        events.sort_by_key(|e| e.seq);
        if events.len() > limit {
            events.truncate(limit);
        }
        Ok(events)
    }

    pub async fn publish_task_event(
        &self,
        task: &Task,
        kind: TaskEventKind,
    ) -> Result<u64, SmsError> {
        let base_stream = Self::type_stream(ResourceType::Task);
        let resource_id = task.task_id.clone();

        let ts_ms = chrono::Utc::now().timestamp_millis();
        let payload = TaskEvent {
            event_id: 0,
            ts: chrono::Utc::now().timestamp(),
            task_id: task.task_id.clone(),
            kind: kind as i32,
            execution_id: None,
        };
        let payload_bytes = payload.encode_to_vec();
        let any = prost_types::Any {
            type_url: "type.googleapis.com/sms.TaskEvent".to_string(),
            value: payload_bytes,
        };

        let op = match kind {
            TaskEventKind::Create => EventOp::Create,
            TaskEventKind::Update => EventOp::Update,
            TaskEventKind::Cancel => EventOp::Cancel,
            TaskEventKind::Unknown => EventOp::Unknown,
        };

        let env = EventEnvelope {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms,
            stream: base_stream.clone(),
            seq: 0,
            resource_type: ResourceType::Task as i32,
            resource_id: resource_id.clone(),
            op: op as i32,
            schema_version: 1,
            node_uuid: String::new(),
            correlation_id: String::new(),
            headers: HashMap::new(),
            payload: Some(any),
            payload_bytes: Vec::new(),
            content_type: "application/protobuf".to_string(),
        };

        self.append_to_streams(
            env,
            &[
                base_stream,
                Self::all_stream().to_string(),
                Self::resource_stream(ResourceType::Task, &resource_id),
            ],
        )
        .await
    }

    pub async fn publish_task_assignment_event(
        &self,
        assignment: &TaskPlacementAssignment,
        op: EventOp,
    ) -> Result<u64, SmsError> {
        let node_uuid = assignment.node_uuid.clone();
        let base_stream = Self::node_stream(&node_uuid);
        let resource_id = format!("{}:{}", assignment.task_id, assignment.node_uuid);
        let ts_ms = chrono::Utc::now().timestamp_millis();
        let payload_bytes = assignment.encode_to_vec();
        let any = prost_types::Any {
            type_url: "type.googleapis.com/sms.TaskPlacementAssignment".to_string(),
            value: payload_bytes,
        };
        let env = EventEnvelope {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms,
            stream: base_stream.clone(),
            seq: 0,
            resource_type: ResourceType::TaskAssignment as i32,
            resource_id: resource_id.clone(),
            op: op as i32,
            schema_version: 1,
            node_uuid,
            correlation_id: String::new(),
            headers: HashMap::new(),
            payload: Some(any),
            payload_bytes: Vec::new(),
            content_type: "application/protobuf".to_string(),
        };

        let mut streams = vec![
            base_stream,
            Self::all_stream().to_string(),
            Self::type_stream(ResourceType::TaskAssignment),
            Self::resource_stream(ResourceType::TaskAssignment, &resource_id),
        ];
        if !assignment.task_id.is_empty() {
            streams.push(Self::resource_stream(
                ResourceType::Task,
                &assignment.task_id,
            ));
        }
        self.append_to_streams(env, &streams).await
    }

    pub async fn publish_node_event(&self, node: &Node, op: EventOp) -> Result<u64, SmsError> {
        let node_uuid = node.uuid.clone();
        let base_stream = Self::node_stream(&node_uuid);
        let resource_id = node_uuid.clone();

        let ts_ms = chrono::Utc::now().timestamp_millis();
        let payload = node.encode_to_vec();
        let any = prost_types::Any {
            type_url: "type.googleapis.com/sms.Node".to_string(),
            value: payload,
        };

        let env = EventEnvelope {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms,
            stream: base_stream.clone(),
            seq: 0,
            resource_type: ResourceType::Node as i32,
            resource_id: node_uuid.clone(),
            op: op as i32,
            schema_version: 1,
            node_uuid,
            correlation_id: String::new(),
            headers: HashMap::new(),
            payload: Some(any),
            payload_bytes: Vec::new(),
            content_type: "application/protobuf".to_string(),
        };

        self.append_to_streams(
            env,
            &[
                base_stream,
                Self::all_stream().to_string(),
                Self::type_stream(ResourceType::Node),
                Self::resource_stream(ResourceType::Node, &resource_id),
            ],
        )
        .await
    }

    pub async fn publish_node_deleted(&self, node_uuid: &str) -> Result<u64, SmsError> {
        let base_stream = Self::node_stream(node_uuid);
        let resource_id = node_uuid.to_string();

        let env = EventEnvelope {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms: chrono::Utc::now().timestamp_millis(),
            stream: base_stream.clone(),
            seq: 0,
            resource_type: ResourceType::Node as i32,
            resource_id: resource_id.clone(),
            op: EventOp::Delete as i32,
            schema_version: 1,
            node_uuid: node_uuid.to_string(),
            correlation_id: String::new(),
            headers: HashMap::new(),
            payload: None,
            payload_bytes: Vec::new(),
            content_type: String::new(),
        };

        self.append_to_streams(
            env,
            &[
                base_stream,
                Self::all_stream().to_string(),
                Self::type_stream(ResourceType::Node),
                Self::resource_stream(ResourceType::Node, &resource_id),
            ],
        )
        .await
    }

    pub async fn append_to_streams(
        &self,
        env: EventEnvelope,
        streams: &[String],
    ) -> Result<u64, SmsError> {
        let mut first_seq: Option<u64> = None;
        for stream in streams {
            let mut e = env.clone();
            e.stream = stream.clone();
            e.seq = self.next_seq(stream).await;
            self.append(e.clone()).await?;
            if first_seq.is_none() {
                first_seq = Some(e.seq);
            }
        }
        Ok(first_seq.unwrap_or(0))
    }

    pub async fn append(&self, env: EventEnvelope) -> Result<(), SmsError> {
        let stream = env.stream.clone();
        let seq = env.seq;
        let key = format!("{}{}:{}", OUTBOX_PREFIX, stream, seq);

        let (payload_type_url, payload_value) = env
            .payload
            .as_ref()
            .map(|a| (a.type_url.clone(), a.value.clone()))
            .unwrap_or_default();

        let se = StoredEnvelope {
            event_id: env.event_id.clone(),
            ts_ms: env.ts_ms,
            stream: env.stream.clone(),
            seq: env.seq,
            resource_type: env.resource_type,
            resource_id: env.resource_id.clone(),
            op: env.op,
            schema_version: env.schema_version,
            node_uuid: env.node_uuid.clone(),
            correlation_id: env.correlation_id.clone(),
            headers: env.headers.clone(),
            payload_type_url,
            payload_value,
            payload_bytes: env.payload_bytes.clone(),
            content_type: env.content_type.clone(),
        };
        let val = serialization::serialize(&se)?;
        self.kv.put(&key, &val).await?;

        let counter_key = format!("{}{}", COUNTER_PREFIX, stream);
        let counter_val = seq.to_string().into_bytes();
        self.kv.put(&counter_key, &counter_val).await?;

        if seq > MAX_EVENTS_PER_STREAM {
            let old_seq = seq - MAX_EVENTS_PER_STREAM;
            let old_key = format!("{}{}:{}", OUTBOX_PREFIX, env.stream, old_seq);
            let _ = self.kv.delete(&old_key).await;
        }

        let tx = self.get_sender(&env.stream).await;
        if tx.receiver_count() > 0 {
            let _ = tx.send(env);
        }
        Ok(())
    }

    fn stored_to_envelope(se: StoredEnvelope) -> EventEnvelope {
        let payload = if !se.payload_type_url.is_empty() || !se.payload_value.is_empty() {
            Some(prost_types::Any {
                type_url: se.payload_type_url,
                value: se.payload_value,
            })
        } else {
            None
        };

        EventEnvelope {
            event_id: se.event_id,
            ts_ms: se.ts_ms,
            stream: se.stream,
            seq: se.seq,
            resource_type: se.resource_type,
            resource_id: se.resource_id,
            op: se.op,
            schema_version: se.schema_version,
            node_uuid: se.node_uuid,
            correlation_id: se.correlation_id,
            headers: se.headers,
            payload,
            payload_bytes: se.payload_bytes,
            content_type: se.content_type,
        }
    }

    pub async fn publish_instance_event(
        &self,
        instance: &Instance,
        op: EventOp,
    ) -> Result<u64, SmsError> {
        let node_uuid = instance.node_uuid.clone();
        let base_stream = Self::node_stream(&node_uuid);
        let resource_id = instance.instance_id.clone();
        let ts_ms = chrono::Utc::now().timestamp_millis();
        let payload = instance.encode_to_vec();
        let any = prost_types::Any {
            type_url: "type.googleapis.com/sms.Instance".to_string(),
            value: payload,
        };
        let env = EventEnvelope {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms,
            stream: base_stream.clone(),
            seq: 0,
            resource_type: ResourceType::Instance as i32,
            resource_id: resource_id.clone(),
            op: op as i32,
            schema_version: 1,
            node_uuid: node_uuid.clone(),
            correlation_id: String::new(),
            headers: HashMap::new(),
            payload: Some(any),
            payload_bytes: Vec::new(),
            content_type: "application/protobuf".to_string(),
        };

        let mut streams = vec![
            base_stream,
            Self::all_stream().to_string(),
            Self::type_stream(ResourceType::Instance),
            Self::resource_stream(ResourceType::Instance, &resource_id),
        ];
        if !instance.task_id.is_empty() {
            streams.push(Self::resource_stream(ResourceType::Task, &instance.task_id));
        }
        self.append_to_streams(env, &streams).await
    }

    pub async fn publish_execution_event(
        &self,
        execution: &Execution,
        op: EventOp,
    ) -> Result<u64, SmsError> {
        let node_uuid = execution.node_uuid.clone();
        let base_stream = Self::node_stream(&node_uuid);
        let resource_id = execution.execution_id.clone();
        let ts_ms = chrono::Utc::now().timestamp_millis();
        let payload = execution.encode_to_vec();
        let any = prost_types::Any {
            type_url: "type.googleapis.com/sms.Execution".to_string(),
            value: payload,
        };
        let env = EventEnvelope {
            event_id: uuid::Uuid::new_v4().to_string(),
            ts_ms,
            stream: base_stream.clone(),
            seq: 0,
            resource_type: ResourceType::Execution as i32,
            resource_id: resource_id.clone(),
            op: op as i32,
            schema_version: 1,
            node_uuid: node_uuid.clone(),
            correlation_id: String::new(),
            headers: HashMap::new(),
            payload: Some(any),
            payload_bytes: Vec::new(),
            content_type: "application/protobuf".to_string(),
        };

        let mut streams = vec![
            base_stream,
            Self::all_stream().to_string(),
            Self::type_stream(ResourceType::Execution),
            Self::resource_stream(ResourceType::Execution, &resource_id),
        ];
        if !execution.task_id.is_empty() {
            streams.push(Self::resource_stream(
                ResourceType::Task,
                &execution.task_id,
            ));
        }
        if !execution.instance_id.is_empty() {
            streams.push(Self::resource_stream(
                ResourceType::Instance,
                &execution.instance_id,
            ));
        }
        self.append_to_streams(env, &streams).await
    }
}
