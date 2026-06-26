import * as ssf from "spear/ssf";

function normalizeAudioChunk(data) {
  if (data instanceof Uint8Array) return data;
  return ssf.encodeUtf8(data);
}

// A thin, reusable RTASR wrapper for Boa-based WASM samples.
// It keeps hostcall details out of sample code and exposes typed, intention-revealing methods.
export class RtasrSession {
  static create() {
    return new RtasrSession(__spear_rtasr_create());
  }

  constructor(fd) {
    this.fd = Number(fd) | 0;
  }

  setParamJson(value) {
    const json = typeof value === "string" ? value : JSON.stringify(value ?? {});
    __spear_rtasr_set_param_json(this.fd, json);
    return this;
  }

  setParamString(key, value) {
    __spear_rtasr_set_param_string(this.fd, String(key), String(value));
    return this;
  }

  setAutoflush(value) {
    const json = typeof value === "string" ? value : JSON.stringify(value ?? {});
    __spear_rtasr_set_autoflush_json(this.fd, json);
    return this;
  }

  connect() {
    __spear_rtasr_connect(this.fd);
    return this;
  }

  flush() {
    __spear_rtasr_flush(this.fd);
    return this;
  }

  clear() {
    __spear_rtasr_clear(this.fd);
    return this;
  }

  writeAudio(data) {
    const u8 = normalizeAudioChunk(data);
    __spear_rtasr_write(this.fd, u8);
    return this;
  }

  readBytes() {
    return __spear_rtasr_read(this.fd);
  }

  readText() {
    const bytes = this.readBytes();
    if (bytes == null) return null;
    return ssf.decodeUtf8(bytes);
  }

  readJson() {
    const text = this.readText();
    if (text == null) return null;
    try {
      return JSON.parse(text);
    } catch (_) {
      return { type: "invalid_json", raw: text };
    }
  }

  close() {
    __spear_rtasr_close(this.fd);
  }
}
