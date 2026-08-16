use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::proto::sms::{McpRegistryEvent, McpServerRecord, NodeBackendSnapshot};
use crate::sms::registry_watch::RegistryWatchHub;

#[derive(Debug)]
pub struct McpRegistryState {
    pub records: RwLock<HashMap<String, McpServerRecord>>,
    pub watch: RegistryWatchHub<McpRegistryEvent>,
}

#[derive(Debug)]
pub struct BackendRegistryState {
    pub snapshots: RwLock<HashMap<String, NodeBackendSnapshot>>,
}

impl McpRegistryState {
    pub fn new(event_buffer_size: usize, broadcast_buffer_size: usize) -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            watch: RegistryWatchHub::new(event_buffer_size, broadcast_buffer_size),
        }
    }

    pub fn current_revision(&self) -> u64 {
        self.watch.current_revision()
    }

    pub fn bump_revision(&self) -> u64 {
        self.watch.bump_revision()
    }

    pub async fn push_event(&self, event: McpRegistryEvent) {
        self.watch.push_event(event).await;
    }
}

impl BackendRegistryState {
    pub fn new() -> Self {
        Self {
            snapshots: RwLock::new(HashMap::new()),
        }
    }
}
