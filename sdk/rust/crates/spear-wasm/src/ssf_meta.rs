//! SSF meta JSON helpers (v1).
//! SSF meta JSON 辅助模块（v1）。

use crate::{constants, SpearError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaKindV1 {
    Open,
    UtteranceBegin,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaModalityV1 {
    Text,
    Audio,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSpecV1 {
    pub format: String,
    pub sample_rate_hz: u32,
    pub channels: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtrlMetaV1 {
    OpenText {
        ts_ms: Option<u64>,
        trace_id: Option<String>,
    },
    OpenAudio {
        audio: AudioSpecV1,
        ts_ms: Option<u64>,
        trace_id: Option<String>,
    },
    UtteranceBeginAudio {
        utterance_id: String,
        ts_ms: Option<u64>,
        trace_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataMetaV1 {
    pub v: u32,
    pub ts_ms: Option<u64>,
    pub trace_id: Option<String>,
    pub message_id: Option<String>,
    pub seq_in_message: Option<u32>,
    pub utterance_id: Option<String>,
    pub chunk_index: Option<u32>,
}

fn invalid_meta(op: &'static str) -> SpearError {
    SpearError {
        code: "invalid_meta_json",
        errno: -constants::SPEAR_EINVAL,
        op,
    }
}

#[derive(serde::Deserialize)]
struct RawAudioSpecV1 {
    format: Option<String>,
    sample_rate_hz: Option<u32>,
    channels: Option<u32>,
}

#[derive(serde::Deserialize)]
struct RawMetaV1 {
    v: Option<u32>,
    kind: Option<String>,
    modality: Option<String>,
    ts_ms: Option<u64>,
    trace_id: Option<String>,
    message_id: Option<String>,
    seq_in_message: Option<u32>,
    utterance_id: Option<String>,
    chunk_index: Option<u32>,
    audio: Option<RawAudioSpecV1>,
}

fn parse_raw_meta_v1(meta_json: &[u8], op: &'static str) -> Result<RawMetaV1, SpearError> {
    let raw: RawMetaV1 = serde_json::from_slice(meta_json).map_err(|_| invalid_meta(op))?;
    let v = raw.v.ok_or_else(|| invalid_meta(op))?;
    if v != 1 {
        return Err(invalid_meta(op));
    }
    Ok(raw)
}

fn parse_kind(kind_s: String) -> MetaKindV1 {
    match kind_s.as_str() {
        "open" => MetaKindV1::Open,
        "utterance_begin" => MetaKindV1::UtteranceBegin,
        _ => MetaKindV1::Unknown(kind_s),
    }
}

fn parse_modality(modality_s: String) -> MetaModalityV1 {
    match modality_s.as_str() {
        "text" => MetaModalityV1::Text,
        "audio" => MetaModalityV1::Audio,
        _ => MetaModalityV1::Unknown(modality_s),
    }
}

fn parse_audio_spec(audio: RawAudioSpecV1, op: &'static str) -> Result<AudioSpecV1, SpearError> {
    let format = audio.format.ok_or_else(|| invalid_meta(op))?;
    let sample_rate_hz = audio.sample_rate_hz.ok_or_else(|| invalid_meta(op))?;
    let channels = audio.channels.ok_or_else(|| invalid_meta(op))?;
    Ok(AudioSpecV1 {
        format,
        sample_rate_hz,
        channels,
    })
}

pub fn parse_ctrl_meta_v1(meta_json: &[u8]) -> Result<CtrlMetaV1, SpearError> {
    let raw = parse_raw_meta_v1(meta_json, "ssf_ctrl_meta_v1_parse")?;
    let kind = raw
        .kind
        .map(parse_kind)
        .ok_or_else(|| invalid_meta("ssf_ctrl_meta_v1_parse"))?;
    let modality = raw
        .modality
        .map(parse_modality)
        .ok_or_else(|| invalid_meta("ssf_ctrl_meta_v1_parse"))?;

    match (kind, modality) {
        (MetaKindV1::Open, MetaModalityV1::Text) => Ok(CtrlMetaV1::OpenText {
            ts_ms: raw.ts_ms,
            trace_id: raw.trace_id,
        }),
        (MetaKindV1::Open, MetaModalityV1::Audio) => {
            let audio = raw
                .audio
                .ok_or_else(|| invalid_meta("ssf_ctrl_meta_v1_parse"))?;
            let audio = parse_audio_spec(audio, "ssf_ctrl_meta_v1_parse")?;
            Ok(CtrlMetaV1::OpenAudio {
                audio,
                ts_ms: raw.ts_ms,
                trace_id: raw.trace_id,
            })
        }
        (MetaKindV1::UtteranceBegin, MetaModalityV1::Audio) => {
            let utterance_id = raw
                .utterance_id
                .ok_or_else(|| invalid_meta("ssf_ctrl_meta_v1_parse"))?;
            Ok(CtrlMetaV1::UtteranceBeginAudio {
                utterance_id,
                ts_ms: raw.ts_ms,
                trace_id: raw.trace_id,
            })
        }
        _ => Err(invalid_meta("ssf_ctrl_meta_v1_parse")),
    }
}

pub fn parse_data_meta_v1(meta_json: &[u8]) -> Result<DataMetaV1, SpearError> {
    let value: serde_json::Value =
        serde_json::from_slice(meta_json).map_err(|_| invalid_meta("ssf_data_meta_v1_parse"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| invalid_meta("ssf_data_meta_v1_parse"))?;

    match obj.get("v") {
        Some(serde_json::Value::Number(n)) if n.as_u64() == Some(1) => {}
        _ => return Err(invalid_meta("ssf_data_meta_v1_parse")),
    }
    if obj.contains_key("kind") || obj.contains_key("modality") || obj.contains_key("audio") {
        return Err(invalid_meta("ssf_data_meta_v1_parse"));
    }

    let raw: RawMetaV1 =
        serde_json::from_value(value).map_err(|_| invalid_meta("ssf_data_meta_v1_parse"))?;
    Ok(DataMetaV1 {
        v: 1,
        ts_ms: raw.ts_ms,
        trace_id: raw.trace_id,
        message_id: raw.message_id,
        seq_in_message: raw.seq_in_message,
        utterance_id: raw.utterance_id,
        chunk_index: raw.chunk_index,
    })
}
