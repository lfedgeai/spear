use std::collections::HashMap;

use crate::proto::sms::{NodeCandidate, Task, TaskSchedulingStrategy, TaskStatus};
use crate::sms::placement::policy::{build_candidate, is_candidate_node, score_node};
use crate::sms::service::SmsServiceImpl;

impl SmsServiceImpl {
    async fn count_active_instances_by_node(
        &self,
        task_id: &str,
    ) -> Result<HashMap<String, usize>, String> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut page_token = String::new();
        let mut counts = HashMap::new();
        for _ in 0..20 {
            let (instances, next) = self
                .instance_execution_index
                .list_task_instances(task_id, now_ms, 100, &page_token)
                .await
                .map_err(|e| e.to_string())?;
            for instance in instances {
                *counts.entry(instance.node_uuid).or_insert(0) += 1;
            }
            if next.is_empty() {
                break;
            }
            page_token = next;
        }
        Ok(counts)
    }

    pub(crate) async fn list_scored_placement_candidates(
        &self,
    ) -> Result<Vec<NodeCandidate>, String> {
        let now_secs = chrono::Utc::now().timestamp();
        self.placement_state.maybe_prune_node_penalties(now_secs);
        let heartbeat_timeout_secs = self.config.heartbeat_timeout as i64;
        let node_service = self.node_service.read().await.clone();
        let nodes = node_service.list_nodes().await.map_err(|e| e.to_string())?;

        let mut candidates = Vec::new();
        for node in nodes {
            if !is_candidate_node(
                &node,
                now_secs,
                heartbeat_timeout_secs,
                &self.placement_state,
            ) {
                continue;
            }
            let resource = match uuid::Uuid::parse_str(&node.uuid) {
                Ok(uuid) => self
                    .resource_service
                    .get_resource(&uuid)
                    .await
                    .ok()
                    .flatten(),
                Err(_) => None,
            };
            let score = score_node(
                resource.as_ref(),
                self.placement_state.penalty_score(&node.uuid, now_secs),
            );
            candidates.push(build_candidate(&node, score));
        }
        Ok(candidates)
    }

    pub(crate) async fn compute_task_assignments(
        &self,
        task: &Task,
    ) -> Result<HashMap<String, u32>, String> {
        let desired_replicas = if task.status == TaskStatus::Deleting as i32 {
            0
        } else {
            task.desired_replicas
        };
        if desired_replicas == 0 {
            return Ok(HashMap::new());
        }

        let candidates = self.list_scored_placement_candidates().await?;
        if candidates.is_empty() {
            return Ok(HashMap::new());
        }

        let spread = task.scheduling_strategy == TaskSchedulingStrategy::Spread as i32;
        let mut synthetic_counts = self.count_active_instances_by_node(&task.task_id).await?;
        let mut assignments = HashMap::new();

        for _ in 0..desired_replicas {
            let chosen = if spread {
                candidates.iter().min_by(|a, b| {
                    let a_count = *synthetic_counts.get(&a.node_uuid).unwrap_or(&0);
                    let b_count = *synthetic_counts.get(&b.node_uuid).unwrap_or(&0);
                    a_count
                        .cmp(&b_count)
                        .then_with(|| b.score.total_cmp(&a.score))
                        .then_with(|| a.node_uuid.cmp(&b.node_uuid))
                })
            } else {
                candidates.iter().max_by(|a, b| {
                    a.score
                        .total_cmp(&b.score)
                        .then_with(|| b.node_uuid.cmp(&a.node_uuid))
                })
            };
            let Some(chosen) = chosen else {
                break;
            };
            *assignments.entry(chosen.node_uuid.clone()).or_insert(0) += 1;
            *synthetic_counts
                .entry(chosen.node_uuid.clone())
                .or_insert(0) += 1;
        }
        Ok(assignments)
    }
}
