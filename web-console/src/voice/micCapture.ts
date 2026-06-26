// Microphone capture (browser).
// 麦克风采集（浏览器）。

import { encodePcm16le, mixToMono, resampleLinear } from './pcm16'
import type { MicChunk, VoiceConfig } from './types'

export type MicCapture = {
  start(): Promise<void>
  stop(): Promise<void>
  onChunk(cb: (c: MicChunk) => void): () => void
}

export function createMicCapture(config: VoiceConfig): MicCapture {
  return new WebAudioMicCapture(config)
}

class WebAudioMicCapture implements MicCapture {
  private config: VoiceConfig
  private subscribers = new Set<(c: MicChunk) => void>()

  private stream: MediaStream | null = null
  private ctx: AudioContext | null = null
  private source: MediaStreamAudioSourceNode | null = null
  private processor: ScriptProcessorNode | null = null

  private inputRateHz = 0
  private pending: Float32Array = new Float32Array(0)
  private started = false

  constructor(config: VoiceConfig) {
    this.config = config
  }

  onChunk(cb: (c: MicChunk) => void): () => void {
    this.subscribers.add(cb)
    return () => this.subscribers.delete(cb)
  }

  async start(): Promise<void> {
    if (this.started) return
    this.started = true
    this.pending = new Float32Array(0)

    const stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: this.config.channels,
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
      video: false,
    })
    this.stream = stream

    const ctx = await createAudioContextPrefer(this.config.sampleRateHz)
    this.ctx = ctx
    this.inputRateHz = ctx.sampleRate

    const source = ctx.createMediaStreamSource(stream)
    this.source = source

    const processor = ctx.createScriptProcessor(4096, source.channelCount ?? 1, 1)
    this.processor = processor

    processor.onaudioprocess = (ev: AudioProcessingEvent) => {
      const input = ev.inputBuffer
      const chans: Float32Array[] = []
      const n = input.numberOfChannels
      for (let i = 0; i < n; i++) {
        chans.push(input.getChannelData(i))
      }
      const mono = mixToMono(chans)
      this.handleMonoFrame(mono)
    }

    source.connect(processor)
    const sink = ctx.createGain()
    sink.gain.value = 0
    processor.connect(sink)
    sink.connect(ctx.destination)
  }

  async stop(): Promise<void> {
    if (!this.started) return
    this.started = false

    try {
      this.processor?.disconnect()
    } catch {
      // ignore
    }
    try {
      this.source?.disconnect()
    } catch {
      // ignore
    }

    const tracks = this.stream?.getTracks() ?? []
    for (const t of tracks) {
      try {
        t.stop()
      } catch {
        // ignore
      }
    }

    try {
      await this.ctx?.close()
    } catch {
      // ignore
    }

    this.stream = null
    this.ctx = null
    this.source = null
    this.processor = null
    this.pending = new Float32Array(0)
    this.inputRateHz = 0
  }

  private handleMonoFrame(frame: Float32Array): void {
    if (!this.started) return

    const combined = new Float32Array(this.pending.length + frame.length)
    combined.set(this.pending, 0)
    combined.set(frame, this.pending.length)
    this.pending = combined

    const samplesPerChunkIn = Math.max(
      1,
      Math.round((this.inputRateHz * this.config.chunkMs) / 1000),
    )
    while (this.pending.length >= samplesPerChunkIn) {
      const slice = this.pending.slice(0, samplesPerChunkIn)
      this.pending = this.pending.slice(samplesPerChunkIn)

      const f32 =
        this.inputRateHz === this.config.sampleRateHz
          ? slice
          : resampleLinear({
              input: slice,
              inputRate: this.inputRateHz,
              outputRate: this.config.sampleRateHz,
            })
      const pcm16le = encodePcm16le(f32)
      const chunk: MicChunk = { tsMs: Date.now(), pcm16le }
      for (const cb of this.subscribers) {
        try {
          cb(chunk)
        } catch {
          // ignore
        }
      }
    }
  }
}

async function createAudioContextPrefer(sampleRateHz: number): Promise<AudioContext> {
  const AudioCtx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext
  try {
    return new AudioCtx({ sampleRate: sampleRateHz })
  } catch {
    return new AudioCtx()
  }
}
