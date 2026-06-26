// Voice stream sender over SSF (browser).
// 基于 SSF 的语音流发送器（浏览器）。

import type { SpearStreamClient } from '@/spearStream'
import type {
  AudioCommitMetaV1,
  AudioDataMetaV1,
  AudioOpenMetaV1,
  AudioUtteranceBeginMetaV1,
  MicChunk,
  VoiceConfig,
} from './types'

export type VoiceSession = {
  utteranceId: string
  chunkIndex: number
}

export type VoiceStreamSender = {
  ensureStreamOpen(): void
  beginUtterance(nowMs: number, utteranceId: string): VoiceSession
  sendChunk(session: VoiceSession, chunk: MicChunk): void
  commitUtterance(session: VoiceSession, nowMs: number): void
}

export function createVoiceStreamSender(params: {
  client: SpearStreamClient
  streamId: number
  config: VoiceConfig
  assumeStreamOpened?: boolean
}): VoiceStreamSender {
  return new SsfVoiceStreamSender(params.client, params.streamId, params.config, params.assumeStreamOpened ?? false)
}

class SsfVoiceStreamSender implements VoiceStreamSender {
  private client: SpearStreamClient
  private streamId: number
  private config: VoiceConfig
  private opened = false

  constructor(client: SpearStreamClient, streamId: number, config: VoiceConfig, assumeOpened: boolean) {
    this.client = client
    this.streamId = streamId
    this.config = config
    this.opened = assumeOpened
  }

  ensureStreamOpen(): void {
    if (this.opened) return
    const meta: AudioOpenMetaV1 = {
      v: 1,
      kind: 'open',
      modality: 'audio',
      audio: {
        format: 'pcm_s16le',
        sample_rate_hz: this.config.sampleRateHz,
        channels: this.config.channels,
      },
      ts_ms: Date.now(),
    }
    this.client.openStream(this.streamId, meta)
    this.opened = true
  }

  beginUtterance(nowMs: number, utteranceId: string): VoiceSession {
    this.ensureStreamOpen()
    const meta: AudioUtteranceBeginMetaV1 = {
      v: 1,
      kind: 'utterance_begin',
      modality: 'audio',
      utterance_id: utteranceId,
      ts_ms: nowMs,
    }
    this.client.openStream(this.streamId, meta)
    return { utteranceId, chunkIndex: 0 }
  }

  sendChunk(session: VoiceSession, chunk: MicChunk): void {
    const idx = session.chunkIndex + 1
    session.chunkIndex = idx
    const meta: AudioDataMetaV1 = {
      v: 1,
      utterance_id: session.utteranceId,
      chunk_index: idx,
      ts_ms: chunk.tsMs,
    }
    this.client.sendBinary(this.streamId, chunk.pcm16le, meta)
  }

  commitUtterance(session: VoiceSession, nowMs: number): void {
    const meta: AudioCommitMetaV1 = {
      v: 1,
      utterance_id: session.utteranceId,
      ts_ms: nowMs,
    }
    this.client.sendCommit(this.streamId, meta)
  }
}
