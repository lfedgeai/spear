use std::sync::Arc;
use std::time::Duration;

use crate::sms::instance_execution_index::InstanceExecutionIndex;
use crate::sms::unified_events::UnifiedEventBus;

#[derive(Clone, Copy)]
enum ProjectorKind {
    Instance,
    Execution,
}

impl ProjectorKind {
    fn stream(self) -> &'static str {
        match self {
            Self::Instance => "type.instance",
            Self::Execution => "type.execution",
        }
    }

    fn checkpoint_name(self) -> &'static str {
        self.stream()
    }

    async fn apply_event(
        self,
        idx: &InstanceExecutionIndex,
        op: i32,
        any: &prost_types::Any,
        now_ms: i64,
    ) {
        match self {
            Self::Instance => {
                let _ = idx.apply_instance_event(op, any, now_ms).await;
            }
            Self::Execution => {
                let _ = idx.apply_execution_event(op, any, now_ms).await;
            }
        }
    }
}

/// Start the instance/execution projectors that maintain the execution index.
/// 启动维护执行索引的 instance/execution projector。
pub fn start_index_projectors(idx: Arc<InstanceExecutionIndex>, bus: Arc<UnifiedEventBus>) {
    {
        let idx = idx.clone();
        let bus = bus.clone();
        tokio::spawn(async move {
            run_projector(ProjectorKind::Instance, idx, bus).await;
        });
    }
    tokio::spawn(async move {
        run_projector(ProjectorKind::Execution, idx, bus).await;
    });
}

/// Run one projector from replay phase into live subscription phase.
/// 运行单个 projector，从 replay 阶段切换到实时订阅阶段。
async fn run_projector(
    kind: ProjectorKind,
    idx: Arc<InstanceExecutionIndex>,
    bus: Arc<UnifiedEventBus>,
) {
    let replay_limit = 1000usize;
    let checkpoint_name = kind.checkpoint_name();
    let stream = kind.stream();
    let mut last_seq = idx.load_checkpoint(checkpoint_name).await.unwrap_or(0);

    loop {
        let batch = bus.replay_since(stream, last_seq, replay_limit).await;
        let Ok(events) = batch else {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        };
        if events.is_empty() {
            break;
        }
        for env in events {
            if env.seq <= last_seq {
                continue;
            }
            if let Some(any) = env.payload {
                let now_ms = chrono::Utc::now().timestamp_millis();
                kind.apply_event(&idx, env.op, &any, now_ms).await;
            }
            last_seq = env.seq;
            let _ = idx.store_checkpoint(checkpoint_name, last_seq).await;
        }
    }

    let mut rx = bus.subscribe(stream).await;
    loop {
        match rx.recv().await {
            Ok(env) => {
                if env.seq <= last_seq {
                    continue;
                }
                if let Some(any) = env.payload {
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    kind.apply_event(&idx, env.op, &any, now_ms).await;
                }
                last_seq = env.seq;
                let _ = idx.store_checkpoint(checkpoint_name, last_seq).await;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
