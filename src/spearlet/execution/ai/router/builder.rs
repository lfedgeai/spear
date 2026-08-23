use std::collections::HashSet;

use crate::spearlet::ai::backend_assembly::build_instance_from_config;
use crate::spearlet::ai::credential_resolver::CredentialResolver;
use crate::spearlet::execution::ai::router::policy::SelectionPolicy;
use crate::spearlet::execution::ai::router::registry::{BackendInstance, BackendRegistry};
use crate::spearlet::execution::runtime::RuntimeConfig;

/// Build an in-process backend registry from a *runtime config snapshot*.
/// 从 *runtime config 快照* 构建进程内 backend registry。
///
/// Notes for developers / 开发者备注：
/// - This function is intentionally SMS-agnostic: it does NOT fetch or read any SMS state.
///   本函数刻意与 SMS 解耦：不会读取或拉取任何 SMS 状态。
/// - Runtime SMS-managed remote backends are injected through the dynamic backend registry
///   instead of being merged into `runtime_config.spearlet_config`.
///   运行时 SMS 托管的 remote backends 通过 dynamic backend registry 注入，
///   而不是预先合并进 `runtime_config.spearlet_config`。
/// - `stub` backends are gated by `spearlet.ai.enable_stub_backend` to avoid accidental usage in
///   production environments.
///   `stub` backend 受 `spearlet.ai.enable_stub_backend` 控制，避免在生产环境误用。
pub fn build_registry_from_runtime_config(
    runtime_config: &RuntimeConfig,
) -> (BackendRegistry, SelectionPolicy) {
    let Some(cfg) = runtime_config.spearlet_config.as_ref() else {
        return (
            BackendRegistry::new(Vec::new()),
            SelectionPolicy::WeightedRandom,
        );
    };
    RegistryBuilder::new(runtime_config, cfg).build()
}

struct RegistryBuilder<'a> {
    cfg: &'a crate::spearlet::config::SpearletConfig,
    seen_names: HashSet<String>,
    credential_resolver: CredentialResolver,
}

impl<'a> RegistryBuilder<'a> {
    fn new(
        runtime_config: &'a RuntimeConfig,
        cfg: &'a crate::spearlet::config::SpearletConfig,
    ) -> Self {
        Self {
            cfg,
            seen_names: HashSet::new(),
            credential_resolver: CredentialResolver::from_config_with_env(
                cfg,
                &runtime_config.global_environment,
            ),
        }
    }

    fn build(mut self) -> (BackendRegistry, SelectionPolicy) {
        let policy = match self.cfg.ai.default_policy.as_deref() {
            Some("weighted_random") | None => SelectionPolicy::WeightedRandom,
            _ => SelectionPolicy::WeightedRandom,
        };

        let instances = self
            .cfg
            .ai
            .backends
            .iter()
            .filter_map(|b| self.try_build_instance(b))
            .collect::<Vec<_>>();

        (BackendRegistry::new(instances), policy)
    }

    fn stub_enabled(&self) -> bool {
        self.cfg.ai.enable_stub_backend
    }

    fn try_build_instance(
        &mut self,
        b: &crate::spearlet::config::AiBackendConfig,
    ) -> Option<BackendInstance> {
        if !self.seen_names.insert(b.name.clone()) {
            tracing::warn!(backend = %b.name, kind = %b.kind, "duplicated backend name");
            return None;
        }

        build_instance_from_config(b, Some(&self.credential_resolver), self.stub_enabled())
    }
}
