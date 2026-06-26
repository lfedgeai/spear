use std::sync::Arc;

use crate::spearlet::execution::ai::backends::BackendAdapter;
use crate::spearlet::execution::ai::ir::CanonicalRequestEnvelope;
use crate::spearlet::execution::ai::router::capabilities::Capabilities;
use crate::proto::sms::BackendSpec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hosting {
    Unknown,
    Local,
    Remote,
}

#[derive(Clone)]
pub struct BackendInstance {
    pub spec: BackendSpec,
    pub hosting: Hosting,
    pub capabilities: Capabilities,
    pub adapter: Arc<dyn BackendAdapter>,
}

#[derive(Clone)]
pub struct BackendRegistry {
    instances: Vec<BackendInstance>,
}

impl BackendRegistry {
    pub fn new(instances: Vec<BackendInstance>) -> Self {
        Self { instances }
    }

    pub fn instances(&self) -> &[BackendInstance] {
        &self.instances
    }

    pub fn candidates<'a>(&'a self, req: &CanonicalRequestEnvelope) -> Vec<&'a BackendInstance> {
        self.instances
            .iter()
            .filter(|inst| inst.capabilities.supports_operation(&req.operation))
            .filter(|inst| {
                req.requirements
                    .required_features
                    .iter()
                    .all(|f| inst.capabilities.has_feature(f))
            })
            .filter(|inst| {
                req.requirements
                    .required_transports
                    .iter()
                    .all(|t| inst.capabilities.transports.iter().any(|x| x == t))
            })
            .collect()
    }
}
