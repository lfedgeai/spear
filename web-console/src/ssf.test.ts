// SSF v1 tests.
// SSF v1 测试。

import { describe, expect, it } from 'vitest'
import { decodeSsfV1Frame, encodeSsfV1Frame, SsfMsgType } from './ssf'

describe('SSF v1', () => {
  it('encodes and decodes a frame', () => {
    const meta = new TextEncoder().encode('{"k":"v"}')
    const data = new TextEncoder().encode('hello')

    const bytes = encodeSsfV1Frame({
      streamId: 1,
      msgType: SsfMsgType.DATA,
      seq: 123n,
      meta,
      data,
    })

    const frame = decodeSsfV1Frame(bytes)
    expect(frame.version).toBe(1)
    expect(frame.streamId).toBe(1)
    expect(frame.msgType).toBe(SsfMsgType.DATA)
    expect(frame.seq).toBe(123n)
    expect(new TextDecoder().decode(frame.meta)).toBe('{"k":"v"}')
    expect(new TextDecoder().decode(frame.data)).toBe('hello')
  })
})
