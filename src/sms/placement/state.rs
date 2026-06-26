use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;

use crate::proto::sms::InvocationOutcomeClass;

#[derive(Debug, Clone)]
struct NodePenalty {
    // Consecutive retryable failures to drive exponential backoff.
    // 连续可重试失败次数，用于指数退避。
    consecutive_failures: u32,
    // If now < blocked_until, the node is temporarily removed from candidate set.
    // 若 now < blocked_until，则节点被临时熔断，不参与候选。
    blocked_until: i64,
    // Timestamp of last failure.
    // 最近一次失败的时间戳。
    last_failure_at: i64,
}

/// In-memory placement penalty state used by SMS spillback scheduling.
/// SMS spillback 调度使用的内存惩罚状态。
#[derive(Debug)]
pub struct PlacementState {
    node_penalties: DashMap<String, NodePenalty>,
    penalty_ops: AtomicU64,
}

impl PlacementState {
    pub fn new() -> Self {
        Self {
            node_penalties: DashMap::new(),
            penalty_ops: AtomicU64::new(0),
        }
    }

    pub fn maybe_prune_node_penalties(&self, now: i64) {
        const PENALTY_TTL_SECS: i64 = 3600;

        let op = self.penalty_ops.fetch_add(1, Ordering::Relaxed);
        if op % 256 != 0 {
            return;
        }

        let mut to_remove: Vec<String> = Vec::new();
        for item in self.node_penalties.iter() {
            let p = item.value();
            if p.blocked_until > now {
                continue;
            }
            if p.last_failure_at == 0 {
                to_remove.push(item.key().clone());
                continue;
            }
            if now - p.last_failure_at > PENALTY_TTL_SECS {
                to_remove.push(item.key().clone());
            }
        }
        for k in to_remove {
            self.node_penalties.remove(&k);
        }
    }

    pub fn is_blocked(&self, node_uuid: &str, now: i64) -> bool {
        self.node_penalties
            .get(node_uuid)
            .map(|p| p.blocked_until > now)
            .unwrap_or(false)
    }

    pub fn penalty_score(&self, node_uuid: &str, now: i64) -> f64 {
        self.node_penalties
            .get(node_uuid)
            .map(|p| {
                if p.blocked_until > now {
                    10.0
                } else {
                    (p.consecutive_failures as f64).min(10.0)
                }
            })
            .unwrap_or(0.0)
    }

    pub fn apply_outcome(&self, node_uuid: String, outcome_class: InvocationOutcomeClass) {
        let now = chrono::Utc::now().timestamp();
        match outcome_class {
            InvocationOutcomeClass::Success => {
                // Success clears penalty state.
                // 成功会清空惩罚状态。
                self.node_penalties.remove(&node_uuid);
            }
            InvocationOutcomeClass::Overloaded
            | InvocationOutcomeClass::Unavailable
            | InvocationOutcomeClass::Timeout => {
                // Retryable failures trigger exponential backoff with a hard cap.
                // 可重试失败触发指数退避，并设置硬上限。
                self.node_penalties
                    .entry(node_uuid)
                    .and_modify(|p| {
                        p.consecutive_failures = p.consecutive_failures.saturating_add(1);
                        p.last_failure_at = now;
                        let base = 10i64;
                        let backoff = base * (1i64 << (p.consecutive_failures.min(5)));
                        p.blocked_until = (now + backoff).min(now + 300);
                    })
                    .or_insert(NodePenalty {
                        consecutive_failures: 1,
                        blocked_until: (now + 20).min(now + 300),
                        last_failure_at: now,
                    });
            }
            _ => {}
        }

        self.maybe_prune_node_penalties(now);
    }

    #[cfg(test)]
    pub fn get_node_penalty_snapshot(&self, node_uuid: &str) -> Option<(u32, i64, i64)> {
        self.node_penalties
            .get(node_uuid)
            .map(|p| (p.consecutive_failures, p.blocked_until, p.last_failure_at))
    }
}
