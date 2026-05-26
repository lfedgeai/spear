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
  const bin = __spear_utf8_encode(s);
  return bin_to_u8(String(bin ?? ""));
}

export function decodeUtf8(u8) {
  if (u8 instanceof Uint8Array) return String(__spear_utf8_decode(u8_to_bin(u8)) ?? "");
  return String(u8 ?? "");
}

function u8_to_bin(u8) {
  let s = "";
  for (let i = 0; i < u8.length; i++) s += String.fromCharCode(u8[i] & 255);
  return s;
}

function bin_to_u8(bin) {
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i) & 255;
  return out;
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
    u8_to_bin(metaU8),
    u8_to_bin(dataU8),
  );
  return bin_to_u8(String(bin ?? ""));
}

export function parseV1Frame(frame) {
  if (!(frame instanceof Uint8Array)) return null;
  const parsed = __spear_ssf_parse_v1(u8_to_bin(frame));
  if (parsed == null) return null;
  const streamId = Number(parsed.streamId) >>> 0;
  const msgType = Number(parsed.msgType) >>> 0;
  const flags = Number(parsed.flags) >>> 0;
  const seqLo = Number(parsed.seqLo) >>> 0;
  const seqHi = Number(parsed.seqHi) >>> 0;
  const meta = bin_to_u8(String(parsed.metaBin ?? ""));
  const data = bin_to_u8(String(parsed.dataBin ?? ""));
  return { streamId, msgType, seqLo, seqHi, flags, meta, data };
}
