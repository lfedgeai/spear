// SSF (Spear Stream Frame) v1 helpers for Boa runtime.
// Boa 运行时下的 SSF（Spear Stream Frame）v1 辅助模块。

export const MsgType = {
  CTRL: 1,
  DATA: 2,
  COMMIT: 3,
  CLOSE: 4,
  ERROR: 6,
};

export function encodeUtf8(data) {
  if (data instanceof Uint8Array) return data;
  const s = typeof data === "string" ? data : String(data);
  return __spear_utf8_encode(s);
}

export function decodeUtf8(u8) {
  if (u8 instanceof Uint8Array) return String(__spear_utf8_decode(u8) ?? "");
  return String(u8 ?? "");
}

export function buildV1Frame(params) {
  const sid = Number(params?.streamId) >>> 0;
  const mt = Number(params?.msgType) & 0xffff;
  const flags = Number(params?.flags ?? 0) & 0xffff;
  const seqLo = Number(params?.seqLo ?? 1) >>> 0;
  const seqHi = Number(params?.seqHi ?? 0) >>> 0;
  const metaU8 = params?.meta instanceof Uint8Array ? params.meta : new Uint8Array(0);
  const dataU8 = params?.data instanceof Uint8Array ? params.data : encodeUtf8(params?.data);
  const bin = __spear_ssf_build_v1(
    sid,
    mt,
    flags,
    seqLo,
    seqHi,
    metaU8,
    dataU8,
  );
  return bin ?? new Uint8Array(0);
}

export function parseV1Frame(frame) {
  if (!(frame instanceof Uint8Array)) return null;
  const parsed = __spear_ssf_parse_v1(frame);
  if (parsed == null) return null;
  const streamId = Number(parsed.streamId) >>> 0;
  const msgType = Number(parsed.msgType) >>> 0;
  const flags = Number(parsed.flags) >>> 0;
  const seqLo = Number(parsed.seqLo) >>> 0;
  const seqHi = Number(parsed.seqHi) >>> 0;
  const meta = parsed.metaBin instanceof Uint8Array ? parsed.metaBin : new Uint8Array(0);
  const data = parsed.dataBin instanceof Uint8Array ? parsed.dataBin : new Uint8Array(0);
  return { streamId, msgType, seqLo, seqHi, flags, meta, data };
}

// Parse SSF meta JSON (v1).
// 解析 SSF meta JSON（v1）。
export function parseMetaV1(metaU8) {
  const s = decodeUtf8(metaU8 instanceof Uint8Array ? metaU8 : encodeUtf8(metaU8));
  try {
    const obj = JSON.parse(s);
    if (!obj || typeof obj !== "object") return null;
    if (obj.v !== 1) return null;
    return obj;
  } catch (_) {
    return null;
  }
}

// Parse CTRL meta into a normalized shape.
// 将 CTRL meta 解析为归一化结构。
export function parseCtrlMetaV1(metaU8) {
  const m = parseMetaV1(metaU8);
  if (!m) return null;
  const kind = String(m.kind ?? "");
  const modality = String(m.modality ?? "");
  if (kind === "open" && modality === "text") {
    return { kind: "open", modality: "text", tsMs: m.ts_ms, traceId: m.trace_id };
  }
  if (kind === "open" && modality === "audio") {
    return { kind: "open", modality: "audio", audio: m.audio ?? null, tsMs: m.ts_ms, traceId: m.trace_id };
  }
  if (kind === "utterance_begin" && modality === "audio") {
    return { kind: "utterance_begin", modality: "audio", utteranceId: m.utterance_id, tsMs: m.ts_ms, traceId: m.trace_id };
  }
  return null;
}

// Parse DATA/COMMIT meta in strict mode.
// 严格模式解析 DATA/COMMIT meta。
//
// Strict rules / 严格规则：
// - `v` must be 1 / `v` 必须为 1
// - `kind/modality/audio` must NOT appear / 禁止出现 `kind/modality/audio`
export function parseDataMetaV1(metaU8) {
  const m = parseMetaV1(metaU8);
  if (!m) return null;
  if ("kind" in m) return null;
  if ("modality" in m) return null;
  if ("audio" in m) return null;
  return m;
}
