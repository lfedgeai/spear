use crate::proto::sms::{Node, NodeCandidate};
use crate::sms::placement::state::PlacementState;
use crate::sms::services::resource_service::NodeResourceInfo;

/// Whether one node is eligible for placement before resource scoring.
/// 一个节点在资源打分前是否具备 placement 资格。
pub fn is_candidate_node(
    node: &Node,
    now: i64,
    heartbeat_timeout: i64,
    placement_state: &PlacementState,
) -> bool {
    if node.status.to_ascii_lowercase() != "online" {
        return false;
    }
    if now - node.last_heartbeat > heartbeat_timeout {
        return false;
    }
    if placement_state.is_blocked(&node.uuid, now) {
        return false;
    }
    true
}

/// Score one node from current resource usage and historical penalty score.
/// 基于当前资源占用和历史惩罚分为节点打分。
pub fn score_node(resource: Option<&NodeResourceInfo>, penalty_score: f64) -> f64 {
    let (cpu, mem, disk, load) = if let Some(r) = resource {
        (
            r.cpu_usage_percent,
            r.memory_usage_percent,
            r.disk_usage_percent,
            r.load_average_1m,
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let mut score = 100.0;
    // Simple weighted scoring: lower usage/load => higher score.
    // 简单加权评分：资源占用/负载越低，分数越高。
    score -= cpu.min(100.0) * 0.5;
    score -= mem.min(100.0) * 0.3;
    score -= disk.min(100.0) * 0.1;
    score -= (load.min(16.0) / 16.0) * 10.0;
    // Apply penalty score derived from historical retryable failures.
    // 基于历史可重试失败的惩罚项。
    score -= penalty_score * 5.0;
    score
}

/// Convert one node and its score into the placement response candidate.
/// 将节点及其分数转换为 placement 返回候选。
pub fn build_candidate(node: &Node, score: f64) -> NodeCandidate {
    NodeCandidate {
        node_uuid: node.uuid.clone(),
        ip_address: node.ip_address.clone(),
        port: node.port,
        score,
    }
}

/// Sort candidates by score descending and keep at most `max_candidates`.
/// 按分数降序排列候选，并截断到 `max_candidates`。
pub fn select_top_candidates(
    mut candidates: Vec<NodeCandidate>,
    max_candidates: u32,
) -> Vec<NodeCandidate> {
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    candidates.truncate(max_candidates as usize);
    candidates
}
