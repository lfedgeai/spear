use dashmap::DashMap;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminationScope {
    Execution,
    Instance,
}

#[derive(Clone, Debug)]
pub struct TerminationSnapshot {
    pub scope: TerminationScope,
    pub errno: i32,
    pub message: Option<String>,
}

#[derive(Debug)]
struct TerminationState {
    terminated: AtomicBool,
    scope: TerminationScope,
    errno: i32,
    message: Mutex<Option<String>>,
}

impl TerminationState {
    fn snapshot(&self) -> TerminationSnapshot {
        TerminationSnapshot {
            scope: self.scope,
            errno: self.errno,
            message: self.message.lock().clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminationRequestRegistry {
    scope: TerminationScope,
    inner: Arc<DashMap<String, Arc<TerminationState>>>,
}

impl TerminationRequestRegistry {
    fn new(scope: TerminationScope) -> Self {
        Self {
            scope,
            inner: Arc::new(DashMap::new()),
        }
    }

    pub fn request_termination(&self, key: &str, errno: i32, message: Option<String>) {
        let entry = self.inner.entry(key.to_string()).or_insert_with(|| {
            Arc::new(TerminationState {
                terminated: AtomicBool::new(false),
                scope: self.scope,
                errno,
                message: Mutex::new(None),
            })
        });
        entry.terminated.store(true, Ordering::Release);
        *entry.message.lock() = message;
    }

    pub fn clear_termination_request(&self, key: &str) {
        self.inner.remove(key);
    }

    pub fn read_termination_request(&self, key: &str) -> Option<TerminationSnapshot> {
        let entry = self.inner.get(key)?;
        if !entry.terminated.load(Ordering::Acquire) {
            return None;
        }
        Some(entry.snapshot())
    }
}

static EXEC_REGISTRY: OnceLock<Arc<TerminationRequestRegistry>> = OnceLock::new();
static INSTANCE_REGISTRY: OnceLock<Arc<TerminationRequestRegistry>> = OnceLock::new();

pub fn execution_termination_registry() -> Arc<TerminationRequestRegistry> {
    EXEC_REGISTRY
        .get_or_init(|| Arc::new(TerminationRequestRegistry::new(TerminationScope::Execution)))
        .clone()
}

pub fn instance_termination_registry() -> Arc<TerminationRequestRegistry> {
    INSTANCE_REGISTRY
        .get_or_init(|| Arc::new(TerminationRequestRegistry::new(TerminationScope::Instance)))
        .clone()
}

pub fn register_execution_termination_request(
    execution_id: &str,
    errno: i32,
    reason: Option<String>,
) {
    execution_termination_registry().request_termination(execution_id, errno, reason);
}

pub fn clear_execution_termination_request(execution_id: &str) {
    execution_termination_registry().clear_termination_request(execution_id);
}

pub fn register_instance_destruction_request(
    instance_id: &str,
    errno: i32,
    reason: Option<String>,
) {
    instance_termination_registry().request_termination(instance_id, errno, reason);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spearlet::execution::host_api::DefaultHostApi;
    use crate::spearlet::execution::runtime::{ResourcePoolConfig, RuntimeConfig, RuntimeType};
    use std::collections::HashMap;

    fn test_runtime_config() -> RuntimeConfig {
        RuntimeConfig {
            runtime_type: RuntimeType::Wasm,
            settings: HashMap::new(),
            global_environment: HashMap::new(),
            spearlet_config: None,
            resource_pool: ResourcePoolConfig::default(),
        }
    }

    #[test]
    fn test_exec_registry_mark_check_clear() {
        let exec_id = "exec-test-1";
        let reg = execution_termination_registry();
        reg.clear_termination_request(exec_id);

        assert!(reg.read_termination_request(exec_id).is_none());

        reg.request_termination(exec_id, -11, Some("terminated".to_string()));
        let s = reg.read_termination_request(exec_id).unwrap();
        assert_eq!(s.scope, TerminationScope::Execution);
        assert_eq!(s.errno, -11);
        assert_eq!(s.message.as_deref(), Some("terminated"));

        reg.clear_termination_request(exec_id);
        assert!(reg.read_termination_request(exec_id).is_none());
    }

    #[test]
    fn test_instance_registry_mark_check_clear() {
        let instance_id = "inst-test-1";
        let reg = instance_termination_registry();
        reg.clear_termination_request(instance_id);

        assert!(reg.read_termination_request(instance_id).is_none());

        reg.request_termination(instance_id, -123, Some("destroyed".to_string()));
        let s = reg.read_termination_request(instance_id).unwrap();
        assert_eq!(s.scope, TerminationScope::Instance);
        assert_eq!(s.errno, -123);
        assert_eq!(s.message.as_deref(), Some("destroyed"));

        reg.clear_termination_request(instance_id);
        assert!(reg.read_termination_request(instance_id).is_none());
    }

    #[test]
    fn test_default_host_api_prefers_execution_over_instance() {
        let exec_id = "exec-test-2";
        let instance_id = "inst-test-2";
        execution_termination_registry().clear_termination_request(exec_id);
        instance_termination_registry().clear_termination_request(instance_id);

        register_execution_termination_request(exec_id, -libc::ECANCELED, Some("e".to_string()));
        register_instance_destruction_request(instance_id, -libc::ECANCELED, Some("i".to_string()));

        let mut api =
            DefaultHostApi::new(test_runtime_config()).with_instance_id(instance_id.to_string());
        api.set_execution_id(Some(exec_id.to_string()));

        let s = api.read_wasm_termination_request().unwrap();
        assert_eq!(s.scope, TerminationScope::Execution);
        assert_eq!(s.message.as_deref(), Some("e"));

        execution_termination_registry().clear_termination_request(exec_id);
        instance_termination_registry().clear_termination_request(instance_id);
    }
}
