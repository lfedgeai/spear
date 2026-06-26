//! RTASR transcription session for the Rust voice chat sample.
//! Rust 版语音对话 sample 的 RTASR 转写会话。

use spear_wasm::{Fd, SpearError};
use spear_wasm_helper::{
    rtasr_protocol::TranscriptEvent,
    rtasr_session::{BaseRtasrSession, RtasrConnectOptions},
};

use crate::{
    config::RTASR_TRANSPORT,
};

#[derive(Debug)]
pub struct VoiceTranscriptionSession {
    base: BaseRtasrSession,
    written_bytes: usize,
}

impl VoiceTranscriptionSession {
    pub fn connect() -> Result<Self, SpearError> {
        let base = BaseRtasrSession::connect(RtasrConnectOptions::new(RTASR_TRANSPORT))?;
        Ok(Self {
            base,
            written_bytes: 0,
        })
    }

    pub fn fd(&self) -> Fd {
        self.base.fd()
    }

    pub fn prepare_for_new_utterance(&mut self) -> Result<(), SpearError> {
        self.base.clear()?;
        self.written_bytes = 0;
        Ok(())
    }

    pub fn append_audio(&mut self, audio: &[u8]) -> Result<(), SpearError> {
        if audio.is_empty() {
            return Ok(());
        }
        self.base.write_audio(audio)?;
        self.written_bytes += audio.len();
        Ok(())
    }

    pub fn written_bytes(&self) -> usize {
        self.written_bytes
    }

    pub fn flush(&mut self) -> Result<(), SpearError> {
        self.base.flush()
    }

    pub fn drain_events(&mut self) -> Result<Vec<TranscriptEvent>, SpearError> {
        self.base.drain_transcript_events()
    }

    pub fn close(self) -> Result<(), SpearError> {
        self.base.close()
    }
}
