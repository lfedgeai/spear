// Spear user stream client (browser).
// Spear 用户流客户端（浏览器）。

import { decodeSsfV1Frame, encodeSsfV1Frame, SsfMsgType } from './ssf'

export type StreamSession = {
  execution_id: string
  token: string
  ws_url: string
  expires_in_ms: number
}

export type SpearStreamClientCallbacks = {
  onOpen?: () => void
  onClose?: (ev: CloseEvent) => void
  onError?: (ev: Event) => void
  onFrame?: (frame: { streamId: number; msgType: number; data: Uint8Array; meta: Uint8Array }) => void
}

export class SpearStreamClient {
  private ws: WebSocket | null = null
  private seq: bigint = 1n
  private callbacks: SpearStreamClientCallbacks

  constructor(callbacks: SpearStreamClientCallbacks) {
    this.callbacks = callbacks
  }

  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN
  }

  disconnect(): void {
    const ws = this.ws
    this.ws = null
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
      ws.close()
    }
  }

  async connect(wsUrl: string, options?: { subprotocol?: string; autoOpenStreamId?: number }): Promise<void> {
    this.disconnect()
    const ws = options?.subprotocol ? new WebSocket(wsUrl, options.subprotocol) : new WebSocket(wsUrl)
    ws.binaryType = 'arraybuffer'
    this.ws = ws

    ws.onopen = () => {
      this.callbacks.onOpen?.()
      const sid = options?.autoOpenStreamId
      if (typeof sid === 'number' && Number.isFinite(sid) && sid > 0) {
        try {
          this.openStream(sid)
        } catch {
          // ignore
        }
      }
    }
    ws.onclose = (ev) => this.callbacks.onClose?.(ev)
    ws.onerror = (ev) => this.callbacks.onError?.(ev)
    ws.onmessage = async (ev) => {
      const data = await toUint8Array(ev.data)
      try {
        const frame = decodeSsfV1Frame(data)
        this.callbacks.onFrame?.({
          streamId: frame.streamId,
          msgType: frame.msgType,
          meta: frame.meta,
          data: frame.data,
        })
      } catch {
        this.callbacks.onFrame?.({
          streamId: 0,
          msgType: 0,
          meta: new Uint8Array(),
          data,
        })
      }
    }
  }

  openStream(streamId: number): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) throw new Error('ws not connected')
    const meta = new TextEncoder().encode('{}')
    const payload = encodeSsfV1Frame({
      streamId,
      msgType: SsfMsgType.CTRL,
      seq: this.seq++,
      meta,
      data: new Uint8Array(),
    })
    this.ws.send(payload)
  }

  sendText(streamId: number, text: string): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) throw new Error('ws not connected')
    const meta = new TextEncoder().encode('{}')
    const data = new TextEncoder().encode(text)
    const payload = encodeSsfV1Frame({
      streamId,
      msgType: SsfMsgType.DATA,
      seq: this.seq++,
      meta,
      data,
    })
    this.ws.send(payload)
  }

  sendCommit(streamId: number): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) throw new Error('ws not connected')
    const meta = new TextEncoder().encode('{}')
    const payload = encodeSsfV1Frame({
      streamId,
      msgType: SsfMsgType.COMMIT,
      seq: this.seq++,
      meta,
      data: new Uint8Array(),
    })
    this.ws.send(payload)
  }
}

export async function createStreamSession(params: {
  executionId: string
}): Promise<StreamSession> {
  const url = new URL(
    `/api/v1/executions/${encodeURIComponent(params.executionId)}/streams/session`,
    window.location.origin,
  )
  const resp = await fetch(url, { method: 'POST' })
  if (!resp.ok) {
    const text = await resp.text().catch(() => '')
    throw new Error(`create_stream_session failed: ${resp.status} ${text}`)
  }
  return (await resp.json()) as StreamSession
}

async function toUint8Array(data: unknown): Promise<Uint8Array> {
  if (data instanceof ArrayBuffer) return new Uint8Array(data)
  if (data instanceof Uint8Array) return data
  if (data instanceof Blob) return new Uint8Array(await data.arrayBuffer())
  if (typeof data === 'string') return new TextEncoder().encode(data)
  return new Uint8Array()
}
