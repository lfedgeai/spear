use crate::proto::sms::{
    Execution, ExecutionSummary, Instance, InstanceStatus, InstanceSummary, LogRef,
};
use crate::sms::services::error::SmsError;
use crate::storage::kv::{serialization, KvStore, RangeOptions};
use prost::Message;
use std::collections::HashMap;
use std::sync::Arc;

const INSTANCE_KEY_PREFIX: &str = "instance:";
const INSTANCE_TOMBSTONE_KEY_PREFIX: &str = "instance_tombstone:";
const EXECUTION_KEY_PREFIX: &str = "execution:";
const IDX_EXECUTIONS_BY_STARTED_PREFIX: &str = "idx:executions:started_at:";
const IDX_TASK_EXECUTIONS_BY_STARTED_PREFIX: &str = "idx:task_executions:";
const IDX_TASK_ACTIVE_INSTANCES_PREFIX: &str = "idx:task_active_instances:";
const IDX_INSTANCE_RECENT_EXECUTIONS_PREFIX: &str = "idx:instance_recent_executions:";
const PROJECTION_CHECKPOINT_PREFIX: &str = "projection_checkpoint:";

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct StoredLogRef {
    backend: String,
    uri_prefix: String,
    content_type: String,
    compression: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct StoredInstanceRecord {
    instance_id: String,
    task_id: String,
    node_uuid: String,
    status: i32,
    created_at_ms: i64,
    updated_at_ms: i64,
    last_seen_ms: i64,
    current_execution_id: String,
    metadata: HashMap<String, String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct StoredExecutionRecord {
    execution_id: String,
    invocation_id: String,
    task_id: String,
    function_name: String,
    node_uuid: String,
    instance_id: String,
    status: i32,
    started_at_ms: i64,
    completed_at_ms: i64,
    log_ref: Option<StoredLogRef>,
    metadata: HashMap<String, String>,
    updated_at_ms: i64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct StoredInstanceSummary {
    instance_id: String,
    node_uuid: String,
    status: i32,
    last_seen_ms: i64,
    current_execution_id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct StoredExecutionSummary {
    execution_id: String,
    task_id: String,
    status: i32,
    started_at_ms: i64,
    completed_at_ms: i64,
    function_name: String,
}

#[derive(Debug, Clone)]
pub struct InstanceExecutionIndex {
    kv: Arc<dyn KvStore>,
    max_active_instances_per_task: usize,
    max_recent_executions_per_instance: usize,
    stale_after_ms: i64,
}

impl InstanceExecutionIndex {
    pub fn new(
        kv: Arc<dyn KvStore>,
        max_active_instances_per_task: usize,
        max_recent_executions_per_instance: usize,
        stale_after_ms: i64,
    ) -> Self {
        Self {
            kv,
            max_active_instances_per_task: max_active_instances_per_task.max(1),
            max_recent_executions_per_instance: max_recent_executions_per_instance.max(1),
            stale_after_ms: stale_after_ms.max(1),
        }
    }

    pub async fn upsert_instance_record(&self, inst: Instance) -> Result<(bool, i64), SmsError> {
        if inst.instance_id.is_empty() || inst.task_id.is_empty() || inst.node_uuid.is_empty() {
            return Err(SmsError::InvalidRequest(
                "instance_id, task_id, node_uuid are required".to_string(),
            ));
        }
        let rec = stored_instance_record_from_proto(&inst);
        if let Some(tombstone_ms) = self.instance_tombstone_ms(&rec.instance_id).await? {
            if tombstone_ms > rec.updated_at_ms {
                return Ok((false, tombstone_ms));
            }
        }
        let key = format!("{}{}", INSTANCE_KEY_PREFIX, rec.instance_id);
        let stored = self.kv.get(&key).await?;
        if let Some(bytes) = stored {
            let existing: StoredInstanceRecord = serialization::deserialize(&bytes)?;
            if existing.updated_at_ms > rec.updated_at_ms {
                return Ok((false, existing.updated_at_ms));
            }
        }
        let val = serialization::serialize(&rec)?;
        self.kv.put(&key, &val).await?;
        self.clear_instance_tombstone(&rec.instance_id).await?;
        Ok((true, rec.updated_at_ms))
    }

    pub async fn tombstone_instance_record(
        &self,
        instance_id: &str,
        task_id: &str,
        deleted_at_ms: i64,
    ) -> Result<(bool, i64), SmsError> {
        if instance_id.is_empty() || task_id.is_empty() {
            return Err(SmsError::InvalidRequest(
                "instance_id and task_id are required".to_string(),
            ));
        }
        let latest_known_ms = self
            .latest_instance_clock_ms(instance_id)
            .await?
            .unwrap_or_default();
        if latest_known_ms > deleted_at_ms {
            return Ok((false, latest_known_ms));
        }

        let key = format!("{}{}", INSTANCE_KEY_PREFIX, instance_id);
        let _ = self.kv.delete(&key).await;
        self.store_instance_tombstone(instance_id, deleted_at_ms).await?;
        self.remove_from_task_active_instances(task_id, instance_id)
            .await?;
        Ok((true, deleted_at_ms))
    }

    pub fn stale_after_ms(&self) -> i64 {
        self.stale_after_ms
    }

    pub async fn upsert_execution_record(&self, exe: Execution) -> Result<(bool, i64), SmsError> {
        if exe.execution_id.is_empty()
            || exe.task_id.is_empty()
            || exe.node_uuid.is_empty()
            || exe.instance_id.is_empty()
        {
            return Err(SmsError::InvalidRequest(
                "execution_id, task_id, node_uuid, instance_id are required".to_string(),
            ));
        }
        let rec = stored_execution_record_from_proto(&exe);
        let key = format!("{}{}", EXECUTION_KEY_PREFIX, rec.execution_id);
        let stored = self.kv.get(&key).await?;
        if let Some(bytes) = stored {
            let existing: StoredExecutionRecord = serialization::deserialize(&bytes)?;
            if existing.updated_at_ms > rec.updated_at_ms {
                return Ok((false, existing.updated_at_ms));
            }
            self.remove_execution_history_indexes(&existing).await?;
        }
        let val = serialization::serialize(&rec)?;
        self.kv.put(&key, &val).await?;
        self.store_execution_history_indexes(&rec).await?;
        Ok((true, rec.updated_at_ms))
    }

    pub async fn project_instance_views(&self, inst: &Instance, now_ms: i64) -> Result<(), SmsError> {
        self.project_task_active_instances_view(inst, now_ms).await
    }

    pub async fn get_instance(&self, instance_id: &str) -> Result<Option<Instance>, SmsError> {
        if instance_id.is_empty() {
            return Ok(None);
        }
        let key = format!("{}{}", INSTANCE_KEY_PREFIX, instance_id);
        let stored = self.kv.get(&key).await?;
        let Some(bytes) = stored else {
            return Ok(None);
        };
        let rec: StoredInstanceRecord = serialization::deserialize(&bytes)?;
        Ok(Some(proto_instance_from_stored(rec)))
    }

    pub async fn get_execution(&self, execution_id: &str) -> Result<Option<Execution>, SmsError> {
        if execution_id.is_empty() {
            return Ok(None);
        }
        let key = format!("{}{}", EXECUTION_KEY_PREFIX, execution_id);
        let stored = self.kv.get(&key).await?;
        let Some(bytes) = stored else {
            return Ok(None);
        };
        let rec: StoredExecutionRecord = serialization::deserialize(&bytes)?;
        Ok(Some(proto_execution_from_stored(rec)))
    }

    pub async fn list_task_instances(
        &self,
        task_id: &str,
        now_ms: i64,
        limit: usize,
        page_token: &str,
    ) -> Result<(Vec<InstanceSummary>, String), SmsError> {
        let key = format!("{}{}", IDX_TASK_ACTIVE_INSTANCES_PREFIX, task_id);
        let list = self.load_vec::<StoredInstanceSummary>(&key).await?;
        let mut filtered: Vec<StoredInstanceSummary> = list
            .into_iter()
            .filter(|s| {
                is_instance_active_and_fresh(s.status, s.last_seen_ms, now_ms, self.stale_after_ms)
            })
            .collect();
        filtered.sort_by(|a, b| b.last_seen_ms.cmp(&a.last_seen_ms));

        let offset = parse_offset(page_token);
        let limit = limit.max(1).min(self.max_active_instances_per_task);
        let end = (offset + limit).min(filtered.len());
        let page: Vec<InstanceSummary> = if offset >= filtered.len() {
            Vec::new()
        } else {
            filtered[offset..end]
                .iter()
                .cloned()
                .map(|s| InstanceSummary {
                    instance_id: s.instance_id,
                    node_uuid: s.node_uuid,
                    status: s.status,
                    last_seen_ms: s.last_seen_ms,
                    current_execution_id: s.current_execution_id,
                })
                .collect()
        };
        let next = if end < filtered.len() {
            end.to_string()
        } else {
            String::new()
        };
        Ok((page, next))
    }

    pub async fn list_instance_executions(
        &self,
        instance_id: &str,
        limit: usize,
        page_token: &str,
    ) -> Result<(Vec<ExecutionSummary>, String), SmsError> {
        let key = format!("{}{}", IDX_INSTANCE_RECENT_EXECUTIONS_PREFIX, instance_id);
        let mut list = self.load_vec::<StoredExecutionSummary>(&key).await?;
        list.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));

        let offset = parse_offset(page_token);
        let limit = limit.max(1).min(self.max_recent_executions_per_instance);
        let end = (offset + limit).min(list.len());
        let page: Vec<ExecutionSummary> = if offset >= list.len() {
            Vec::new()
        } else {
            list[offset..end]
                .iter()
                .cloned()
                .map(|s| ExecutionSummary {
                    execution_id: s.execution_id,
                    task_id: s.task_id,
                    status: s.status,
                    started_at_ms: s.started_at_ms,
                    completed_at_ms: s.completed_at_ms,
                    function_name: s.function_name,
                })
                .collect()
        };
        let next = if end < list.len() {
            end.to_string()
        } else {
            String::new()
        };
        Ok((page, next))
    }

    pub async fn list_executions(
        &self,
        task_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
        page_token: &str,
    ) -> Result<(Vec<Execution>, String), SmsError> {
        let task_filter = task_id
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let status_filter = status
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());
        let prefix = execution_history_index_prefix(task_filter.as_deref());
        let start_key = if page_token.is_empty() {
            prefix.clone()
        } else {
            next_index_cursor(page_token)
        };
        let end_key = index_prefix_end(&prefix);
        let mut executions = Vec::new();
        let mut cursor = start_key;
        let mut next = String::new();
        let batch_limit = limit.max(1).max(64);
        while executions.len() < limit.max(1) + 1 {
            let pairs = self
                .kv
                .range(
                    &RangeOptions::new()
                        .start_key(cursor.clone())
                        .end_key(end_key.clone())
                        .limit(batch_limit),
                )
                .await?;
            if pairs.is_empty() {
                break;
            }

            let mut last_key = String::new();
            let mut last_returned_key = String::new();
            for pair in pairs.iter() {
                last_key = pair.key.clone();
                let execution_id = String::from_utf8(pair.value.clone()).map_err(|e| {
                    SmsError::Serialization(format!("invalid execution archive index value: {}", e))
                })?;
                let Some(exe) = self.get_execution(&execution_id).await? else {
                    continue;
                };
                if let Some(status_filter) = status_filter.as_ref() {
                    let public_status =
                        crate::sms::execution_status_to_public_str(exe.status).to_ascii_lowercase();
                    if public_status != *status_filter {
                        continue;
                    }
                }
                executions.push(exe);
                if executions.len() <= limit.max(1) {
                    last_returned_key = last_key.clone();
                }
                if executions.len() > limit.max(1) {
                    next = last_returned_key;
                    break;
                }
            }
            if !next.is_empty() {
                break;
            }
            if last_key.is_empty() || pairs.len() < batch_limit {
                break;
            }
            cursor = next_index_cursor(&last_key);
        }
        if executions.len() > limit.max(1) {
            executions.truncate(limit.max(1));
        }
        Ok((executions, next))
    }

    pub async fn project_instance_event(
        &self,
        op: i32,
        payload: &prost_types::Any,
        now_ms: i64,
    ) -> Result<(), SmsError> {
        let inst = decode_any::<Instance>(payload)?;
        if op == crate::proto::sms::EventOp::Delete as i32 {
            self.tombstone_instance_record(
                &inst.instance_id,
                &inst.task_id,
                inst.updated_at_ms.max(now_ms),
            )
                .await?;
            return Ok(());
        }
        let (accepted, _) = self.upsert_instance_record(inst.clone()).await?;
        if accepted {
            self.project_task_active_instances_view(&inst, now_ms).await?;
        }
        Ok(())
    }

    pub async fn project_execution_event(
        &self,
        op: i32,
        payload: &prost_types::Any,
        now_ms: i64,
    ) -> Result<(), SmsError> {
        let exe = decode_any::<Execution>(payload)?;
        if op == crate::proto::sms::EventOp::Delete as i32 {
            self.purge_execution_record(&exe.execution_id).await?;
            self.remove_from_instance_recent_executions(&exe.instance_id, &exe.execution_id)
                .await?;
            return Ok(());
        }
        let _ = self.upsert_execution_record(exe.clone()).await?;
        self.project_instance_recent_executions_view(&exe).await?;
        if !exe.instance_id.is_empty() && !exe.task_id.is_empty() {
            let inst = Instance {
                instance_id: exe.instance_id.clone(),
                task_id: exe.task_id.clone(),
                node_uuid: exe.node_uuid.clone(),
                status: InstanceStatus::Running as i32,
                created_at_ms: 0,
                updated_at_ms: now_ms,
                last_seen_ms: now_ms,
                current_execution_id: exe.execution_id.clone(),
                metadata: std::collections::HashMap::new(),
            };
            let (accepted, _) = self.upsert_instance_record(inst.clone()).await?;
            if accepted {
                self.project_task_active_instances_view(&inst, now_ms).await?;
            }
        }
        Ok(())
    }

    pub async fn load_checkpoint(&self, name: &str) -> Result<u64, SmsError> {
        let key = format!("{}{}", PROJECTION_CHECKPOINT_PREFIX, name);
        let v = self
            .kv
            .get(&key)
            .await?
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        Ok(v)
    }

    pub async fn store_checkpoint(&self, name: &str, seq: u64) -> Result<(), SmsError> {
        let key = format!("{}{}", PROJECTION_CHECKPOINT_PREFIX, name);
        let bytes = seq.to_string().into_bytes();
        self.kv.put(&key, &bytes).await?;
        Ok(())
    }

    async fn latest_instance_clock_ms(&self, instance_id: &str) -> Result<Option<i64>, SmsError> {
        let record_ms = match self
            .kv
            .get(&format!("{}{}", INSTANCE_KEY_PREFIX, instance_id))
            .await?
        {
            Some(bytes) => {
                let existing: StoredInstanceRecord = serialization::deserialize(&bytes)?;
                Some(existing.updated_at_ms)
            }
            None => None,
        };
        let tombstone_ms = self.instance_tombstone_ms(instance_id).await?;
        Ok(match (record_ms, tombstone_ms) {
            (Some(record_ms), Some(tombstone_ms)) => Some(record_ms.max(tombstone_ms)),
            (Some(record_ms), None) => Some(record_ms),
            (None, Some(tombstone_ms)) => Some(tombstone_ms),
            (None, None) => None,
        })
    }

    async fn instance_tombstone_ms(&self, instance_id: &str) -> Result<Option<i64>, SmsError> {
        let key = format!("{}{}", INSTANCE_TOMBSTONE_KEY_PREFIX, instance_id);
        let bytes = self.kv.get(&key).await?;
        Ok(bytes
            .and_then(|raw| String::from_utf8(raw).ok())
            .and_then(|value| value.parse::<i64>().ok()))
    }

    async fn store_instance_tombstone(
        &self,
        instance_id: &str,
        deleted_at_ms: i64,
    ) -> Result<(), SmsError> {
        let key = format!("{}{}", INSTANCE_TOMBSTONE_KEY_PREFIX, instance_id);
        let bytes = deleted_at_ms.to_string().into_bytes();
        self.kv.put(&key, &bytes).await?;
        Ok(())
    }

    async fn clear_instance_tombstone(&self, instance_id: &str) -> Result<(), SmsError> {
        let key = format!("{}{}", INSTANCE_TOMBSTONE_KEY_PREFIX, instance_id);
        let _ = self.kv.delete(&key).await;
        Ok(())
    }

    async fn purge_execution_record(&self, execution_id: &str) -> Result<(), SmsError> {
        if execution_id.is_empty() {
            return Ok(());
        }
        let key = format!("{}{}", EXECUTION_KEY_PREFIX, execution_id);
        if let Some(bytes) = self.kv.get(&key).await? {
            let rec: StoredExecutionRecord = serialization::deserialize(&bytes)?;
            self.remove_execution_history_indexes(&rec).await?;
        }
        let _ = self.kv.delete(&key).await;
        Ok(())
    }

    async fn project_task_active_instances_view(
        &self,
        inst: &Instance,
        now_ms: i64,
    ) -> Result<(), SmsError> {
        let key = format!("{}{}", IDX_TASK_ACTIVE_INSTANCES_PREFIX, inst.task_id);
        let mut list = self.load_vec::<StoredInstanceSummary>(&key).await?;
        list.retain(|s| {
            s.instance_id != inst.instance_id
                && is_instance_active_and_fresh(
                    s.status,
                    s.last_seen_ms,
                    now_ms,
                    self.stale_after_ms,
                )
        });

        if is_instance_active_and_fresh(inst.status, inst.last_seen_ms, now_ms, self.stale_after_ms)
        {
            list.push(StoredInstanceSummary {
                instance_id: inst.instance_id.clone(),
                node_uuid: inst.node_uuid.clone(),
                status: inst.status,
                last_seen_ms: inst.last_seen_ms,
                current_execution_id: inst.current_execution_id.clone(),
            });
        }
        list.sort_by(|a, b| b.last_seen_ms.cmp(&a.last_seen_ms));
        if list.len() > self.max_active_instances_per_task {
            list.truncate(self.max_active_instances_per_task);
        }
        self.store_vec(&key, &list).await?;
        Ok(())
    }

    async fn remove_from_task_active_instances(
        &self,
        task_id: &str,
        instance_id: &str,
    ) -> Result<(), SmsError> {
        let key = format!("{}{}", IDX_TASK_ACTIVE_INSTANCES_PREFIX, task_id);
        let mut list = self.load_vec::<StoredInstanceSummary>(&key).await?;
        let before = list.len();
        list.retain(|s| s.instance_id != instance_id);
        if list.len() != before {
            self.store_vec(&key, &list).await?;
        }
        Ok(())
    }

    async fn project_instance_recent_executions_view(
        &self,
        exe: &Execution,
    ) -> Result<(), SmsError> {
        let key = format!(
            "{}{}",
            IDX_INSTANCE_RECENT_EXECUTIONS_PREFIX, exe.instance_id
        );
        let mut list = self.load_vec::<StoredExecutionSummary>(&key).await?;
        list.retain(|s| s.execution_id != exe.execution_id);
        list.push(StoredExecutionSummary {
            execution_id: exe.execution_id.clone(),
            task_id: exe.task_id.clone(),
            status: exe.status,
            started_at_ms: exe.started_at_ms,
            completed_at_ms: exe.completed_at_ms,
            function_name: exe.function_name.clone(),
        });
        list.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms));
        if list.len() > self.max_recent_executions_per_instance {
            list.truncate(self.max_recent_executions_per_instance);
        }
        self.store_vec(&key, &list).await?;
        Ok(())
    }

    async fn remove_from_instance_recent_executions(
        &self,
        instance_id: &str,
        execution_id: &str,
    ) -> Result<(), SmsError> {
        if instance_id.is_empty() || execution_id.is_empty() {
            return Ok(());
        }
        let key = format!("{}{}", IDX_INSTANCE_RECENT_EXECUTIONS_PREFIX, instance_id);
        let mut list = self.load_vec::<StoredExecutionSummary>(&key).await?;
        let before = list.len();
        list.retain(|s| s.execution_id != execution_id);
        if list.len() != before {
            self.store_vec(&key, &list).await?;
        }
        Ok(())
    }

    async fn store_execution_history_indexes(
        &self,
        rec: &StoredExecutionRecord,
    ) -> Result<(), SmsError> {
        let execution_id_bytes = rec.execution_id.as_bytes().to_vec();
        self.kv
            .put(&execution_history_index_key(None, rec), &execution_id_bytes)
            .await?;
        self.kv
            .put(
                &execution_history_index_key(Some(rec.task_id.as_str()), rec),
                &execution_id_bytes,
            )
            .await?;
        Ok(())
    }

    async fn remove_execution_history_indexes(
        &self,
        rec: &StoredExecutionRecord,
    ) -> Result<(), SmsError> {
        let _ = self
            .kv
            .delete(&execution_history_index_key(None, rec))
            .await?;
        let _ = self
            .kv
            .delete(&execution_history_index_key(Some(rec.task_id.as_str()), rec))
            .await?;
        Ok(())
    }

    async fn load_vec<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Vec<T>, SmsError> {
        let key_owned = key.to_string();
        let Some(bytes) = self.kv.get(&key_owned).await? else {
            return Ok(Vec::new());
        };
        let v: Vec<T> = serialization::deserialize(&bytes)?;
        Ok(v)
    }

    async fn store_vec<T: serde::Serialize>(
        &self,
        key: &str,
        value: &Vec<T>,
    ) -> Result<(), SmsError> {
        let bytes = serialization::serialize(value)?;
        let key_owned = key.to_string();
        self.kv.put(&key_owned, &bytes).await?;
        Ok(())
    }
}

