// PCM16 utilities (browser).
// PCM16 工具（浏览器）。

export function encodePcm16le(samples: Float32Array): Uint8Array {
  const out = new Uint8Array(samples.length * 2)
  const dv = new DataView(out.buffer, out.byteOffset, out.byteLength)
  for (let i = 0; i < samples.length; i++) {
    const s = clamp(samples[i] ?? 0, -1, 1)
    const v = s < 0 ? Math.round(s * 32768) : Math.round(s * 32767)
    dv.setInt16(i * 2, v, true)
  }
  return out
}

export function resampleLinear(params: {
  input: Float32Array
  inputRate: number
  outputRate: number
}): Float32Array {
  if (params.inputRate <= 0 || params.outputRate <= 0) return new Float32Array()
  if (params.inputRate === params.outputRate) return params.input
  const ratio = params.outputRate / params.inputRate
  const outLen = Math.max(0, Math.floor(params.input.length * ratio))
  if (outLen === 0) return new Float32Array()

  const out = new Float32Array(outLen)
  for (let i = 0; i < outLen; i++) {
    const t = i / ratio
    const idx = Math.floor(t)
    const frac = t - idx
    const a = params.input[idx] ?? 0
    const b = params.input[Math.min(idx + 1, params.input.length - 1)] ?? 0
    out[i] = a + (b - a) * frac
  }
  return out
}

export function mixToMono(input: Float32Array[]): Float32Array {
  if (input.length === 0) return new Float32Array()
  if (input.length === 1) return input[0]
  const n = input[0]?.length ?? 0
  const out = new Float32Array(n)
  for (let i = 0; i < n; i++) {
    let acc = 0
    for (let c = 0; c < input.length; c++) {
      acc += input[c]?.[i] ?? 0
    }
    out[i] = acc / input.length
  }
  return out
}

function clamp(x: number, lo: number, hi: number): number {
  if (x < lo) return lo
  if (x > hi) return hi
  return x
}

