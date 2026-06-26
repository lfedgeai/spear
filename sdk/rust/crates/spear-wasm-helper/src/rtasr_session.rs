//! Shared RTASR session skeleton for Rust-first samples.
//! Rust-first 示例共享的 RTASR 会话骨架。

use spear_wasm::{
    rtasr_clear, rtasr_close, rtasr_connect, rtasr_create, rtasr_flush, rtasr_read_alloc,
    rtasr_set_autoflush_json, rtasr_set_param_string, rtasr_write, Fd, SpearError,
};

use crate::rtasr_protocol::{parse_transcript_event, TranscriptEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtasrConnectOptions {
    pub transport: &'static str,
    pub autoflush_json: Option<&'static str>,
}

impl RtasrConnectOptions {
    pub fn new(transport: &'static str) -> Self {
        Self {
            transport,
            autoflush_json: None,
        }
    }

    pub fn with_autoflush_json(mut self, autoflush_json: &'static str) -> Self {
        self.autoflush_json = Some(autoflush_json);
        self
    }
}

#[derive(Debug)]
pub struct BaseRtasrSession {
    fd: Fd,
}

impl BaseRtasrSession {
    pub fn connect(options: RtasrConnectOptions) -> Result<Self, SpearError> {
        let fd = rtasr_create()?;
        if let Some(autoflush_json) = options.autoflush_json {
            rtasr_set_autoflush_json(fd, autoflush_json)?;
        }
        rtasr_set_param_string(fd, "transport", options.transport)?;
        rtasr_connect(fd)?;
        Ok(Self { fd })
    }

    pub fn fd(&self) -> Fd {
        self.fd
    }

    pub fn clear(&mut self) -> Result<(), SpearError> {
        rtasr_clear(self.fd)
    }

    pub fn write_audio(&mut self, audio: &[u8]) -> Result<(), SpearError> {
        if audio.is_empty() {
            return Ok(());
        }
        rtasr_write(self.fd, audio)
    }

    pub fn flush(&mut self) -> Result<(), SpearError> {
        rtasr_flush(self.fd)
    }

    pub fn drain_transcript_events(&mut self) -> Result<Vec<TranscriptEvent>, SpearError> {
        let mut events = Vec::new();
        loop {
            match rtasr_read_alloc(self.fd) {
                Ok(Some(payload)) => {
                    if let Some(event) = parse_transcript_event(&payload) {
                        events.push(event);
                    }
                }
                Ok(None) => return Ok(events),
                Err(err) => return Err(err),
            }
        }
    }

    pub fn close(self) -> Result<(), SpearError> {
        rtasr_close(self.fd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_options_default_to_no_autoflush() {
        let options = RtasrConnectOptions::new("websocket");
        assert_eq!(options.transport, "websocket");
        assert_eq!(options.autoflush_json, None);
    }

    #[test]
    fn connect_options_store_autoflush_json() {
        let options = RtasrConnectOptions::new("websocket")
            .with_autoflush_json(r#"{"strategy":"server_vad"}"#);
        assert_eq!(options.autoflush_json, Some(r#"{"strategy":"server_vad"}"#));
    }
}
