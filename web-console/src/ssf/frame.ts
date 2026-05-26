// SSF v1 frame codec (browser).
// SSF v1 帧编解码（浏览器）。

import { SSF_HEADER_LEN_V1, SSF_MAGIC, SSF_VERSION_V1 } from './constants'

export type SsfV1Frame = {
  version: number
  headerLen: number
  msgType: number
  flags: number
  streamId: number
  seq: bigint
  meta: Uint8Array
  data: Uint8Array
}

export function encodeSsfV1Frame(params: {
  streamId: number
  msgType: number
  seq: bigint
  flags?: number
  meta?: Uint8Array
  data?: Uint8Array
}): Uint8Array {
  const headerLen = SSF_HEADER_LEN_V1
  const meta = params.meta ?? new Uint8Array()
  const data = params.data ?? new Uint8Array()

  const out = new Uint8Array(headerLen + meta.length + data.length)
  const dv = new DataView(out.buffer, out.byteOffset, out.byteLength)

  out.set(SSF_MAGIC, 0)
  dv.setUint16(4, SSF_VERSION_V1, true)
  dv.setUint16(6, headerLen, true)
  dv.setUint16(8, params.msgType, true)
  dv.setUint16(10, params.flags ?? 0, true)
  dv.setUint32(12, params.streamId >>> 0, true)
  dv.setBigUint64(16, params.seq, true)
  dv.setUint32(24, meta.length >>> 0, true)
  dv.setUint32(28, data.length >>> 0, true)

  out.set(meta, headerLen)
  out.set(data, headerLen + meta.length)
  return out
}

export function decodeSsfV1Frame(buf: ArrayBuffer | Uint8Array): SsfV1Frame {
  const u8 = buf instanceof Uint8Array ? buf : new Uint8Array(buf)
  if (u8.length < SSF_HEADER_LEN_V1) throw new Error('SSF frame too short')
  if (u8[0] !== SSF_MAGIC[0] || u8[1] !== SSF_MAGIC[1] || u8[2] !== SSF_MAGIC[2] || u8[3] !== SSF_MAGIC[3]) {
    throw new Error('invalid SSF magic')
  }

  const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength)
  const version = dv.getUint16(4, true)
  const headerLen = dv.getUint16(6, true)
  const msgType = dv.getUint16(8, true)
  const flags = dv.getUint16(10, true)
  const streamId = dv.getUint32(12, true)
  const seq = dv.getBigUint64(16, true)
  const metaLen = dv.getUint32(24, true)
  const dataLen = dv.getUint32(28, true)

  const metaStart = headerLen
  const dataStart = headerLen + metaLen
  const end = dataStart + dataLen
  if (end > u8.length) throw new Error('SSF frame truncated')

  return {
    version,
    headerLen,
    msgType,
    flags,
    streamId,
    seq,
    meta: u8.slice(metaStart, metaStart + metaLen),
    data: u8.slice(dataStart, end),
  }
}

