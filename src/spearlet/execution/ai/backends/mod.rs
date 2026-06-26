pub mod http_json;
pub mod ollama_chat;
pub mod openai_chat_completion;
pub mod openai_realtime_ws;
pub mod stub;

pub const KIND_PREFIX_OPENAI: &str = "openai_";
pub const KIND_OPENAI_CHAT_COMPLETION: &str = "openai_chat_completion";
pub const KIND_OPENAI_REALTIME_WS: &str = "openai_realtime_ws";
pub const KIND_OLLAMA_CHAT: &str = "ollama_chat";
pub const KIND_STUB: &str = "stub";

use crate::spearlet::execution::ai::ir::{
    CanonicalError, CanonicalRequestEnvelope, CanonicalResponseEnvelope,
};
use crate::spearlet::execution::ai::streaming::StreamingPlan;

pub trait BackendAdapter: Send + Sync {
    fn name(&self) -> &str;

    fn invoke(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<CanonicalResponseEnvelope, CanonicalError>;

    fn streaming_plan(
        &self,
        req: &CanonicalRequestEnvelope,
    ) -> Result<StreamingPlan, CanonicalError> {
        Err(CanonicalError {
            code: "unsupported_streaming".to_string(),
            message: "streaming not supported".to_string(),
            retryable: false,
            operation: Some(req.operation.clone()),
        })
    }
}
