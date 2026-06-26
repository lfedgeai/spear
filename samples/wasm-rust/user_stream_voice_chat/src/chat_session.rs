//! Downstream chat completion helper for the Rust voice chat sample.
//! Rust 版语音对话 sample 的下游 Chat Completion 辅助模块。

use spear_wasm::{cchat_close, cchat_recv_alloc, ChatRequestContext, Fd, SpearError};

use crate::config::CHAT_TIMEOUT_MS;

pub fn send_chat_completion(prompt: &str) -> Result<String, SpearError> {
    let mut request_ctx = ChatRequestContext::create()?;
    let result = send_chat_completion_with_request_context(&mut request_ctx, prompt);
    let close_result = request_ctx.close();
    match (result, close_result) {
        (Ok(text), Ok(())) => Ok(text),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

fn send_chat_completion_with_request_context(
    request_ctx: &mut ChatRequestContext,
    prompt: &str,
) -> Result<String, SpearError> {
    request_ctx.write_message("user", prompt)?;
    request_ctx.set_param_json(&format!(r#"{{"key":"timeout_ms","value":{CHAT_TIMEOUT_MS}}}"#))?;
    let response_fd = request_ctx.send(0)?;
    let response = recv_response_text(response_fd);
    let close_result = cchat_close(response_fd);
    match (response, close_result) {
        (Ok(text), Ok(())) => Ok(text),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

fn recv_response_text(response_fd: Fd) -> Result<String, SpearError> {
    let bytes = cchat_recv_alloc(response_fd)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_json_uses_expected_value() {
        let json = format!(r#"{{"key":"timeout_ms","value":{CHAT_TIMEOUT_MS}}}"#);
        assert!(json.contains("\"timeout_ms\""));
        assert!(json.contains("30000"));
    }
}
