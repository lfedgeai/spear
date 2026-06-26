// Voice input types (browser).
// 语音输入类型定义（浏览器）。

export type VoiceConfig = {
  sampleRateHz: number
  channels: number
  chunkMs: number
}

export type MetaCommonV1 = {
  v: 1
  ts_ms?: number
  trace_id?: string
}

export type TextOpenMetaV1 = MetaCommonV1 & {
  kind: 'open'
  modality: 'text'
  encoding: 'utf-8'
}

export type TextDataMetaV1 = MetaCommonV1 & {
  message_id: string
  seq_in_message: number
}

export type TextCommitMetaV1 = MetaCommonV1 & {
  message_id: string
}

export type AudioOpenMetaV1 = MetaCommonV1 & {
  kind: 'open'
  modality: 'audio'
  audio: {
    format: 'pcm_s16le'
    sample_rate_hz: number
    channels: number
  }
}

export type AudioUtteranceBeginMetaV1 = MetaCommonV1 & {
  kind: 'utterance_begin'
  modality: 'audio'
  utterance_id: string
}

export type AudioDataMetaV1 = MetaCommonV1 & {
  utterance_id: string
  chunk_index: number
}

export type AudioCommitMetaV1 = MetaCommonV1 & {
  utterance_id: string
}

export type MicChunk = {
  tsMs: number
  pcm16le: Uint8Array
}
