//! Shared SSF v1 and user-stream protocol helpers.
//! 共享的 SSF v1 与 user stream 协议辅助模块。

use spear_ssf::{split_v1, MsgType};
use spear_wasm::ssf_meta::{parse_ctrl_meta_v1, parse_data_meta_v1, CtrlMetaV1, DataMetaV1};

use crate::stream::StreamModality;

#[derive(Debug, Clone)]
pub enum IncomingStreamMessage {
    Ctrl(CtrlMetaV1),
    Data { meta: DataMetaV1, data: Vec<u8> },
    Commit { meta: DataMetaV1 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMessageError {
    InvalidFrame,
    InvalidCtrlMeta,
    InvalidDataMeta,
    InvalidCommitMeta,
    UnsupportedType,
}

pub fn parse_stream_message(frame: &[u8]) -> Result<IncomingStreamMessage, StreamMessageError> {
    let (header, meta, data) = split_v1(frame).map_err(|_| StreamMessageError::InvalidFrame)?;
    let msg_type = MsgType::try_from(header.msg_type).map_err(|_| StreamMessageError::UnsupportedType)?;

    match msg_type {
        MsgType::Ctrl => {
            let ctrl = parse_ctrl_meta_v1(meta).map_err(|_| StreamMessageError::InvalidCtrlMeta)?;
            Ok(IncomingStreamMessage::Ctrl(ctrl))
        }
        MsgType::Data => {
            let payload = parse_data_meta_v1(meta).map_err(|_| StreamMessageError::InvalidDataMeta)?;
            Ok(IncomingStreamMessage::Data {
                meta: payload,
                data: data.to_vec(),
            })
        }
        MsgType::Commit => {
            let payload = parse_data_meta_v1(meta).map_err(|_| StreamMessageError::InvalidCommitMeta)?;
            Ok(IncomingStreamMessage::Commit { meta: payload })
        }
        MsgType::Close | MsgType::Error => Err(StreamMessageError::UnsupportedType),
    }
}

pub fn is_open_handshake(ctrl: &CtrlMetaV1, modality: StreamModality) -> bool {
    matches!(
        (ctrl, modality),
        (CtrlMetaV1::OpenText { .. }, StreamModality::Text)
            | (CtrlMetaV1::OpenAudio { .. }, StreamModality::Audio)
    )
}

pub fn is_audio_utterance_begin(ctrl: &CtrlMetaV1) -> bool {
    matches!(ctrl, CtrlMetaV1::UtteranceBeginAudio { .. })
}
