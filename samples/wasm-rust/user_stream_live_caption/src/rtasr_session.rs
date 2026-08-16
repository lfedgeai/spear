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
    pending_chunks: Vec<Vec<u8>>,
    pending_bytes: usize,
    audio_bytes_since_flush: usize,
}

impl PendingAudioBuffer {
    fn reset(&mut self) {
        self.pending_chunks.clear();
        self.pending_bytes = 0;
        self.audio_bytes_since_flush = 0;
    }

    fn append(&mut self, audio: &[u8]) {
        if audio.is_empty() {
            return;
        }
        self.pending_bytes += audio.len();
        self.pending_chunks.push(audio.to_vec());
    }

    fn should_write_pending(&self) -> bool {
        self.pending_bytes >= AGGREGATE_WRITE_BYTES
    }

    fn can_flush_commit(&self) -> bool {
        self.audio_bytes_since_flush + self.pending_bytes >= MIN_COMMIT_AUDIO_BYTES
    }

    fn take_merged(&mut self) -> Option<Vec<u8>> {
        if self.pending_bytes == 0 {
            return None;
        }
        let mut merged = Vec::with_capacity(self.pending_bytes);
        for chunk in self.pending_chunks.drain(..) {
            merged.extend_from_slice(&chunk);
        }
        self.pending_bytes = 0;
        Some(merged)
    }

    fn record_write(&mut self, bytes: usize) {
        self.audio_bytes_since_flush += bytes;
    }

    fn reset_flush_counter(&mut self) {
        self.audio_bytes_since_flush = 0;
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
        self.pending_audio.reset();
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
        let Some(audio) = self.pending_audio.take_merged() else {
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
}
