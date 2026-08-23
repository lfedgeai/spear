use std::{fs, io::Write, path::PathBuf};

use tracing::warn;

use crate::spearlet::config::SpearletConfig;

/// Persist task event cursor to local disk for replay continuity.
/// 将 task 事件游标持久化到本地磁盘，保证重放连续性。
#[derive(Debug, Clone)]
pub(crate) struct TaskEventCursorStore {
    path: PathBuf,
}

impl TaskEventCursorStore {
    /// Create a cursor store from spearlet config.
    /// 根据 spearlet 配置创建游标存储。
    pub(crate) fn new(config: &SpearletConfig) -> Self {
        let node_uuid = config.compute_node_uuid();
        let path = PathBuf::from(&config.storage.data_dir)
            .join(format!("task_events_cursor_{}.json", node_uuid));
        Self { path }
    }

    /// Load the last seen event sequence, defaulting to zero.
    /// 加载最近一次消费到的事件序号，默认返回零。
    pub(crate) fn load(&self) -> u64 {
        if let Ok(bytes) = fs::read(&self.path) {
            serde_json::from_slice::<u64>(&bytes).unwrap_or(0)
        } else {
            0
        }
    }

    /// Store the last seen event sequence with best-effort durability.
    /// 尽力持久化最近一次消费到的事件序号。
    pub(crate) fn store(&self, value: u64) {
        let Some(parent) = self.path.parent() else {
            return;
        };
        if let Err(error) = fs::create_dir_all(parent) {
            warn!(error = %error, path = %self.path.display(), "Failed to create task cursor directory");
            return;
        }
        match fs::File::create(&self.path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(
                    serde_json::to_string(&value)
                        .unwrap_or_else(|_| "0".to_string())
                        .as_bytes(),
                ) {
                    warn!(error = %error, path = %self.path.display(), "Failed to write task cursor");
                }
            }
            Err(error) => {
                warn!(error = %error, path = %self.path.display(), "Failed to create task cursor file");
            }
        }
    }
}
