use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;
use tracing::warn;

use super::{
    artifact::Artifact,
    manager::ExecutionStatistics,
    task::{Task, TaskId},
    ArtifactId,
};

/// Finalize local task removal after instances are drained.
/// 在实例排空后完成本地 task 的最终拆除。
pub(super) async fn finalize_local_task_removal(
    task_id: &str,
    tasks: &Arc<DashMap<TaskId, Arc<Task>>>,
    artifacts: &Arc<DashMap<ArtifactId, Arc<Artifact>>>,
    instance_count: usize,
    statistics: &Arc<RwLock<ExecutionStatistics>>,
) {
    if let Some((_, removed_task)) = tasks.remove(task_id) {
        removed_task.mark_stopped();
        if let Some(artifact_entry) = artifacts.get(removed_task.artifact_id()) {
            let artifact = artifact_entry.value();
            if let Err(error) = artifact.remove_task(task_id) {
                warn!(
                    task_id = %task_id,
                    artifact_id = %removed_task.artifact_id(),
                    error = %error,
                    "Failed to detach task from artifact during delete"
                );
            }
        }
    }

    let mut stats = statistics.write();
    stats.active_tasks = tasks.len() as u64;
    stats.active_instances = instance_count as u64;
}