fn reverse_started_at_key(ts_ms: i64) -> String {
    let normalized = ts_ms.max(0) as u64;
    format!("{:020}", u64::MAX - normalized)
}

fn execution_history_index_prefix(task_id: Option<&str>) -> String {
    match task_id {
        Some(task_id) if !task_id.is_empty() => {
            format!("{}{}:", IDX_TASK_EXECUTIONS_BY_STARTED_PREFIX, task_id)
        }
        _ => IDX_EXECUTIONS_BY_STARTED_PREFIX.to_string(),
    }
}

fn execution_history_index_key(task_id: Option<&str>, rec: &StoredExecutionRecord) -> String {
    format!(
        "{}{}:{}",
        execution_history_index_prefix(task_id),
        reverse_started_at_key(rec.started_at_ms),
        rec.execution_id
    )
}

fn index_prefix_end(prefix: &str) -> String {
    format!("{}{}", prefix, '\u{10FFFF}')
}

fn next_index_cursor(last_key: &str) -> String {
    format!("{}{}", last_key, '\0')
}

fn stored_log_ref_from_proto(lr: &LogRef) -> StoredLogRef {
    StoredLogRef {
        backend: lr.backend.clone(),
        uri_prefix: lr.uri_prefix.clone(),
        content_type: lr.content_type.clone(),
        compression: lr.compression.clone(),
    }
}

