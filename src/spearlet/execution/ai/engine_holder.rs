use arc_swap::ArcSwap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use crate::spearlet::execution::ai::AiEngine;

pub struct EngineHolder {
    engine: ArcSwap<AiEngine>,
    revision: AtomicU64,
}

impl std::fmt::Debug for EngineHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineHolder")
            .field("revision", &self.revision())
            .finish()
    }
}

impl EngineHolder {
    pub fn new(engine: Arc<AiEngine>, revision: u64) -> Self {
        Self {
            engine: ArcSwap::new(engine),
            revision: AtomicU64::new(revision),
        }
    }

    pub fn get(&self) -> Arc<AiEngine> {
        self.engine.load_full()
    }

    pub fn swap(&self, engine: Arc<AiEngine>, revision: u64) {
        self.engine.store(engine);
        self.revision.store(revision, Ordering::Release);
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
}

static GLOBAL: OnceLock<Arc<EngineHolder>> = OnceLock::new();

pub fn init_global(holder: Arc<EngineHolder>) -> Arc<EngineHolder> {
    GLOBAL.get_or_init(|| holder).clone()
}

pub fn global() -> Option<Arc<EngineHolder>> {
    GLOBAL.get().cloned()
}
