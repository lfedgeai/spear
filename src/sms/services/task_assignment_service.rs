use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::proto::sms::TaskPlacementAssignment;

#[derive(Debug, Clone, Default)]
pub struct AssignmentReplaceResult {
    pub generation: u64,
    pub upserts: Vec<TaskPlacementAssignment>,
    pub deletes: Vec<TaskPlacementAssignment>,
}

#[derive(Debug, Clone)]
pub struct TaskAssignmentService {
    assignments: Arc<RwLock<HashMap<(String, String), TaskPlacementAssignment>>>,
    generations: Arc<RwLock<HashMap<String, u64>>>,
}

impl TaskAssignmentService {
    pub fn new() -> Self {
        Self {
            assignments: Arc::new(RwLock::new(HashMap::new())),
            generations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn list_node_assignments(&self, node_uuid: &str) -> Vec<TaskPlacementAssignment> {
        let assignments = self.assignments.read().await;
        let mut out: Vec<_> = assignments
            .values()
            .filter(|assignment| assignment.node_uuid == node_uuid)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        out
    }

    pub async fn list_task_assignments(&self, task_id: &str) -> Vec<TaskPlacementAssignment> {
        let assignments = self.assignments.read().await;
        let mut out: Vec<_> = assignments
            .values()
            .filter(|assignment| assignment.task_id == task_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.node_uuid.cmp(&b.node_uuid));
        out
    }

    pub async fn replace_task_assignments(
        &self,
        task_id: &str,
        desired_instances_by_node: HashMap<String, u32>,
        updated_at_ms: i64,
    ) -> AssignmentReplaceResult {
        let generation = {
            let mut generations = self.generations.write().await;
            let entry = generations.entry(task_id.to_string()).or_insert(0);
            *entry = entry.saturating_add(1);
            *entry
        };

        let mut assignments = self.assignments.write().await;
        let existing_keys: Vec<_> = assignments
            .keys()
            .filter(|(existing_task_id, _)| existing_task_id == task_id)
            .cloned()
            .collect();

        let mut result = AssignmentReplaceResult {
            generation,
            ..Default::default()
        };

        for node_uuid in existing_keys
            .iter()
            .map(|(_, node_uuid)| node_uuid.clone())
            .collect::<Vec<_>>()
        {
            if desired_instances_by_node.contains_key(&node_uuid) {
                continue;
            }
            if let Some(removed) = assignments.remove(&(task_id.to_string(), node_uuid.clone())) {
                result.deletes.push(removed);
            }
        }

        for (node_uuid, desired_instances) in desired_instances_by_node {
            let assignment = TaskPlacementAssignment {
                task_id: task_id.to_string(),
                node_uuid: node_uuid.clone(),
                desired_instances,
                generation,
                updated_at_ms,
            };
            assignments.insert((task_id.to_string(), node_uuid), assignment.clone());
            result.upserts.push(assignment);
        }

        result
    }

    pub async fn remove_task_assignments(
        &self,
        task_id: &str,
        _updated_at_ms: i64,
    ) -> AssignmentReplaceResult {
        self.replace_task_assignments(task_id, HashMap::new(), 0)
            .await
    }
}
