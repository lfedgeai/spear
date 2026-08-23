//! RTASR session orchestration for the Rust live caption sample.
//! Rust 版实时字幕 sample 的 RTASR 会话编排。

use spear_wasm::{Fd, SpearError};
use spear_wasm_helper::{
    rtasr_protocol::TranscriptEvent,
    rtasr_session::{BaseRtasrSession, RtasrConnectOptions},
};

use crate::{
    config::{
        AGGREGATE_WRITE_BYTES, MIN_COMMIT_AUDIO_BYTES, RTASR_AUTOFLUSH_JSON, RTASR_TRANSPORT,
    },
};

fn debug_guest(message: impl std::fmt::Display) {
    eprintln!("[live_caption] {message}");
}

#[derive(Debug, Default)]
struct PendingAudioBuffer {
    // Keep a single contiguous buffer so the session logic does not need to
    // manage per-chunk bookkeeping before each write.
    // 使用单一连续缓冲区，避免在每次写入前维护按 chunk 拆分的额外状态。
    pending_audio: Vec<u8>,
    bytes_since_flush: usize,
}

impl PendingAudioBuffer {
    fn clear(&mut self) {
        self.pending_audio.clear();
        self.bytes_since_flush = 0;
    }

    fn append(&mut self, audio: &[u8]) {
        self.pending_audio.extend_from_slice(audio);
    }

    fn should_write_pending(&self) -> bool {
        self.pending_audio.len() >= AGGREGATE_WRITE_BYTES
    }

    fn can_flush_commit(&self) -> bool {
        self.bytes_since_flush + self.pending_audio.len() >= MIN_COMMIT_AUDIO_BYTES
    }

    fn take_pending_audio(&mut self) -> Option<Vec<u8>> {
        if self.pending_audio.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.pending_audio))
    }

    fn record_write(&mut self, bytes: usize) {
        self.bytes_since_flush += bytes;
    }

    fn reset_flush_counter(&mut self) {
        self.bytes_since_flush = 0;
    }
}

#[derive(Debug)]
pub struct LiveCaptionRtasr {
    base: BaseRtasrSession,
    pending_audio: PendingAudioBuffer,
}

impl LiveCaptionRtasr {
    pub fn connect() -> Result<Self, SpearError> {
        debug_guest("rtasr connect start");
        let base = BaseRtasrSession::connect(
            RtasrConnectOptions::new(RTASR_TRANSPORT).with_autoflush_json(RTASR_AUTOFLUSH_JSON),
        )?;
        debug_guest(format!("rtasr connect ok fd={}", base.fd().raw()));
        Ok(Self {
            base,
            pending_audio: PendingAudioBuffer::default(),
        })
    }

    pub fn fd(&self) -> Fd {
        self.base.fd()
    }

    pub fn prepare_for_new_utterance(&mut self) -> Result<(), SpearError> {
        debug_guest("rtasr clear start");
        self.base.clear()?;
        debug_guest("rtasr clear ok");
        self.pending_audio.clear();
        Ok(())
    }

    pub fn append_audio(&mut self, audio: &[u8]) -> Result<(), SpearError> {
        self.pending_audio.append(audio);
        if self.pending_audio.should_write_pending() {
            self.write_pending_audio()?;
        }
        Ok(())
    }

    pub fn flush_commit(&mut self) -> Result<(), SpearError> {
        self.write_pending_audio()?;
        if !self.pending_audio.can_flush_commit() {
            debug_guest("rtasr flush skipped due to insufficient audio");
            return Ok(());
        }
        debug_guest("rtasr flush start");
        self.base.flush()?;
        debug_guest("rtasr flush ok");
        self.pending_audio.reset_flush_counter();
        Ok(())
    }

    pub fn drain_events(&mut self) -> Result<Vec<TranscriptEvent>, SpearError> {
        debug_guest("rtasr read events start");
        self.base.drain_transcript_events()
    }

    pub fn close(self) -> Result<(), SpearError> {
        self.base.close()
    }

    fn write_pending_audio(&mut self) -> Result<(), SpearError> {
        let Some(audio) = self.pending_audio.take_pending_audio() else {
            return Ok(());
        };
        let written = audio.len();
        debug_guest(format!("rtasr write_audio start bytes={written}"));
        self.base.write_audio(&audio)?;
        debug_guest(format!("rtasr write_audio ok bytes={written}"));
        self.pending_audio.record_write(written);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_audio_requires_threshold_before_write() {
        let mut buffer = PendingAudioBuffer::default();
        buffer.append(&vec![0u8; AGGREGATE_WRITE_BYTES - 1]);
        assert!(!buffer.should_write_pending());
        buffer.append(&[0u8]);
        assert!(buffer.should_write_pending());
    }

    #[test]
    fn pending_audio_commit_guard_matches_minimum_audio_requirement() {
        let mut buffer = PendingAudioBuffer::default();
        buffer.append(&vec![0u8; MIN_COMMIT_AUDIO_BYTES - 1]);
        assert!(!buffer.can_flush_commit());
        buffer.append(&[0u8]);
        assert!(buffer.can_flush_commit());
    }

    #[test]
    fn pending_audio_take_preserves_append_order() {
        let mut buffer = PendingAudioBuffer::default();
        buffer.append(&[1, 2]);
        buffer.append(&[3, 4]);

        assert_eq!(buffer.take_pending_audio(), Some(vec![1, 2, 3, 4]));
        assert_eq!(buffer.take_pending_audio(), None);
    }

    #[test]
    fn pending_audio_clear_resets_flush_guard_state() {
        let mut buffer = PendingAudioBuffer::default();
        buffer.record_write(MIN_COMMIT_AUDIO_BYTES);
        assert!(buffer.can_flush_commit());

        buffer.clear();

        assert!(!buffer.can_flush_commit());
    }
}