fn proto_log_ref_from_stored(lr: StoredLogRef) -> LogRef {
    LogRef {
        backend: lr.backend,
        uri_prefix: lr.uri_prefix,
        content_type: lr.content_type,
        compression: lr.compression,
    }
}

fn stored_instance_record_from_proto(inst: &Instance) -> StoredInstanceRecord {
    StoredInstanceRecord {
        instance_id: inst.instance_id.clone(),
        task_id: inst.task_id.clone(),
        node_uuid: inst.node_uuid.clone(),
        status: inst.status,
        created_at_ms: inst.created_at_ms,
        updated_at_ms: inst.updated_at_ms,
        last_seen_ms: inst.last_seen_ms,
        current_execution_id: inst.current_execution_id.clone(),
        metadata: inst.metadata.clone(),
    }
}

fn proto_instance_from_stored(rec: StoredInstanceRecord) -> Instance {
    Instance {
        instance_id: rec.instance_id,
        task_id: rec.task_id,
        node_uuid: rec.node_uuid,
        status: rec.status,
        created_at_ms: rec.created_at_ms,
        updated_at_ms: rec.updated_at_ms,
        last_seen_ms: rec.last_seen_ms,
        current_execution_id: rec.current_execution_id,
        metadata: rec.metadata,
    }
}

