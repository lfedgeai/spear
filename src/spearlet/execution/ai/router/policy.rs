use rand::Rng;

use crate::spearlet::execution::ai::ir::{CanonicalError, CanonicalRequestEnvelope};
use crate::spearlet::execution::ai::router::registry::BackendInstance;

#[derive(Clone)]
pub enum SelectionPolicy {
    WeightedRandom,
}

impl SelectionPolicy {
    pub fn select<'a>(
        &self,
        req: &CanonicalRequestEnvelope,
        candidates: Vec<&'a BackendInstance>,
    ) -> Result<&'a BackendInstance, CanonicalError> {
        match self {
            SelectionPolicy::WeightedRandom => select_weighted_random(req, candidates),
        }
    }
}

fn select_weighted_random<'a>(
    req: &CanonicalRequestEnvelope,
    candidates: Vec<&'a BackendInstance>,
) -> Result<&'a BackendInstance, CanonicalError> {
    let total: u32 = candidates.iter().map(|c| c.spec.weight.max(1)).sum();
    let mut rng = rand::thread_rng();
    let mut pick = rng.gen_range(0..total);
    for c in candidates {
        let w = c.spec.weight.max(1);
        if pick < w {
            return Ok(c);
        }
        pick -= w;
    }
    Err(CanonicalError {
        code: "no_candidate_backend".to_string(),
        message: "no candidate backend".to_string(),
        retryable: false,
        operation: Some(req.operation.clone()),
    })
}
