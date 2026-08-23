//! User-stream state and buffered text output helpers.
//! user stream 状态与文本缓冲输出辅助类型。

use std::collections::VecDeque;

use spear_ssf::{build_v1_frame, MsgType};
use spear_wasm::{
    constants, user_stream_close, user_stream_open, user_stream_write, Fd, SpearError,
    UserStreamDirection,
};

use crate::event_loop::EpollDriver;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamModality {
    Text,
    Audio,
}

#[derive(Debug)]
pub struct ManagedStream {
    pub stream_id: u32,
    pub modality: StreamModality,
    fd: Option<Fd>,
    opened: bool,
}

impl ManagedStream {
    pub fn new(stream_id: u32, modality: StreamModality) -> Self {
        Self {
            stream_id,
            modality,
            fd: None,
            opened: false,
        }
    }

    pub fn open_from_runtime(&mut self) -> Result<Fd, SpearError> {
        let fd = user_stream_open(self.stream_id, UserStreamDirection::Bidirectional)?;
        self.fd = Some(fd);
        self.opened = false;
        Ok(fd)
    }

    pub fn fd(&self) -> Option<Fd> {
        self.fd
    }

    pub fn raw_fd(&self) -> Option<i32> {
        self.fd.map(Fd::raw)
    }

    pub fn mark_protocol_opened(&mut self) {
        self.opened = true;
    }

    pub fn can_write_data(&self) -> bool {
        self.fd.is_some() && self.opened
    }

    pub fn take_fd(&mut self) -> Option<Fd> {
        self.opened = false;
        self.fd.take()
    }

    pub fn close(&mut self) -> Result<(), SpearError> {
        if let Some(fd) = self.take_fd() {
            user_stream_close(fd)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct StreamEndpoint {
    stream: ManagedStream,
}

impl StreamEndpoint {
    pub fn new(stream_id: u32, modality: StreamModality) -> Self {
        Self {
            stream: ManagedStream::new(stream_id, modality),
        }
    }

    pub fn stream_id(&self) -> u32 {
        self.stream.stream_id
    }

    pub fn modality(&self) -> StreamModality {
        self.stream.modality
    }

    pub fn fd(&self) -> Option<Fd> {
        self.stream.fd()
    }

    pub fn raw_fd(&self) -> Option<i32> {
        self.stream.raw_fd()
    }

    pub fn matches_fd(&self, fd: i32) -> bool {
        self.raw_fd() == Some(fd)
    }

    pub fn is_connected(&self) -> bool {
        self.fd().is_some()
    }

    pub fn mark_protocol_opened(&mut self) {
        self.stream.mark_protocol_opened();
    }

    pub fn can_write_data(&self) -> bool {
        self.stream.can_write_data()
    }

    pub fn attach_runtime_fd(
        &mut self,
        epoll: &EpollDriver,
        writable: bool,
    ) -> Result<Fd, SpearError> {
        let fd = self.stream.open_from_runtime()?;
        epoll.add(fd.raw(), self.default_interest_mask(writable))?;
        Ok(fd)
    }

    pub fn close_with_epoll(&mut self, epoll: &EpollDriver) -> Result<(), SpearError> {
        if let Some(fd) = self.raw_fd() {
            epoll.remove(fd);
        }
        self.stream.close()
    }

    pub fn as_stream(&self) -> &ManagedStream {
        &self.stream
    }

    fn default_interest_mask(&self, writable: bool) -> i32 {
        let mut mask =
            constants::SPEAR_EPOLLIN | constants::SPEAR_EPOLLERR | constants::SPEAR_EPOLLHUP;
        if writable {
            mask |= constants::SPEAR_EPOLLOUT;
        }
        mask
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TextAction {
    Data(Vec<u8>),
    Commit,
}

#[derive(Debug, Default)]
pub struct BufferedTextOutput {
    pending: VecDeque<TextAction>,
    out_seq: u64,
    meta_v1_json: &'static [u8],
}

impl BufferedTextOutput {
    pub fn new(meta_v1_json: &'static [u8]) -> Self {
        Self {
            pending: VecDeque::new(),
            out_seq: 0,
            meta_v1_json,
        }
    }

    pub fn write(&mut self, text: impl AsRef<str>) {
        self.pending
            .push_back(TextAction::Data(text.as_ref().as_bytes().to_vec()));
    }

    pub fn commit(&mut self) {
        self.pending.push_back(TextAction::Commit);
    }

    pub fn flush(&mut self, stream: &ManagedStream) -> Result<(), SpearError> {
        let Some(fd) = stream.fd() else {
            return Ok(());
        };
        if !stream.can_write_data() {
            return Ok(());
        }

        while let Some(action) = self.pending.front() {
            let frame =
                encode_action_frame(stream.stream_id, self.out_seq, self.meta_v1_json, action);
            match user_stream_write(fd, &frame) {
                Ok(()) => {
                    self.pending.pop_front();
                    self.out_seq += 1;
                }
                Err(err) if err.is_code("eagain") => break,
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
}

fn encode_action_frame(
    stream_id: u32,
    out_seq: u64,
    meta_v1_json: &[u8],
    action: &TextAction,
) -> Vec<u8> {
    match action {
        TextAction::Data(data) => build_v1_frame(
            stream_id,
            MsgType::Data.as_u16(),
            0,
            out_seq,
            meta_v1_json,
            data,
        ),
        TextAction::Commit => build_v1_frame(
            stream_id,
            MsgType::Commit.as_u16(),
            0,
            out_seq,
            meta_v1_json,
            &[],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spear_ssf::{split_v1, MsgType};

    static META: &[u8] = br#"{"v":1}"#;

    #[test]
    fn encode_data_frame_uses_expected_sequence_and_payload() {
        let frame = encode_action_frame(1, 7, META, &TextAction::Data(b"hello".to_vec()));
        let (header, meta, data) = split_v1(&frame).unwrap();
        assert_eq!(header.stream_id, 1);
        assert_eq!(header.msg_type, MsgType::Data.as_u16());
        assert_eq!(header.seq, 7);
        assert_eq!(meta, META);
        assert_eq!(data, b"hello");
    }
}