fn stored_execution_record_from_proto(exe: &Execution) -> StoredExecutionRecord {
    StoredExecutionRecord {
        execution_id: exe.execution_id.clone(),
        invocation_id: exe.invocation_id.clone(),
        task_id: exe.task_id.clone(),
        function_name: exe.function_name.clone(),
        node_uuid: exe.node_uuid.clone(),
        instance_id: exe.instance_id.clone(),
        status: exe.status,
        started_at_ms: exe.started_at_ms,
        completed_at_ms: exe.completed_at_ms,
        log_ref: exe.log_ref.as_ref().map(stored_log_ref_from_proto),
        metadata: exe.metadata.clone(),
        updated_at_ms: exe.updated_at_ms,
    }
}

fn proto_execution_from_stored(rec: StoredExecutionRecord) -> Execution {
    Execution {
        execution_id: rec.execution_id,
        invocation_id: rec.invocation_id,
        task_id: rec.task_id,
        function_name: rec.function_name,
        node_uuid: rec.node_uuid,
        instance_id: rec.instance_id,
        status: rec.status,
        started_at_ms: rec.started_at_ms,
        completed_at_ms: rec.completed_at_ms,
        log_ref: rec.log_ref.map(proto_log_ref_from_stored),
        metadata: rec.metadata,
        updated_at_ms: rec.updated_at_ms,
    }
}

