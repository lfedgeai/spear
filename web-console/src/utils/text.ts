export function decodeAsUtf8(data: Uint8Array): string {
  try {
    return new TextDecoder().decode(data)
  } catch {
    return ''
  }
}

export function formatFramePrefix(msgType: number, metaText: string): string {
  if (msgType === 2 && isDefaultTextMeta(metaText)) return ''
  if (!metaText && !msgType) return ''
  if (!metaText) return `[type=${msgType}] `
  return `[type=${msgType} meta=${metaText}] `
}

function isDefaultTextMeta(metaText: string): boolean {
  const trimmed = metaText.trim()
  if (!trimmed) return false
  try {
    const parsed = JSON.parse(trimmed)
    return !!parsed && typeof parsed === 'object' && !Array.isArray(parsed) && Object.keys(parsed).length === 1 && parsed.v === 1
  } catch {
    return trimmed === '{"v":1}'
  }
}

type TextPart = { type: 'text'; text: string }
type CodePart = { type: 'code'; code: string; lang?: string }

export function splitCodeBlocks(input: string): Array<TextPart | CodePart> {
  const out: Array<TextPart | CodePart> = []
  const re = /```([a-zA-Z0-9_-]+)?\n([\s\S]*?)```/g
  let last = 0
  for (;;) {
    const m = re.exec(input)
    if (!m) break
    const idx = m.index
    if (idx > last) {
      out.push({ type: 'text', text: input.slice(last, idx) })
    }
    out.push({ type: 'code', lang: m[1], code: m[2] })
    last = idx + m[0].length
  }
  if (last < input.length) {
    out.push({ type: 'text', text: input.slice(last) })
  }
  if (!out.length) out.push({ type: 'text', text: input })
  return out
}

export function jsonPretty(v: unknown): string {
  if (v == null) return 'null'
  try {
    return JSON.stringify(v, null, 2)
  } catch {
    return String(v)
  }
}