pub(crate) fn is_instance_active_and_fresh(
    status: i32,
    last_seen_ms: i64,
    now_ms: i64,
    stale_after_ms: i64,
) -> bool {
    if last_seen_ms <= 0 {
        return false;
    }
    if now_ms.saturating_sub(last_seen_ms) > stale_after_ms {
        return false;
    }
    crate::sms::is_instance_live(status)
}

fn parse_offset(token: &str) -> usize {
    token.parse::<usize>().unwrap_or(0)
}

fn decode_any<T: Message + Default>(any: &prost_types::Any) -> Result<T, SmsError> {
    T::decode(any.value.as_slice()).map_err(|e| SmsError::Serialization(e.to_string()))
}

impl Default for InstanceExecutionIndex {
    fn default() -> Self {
        Self {
            kv: Arc::new(crate::storage::kv::MemoryKvStore::new()),
            max_active_instances_per_task: 256,
            max_recent_executions_per_instance: 1000,
            stale_after_ms: 120_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_instance_active_and_fresh, InstanceExecutionIndex};
    use crate::proto::sms::{EventOp, Execution, ExecutionStatus, Instance, InstanceStatus};
    use prost::Message;

    #[tokio::test]
    async fn get_instance_returns_latest_stored_instance() {
        let idx = InstanceExecutionIndex::default();
        let inst = Instance {
            instance_id: "inst-1".to_string(),
            task_id: "task-1".to_string(),
            node_uuid: "node-1".to_string(),
            status: InstanceStatus::Running as i32,
            created_at_ms: 10,
            updated_at_ms: 20,
            last_seen_ms: 20,
            current_execution_id: "exe-1".to_string(),
            metadata: std::collections::HashMap::new(),
        };

        idx.upsert_instance_record(inst.clone()).await.unwrap();

        let stored = idx.get_instance("inst-1").await.unwrap().unwrap();
        assert_eq!(stored.instance_id, inst.instance_id);
        assert_eq!(stored.task_id, inst.task_id);
        assert_eq!(stored.node_uuid, inst.node_uuid);
        assert_eq!(stored.status, inst.status);
        assert_eq!(stored.current_execution_id, inst.current_execution_id);
    }

    #[test]
    fn instance_activity_requires_non_terminated_and_fresh_status() {
        assert!(is_instance_active_and_fresh(
            InstanceStatus::Running as i32,
            1_000,
            1_100,
            500,
        ));
        assert!(!is_instance_active_and_fresh(
            InstanceStatus::Terminated as i32,
            1_000,
            1_100,
            500,
        ));
        assert!(!is_instance_active_and_fresh(
            InstanceStatus::Running as i32,
            1_000,
            2_000,
            500,
        ));
    }

    #[tokio::test]
    async fn delete_instance_event_removes_active_instance_and_blocks_stale_upsert() {
        let idx = InstanceExecutionIndex::default();
        let running = Instance {
            instance_id: "inst-delete-1".to_string(),
            task_id: "task-delete-1".to_string(),
            node_uuid: "node-1".to_string(),
            status: InstanceStatus::Running as i32,
            created_at_ms: 100,
            updated_at_ms: 100,
            last_seen_ms: 100,
            current_execution_id: String::new(),
            metadata: std::collections::HashMap::new(),
        };
        let running_any = prost_types::Any {
            type_url: "type.googleapis.com/sms.Instance".to_string(),
            value: running.encode_to_vec(),
        };
        idx.project_instance_event(EventOp::Upsert as i32, &running_any, 100)
            .await
            .unwrap();

        let (instances, _) = idx
            .list_task_instances("task-delete-1", 100, 10, "")
            .await
            .unwrap();
        assert_eq!(instances.len(), 1);

        let delete_payload = Instance {
            updated_at_ms: 200,
            last_seen_ms: 200,
            ..running.clone()
        };
        let delete_any = prost_types::Any {
            type_url: "type.googleapis.com/sms.Instance".to_string(),
            value: delete_payload.encode_to_vec(),
        };
        idx.project_instance_event(EventOp::Delete as i32, &delete_any, 200)
            .await
            .unwrap();

        assert!(idx.get_instance("inst-delete-1").await.unwrap().is_none());
        let (instances, _) = idx
            .list_task_instances("task-delete-1", 200, 10, "")
            .await
            .unwrap();
        assert!(instances.is_empty());

        let stale_running = Instance {
            updated_at_ms: 150,
            last_seen_ms: 150,
            ..running
        };
        let stale_running_any = prost_types::Any {
            type_url: "type.googleapis.com/sms.Instance".to_string(),
            value: stale_running.encode_to_vec(),
        };
        idx.project_instance_event(EventOp::Upsert as i32, &stale_running_any, 150)
            .await
            .unwrap();

        assert!(idx.get_instance("inst-delete-1").await.unwrap().is_none());
        let (instances, _) = idx
            .list_task_instances("task-delete-1", 200, 10, "")
            .await
            .unwrap();
        assert!(instances.is_empty());
    }

    #[tokio::test]
    async fn list_executions_uses_started_at_index_for_pagination() {
        let idx = InstanceExecutionIndex::default();
        for (execution_id, started_at_ms) in [("exe-1", 100), ("exe-2", 300), ("exe-3", 200)] {
            idx.upsert_execution_record(Execution {
                execution_id: execution_id.to_string(),
                invocation_id: format!("inv-{}", execution_id),
                task_id: "task-1".to_string(),
                function_name: "main".to_string(),
                node_uuid: "node-1".to_string(),
                instance_id: "inst-1".to_string(),
                status: ExecutionStatus::Completed as i32,
                started_at_ms,
                completed_at_ms: started_at_ms + 10,
                log_ref: None,
                metadata: std::collections::HashMap::new(),
                updated_at_ms: started_at_ms + 20,
            })
            .await
            .unwrap();
        }

        let (page1, token1) = idx.list_executions(None, None, 2, "").await.unwrap();
        assert_eq!(
            page1.iter().map(|e| e.execution_id.as_str()).collect::<Vec<_>>(),
            vec!["exe-2", "exe-3"]
        );
        assert!(!token1.is_empty());

        let (page2, token2) = idx.list_executions(None, None, 2, &token1).await.unwrap();
        assert_eq!(
            page2.iter().map(|e| e.execution_id.as_str()).collect::<Vec<_>>(),
            vec!["exe-1"]
        );
        assert!(token2.is_empty());
    }

    #[tokio::test]
    async fn list_executions_filters_by_task_and_status() {
        let idx = InstanceExecutionIndex::default();
        for (execution_id, task_id, status, started_at_ms) in [
            ("exe-a", "task-a", ExecutionStatus::Completed as i32, 100),
            ("exe-b", "task-b", ExecutionStatus::Completed as i32, 200),
            ("exe-c", "task-a", ExecutionStatus::Failed as i32, 300),
        ] {
            idx.upsert_execution_record(Execution {
                execution_id: execution_id.to_string(),
                invocation_id: format!("inv-{}", execution_id),
                task_id: task_id.to_string(),
                function_name: "main".to_string(),
                node_uuid: "node-1".to_string(),
                instance_id: "inst-1".to_string(),
                status,
                started_at_ms,
                completed_at_ms: started_at_ms + 10,
                log_ref: None,
                metadata: std::collections::HashMap::new(),
                updated_at_ms: started_at_ms + 20,
            })
            .await
            .unwrap();
        }

        let (page, token) = idx
            .list_executions(Some("task-a"), Some("completed"), 10, "")
            .await
            .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].execution_id, "exe-a");
        assert!(token.is_empty());
    }
}

pub fn make_default_log_ref(execution_id: &str) -> LogRef {
    LogRef {
        backend: "sms_log".to_string(),
        uri_prefix: format!("smslog://executions/{}/", execution_id),
        content_type: "text/plain".to_string(),
        compression: "".to_string(),
    }
}
