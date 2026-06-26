import * as ssf from "spear/ssf";

class ChatCompletionResponse {
  constructor(rawJson) {
    this._rawJson = rawJson;
  }

  json() {
    return JSON.parse(this._rawJson);
  }

  text() {
    try {
      const j = this.json();
      const c = j?.choices?.[0]?.message?.content;
      if (typeof c === "string") return c;
    } catch (_) {}
    return this._rawJson;
  }

  raw() {
    return ssf.encodeUtf8(this._rawJson);
  }
}

if (typeof globalThis.console !== "object" || globalThis.console === null) {
  globalThis.console = {};
}
if (typeof globalThis.console.log !== "function") {
  globalThis.console.log = (...args) =>
    __spear_print(args.map((x) => String(x)).join(" "));
}

export const Spear = {
  sleepMs: (ms) => __spear_sleep_ms(Number(ms) | 0),
  chat: {
    completions: {
      create: async (options) => {
        const raw = __spear_cchat_completion(JSON.stringify(options ?? {}));
        return new ChatCompletionResponse(raw);
      },
    },
  },
  userStream: {
    Direction: {
      INBOUND: 1,
      OUTBOUND: 2,
      BIDIRECTIONAL: 3,
    },
    open: (streamId, direction) => {
      const sid = Number(streamId) | 0;
      const dir = direction == null ? 3 : Number(direction) | 0;
      const fd = __spear_user_stream_open(sid, dir);
      let seqLo = 1 >>> 0;
      let seqHi = 0 >>> 0;
      const write = (data) => {
        const u8 = ssf.encodeUtf8(data);
        __spear_user_stream_write(fd, u8);
      };
      const read = () => {
        const u8 = __spear_user_stream_read(fd);
        if (u8 == null) return null;
        return u8;
      };
      const close = () => __spear_user_stream_close(fd);
      const streamIdU32 = sid >>> 0;
      const nextSeq = () => {
        const outLo = seqLo;
        const outHi = seqHi;
        seqLo = (seqLo + 1) >>> 0;
        if (seqLo === 0) seqHi = (seqHi + 1) >>> 0;
        return { seqLo: outLo, seqHi: outHi };
      };
      const sendFrame = (msgType, meta, data) => {
        const seq = nextSeq();
        const frame = ssf.buildV1Frame({
          streamId: streamIdU32,
          msgType,
          meta: meta instanceof Uint8Array ? meta : new Uint8Array(0),
          data: data instanceof Uint8Array ? data : ssf.encodeUtf8(data),
          flags: 0,
          seqLo: seq.seqLo,
          seqHi: seq.seqHi,
        });
        write(frame);
      };
      const normalizeMeta = (meta) => (meta instanceof Uint8Array ? meta : new Uint8Array(0));
      const sendText = (text, meta) =>
        sendFrame(ssf.MsgType.DATA, normalizeMeta(meta), ssf.encodeUtf8(String(text)));
      const sendData = (data, meta) => sendFrame(ssf.MsgType.DATA, normalizeMeta(meta), data);
      const sendCommit = (meta) =>
        sendFrame(ssf.MsgType.COMMIT, normalizeMeta(meta), new Uint8Array(0));
      const sendCtrl = (meta, data) =>
        sendFrame(
          ssf.MsgType.CTRL,
          normalizeMeta(meta),
          data instanceof Uint8Array ? data : new Uint8Array(0),
        );
      const readMessage = () => {
        const frame = read();
        if (!frame) return null;
        const parsed = ssf.parseV1Frame(frame);
        if (!parsed) return null;
        if ((parsed.streamId >>> 0) !== streamIdU32) return null;
        const t = parsed.msgType >>> 0;
        let message;
        if (t === (ssf.MsgType.DATA >>> 0)) {
          message = {
            kind: "data",
            data: parsed.data,
            meta: parsed.meta,
            get text() {
              return ssf.decodeUtf8(parsed.data);
            },
          };
        } else if (t === (ssf.MsgType.COMMIT >>> 0)) {
          message = { kind: "commit", meta: parsed.meta };
        } else if (t === (ssf.MsgType.CTRL >>> 0)) {
          message = { kind: "ctrl", meta: parsed.meta, data: parsed.data };
        } else {
          message = { kind: "frame", msgType: parsed.msgType >>> 0, meta: parsed.meta, data: parsed.data };
        }
        return message;
      };
      return {
        fd,
        streamId: streamIdU32,
        read,
        write,
        sendFrame,
        readMessage,
        sendCtrl,
        sendData,
        sendCommit,
        sendText,
        close,
      };
    },
    ctlOpen: () => {
      const fd = __spear_user_stream_ctl_open();
      const readEvent = () => __spear_user_stream_ctl_read_event(fd);
      const close = () => __spear_user_stream_close(fd);
      return { fd, readEvent, close };
    },
    ssf: {
      MsgType: ssf.MsgType,
      buildV1Frame: ssf.buildV1Frame,
      parseV1Frame: ssf.parseV1Frame,
      encodeUtf8: ssf.encodeUtf8,
      decodeUtf8: ssf.decodeUtf8,
    },
  },
  tool: (spec) => {
    const name = spec?.name;
    const description = spec?.description;
    const parameters = spec?.parameters;
    const handler = spec?.handler;

    if (typeof name !== "string" || name.length === 0) {
      throw new Error("invalid tool name");
    }
    if (typeof handler !== "function") {
      throw new Error("invalid tool handler");
    }

    const fnObj = {
      type: "function",
      function: {
        name,
        description: typeof description === "string" ? description : "",
        parameters: parameters ?? { type: "object", properties: {} },
      },
    };

    const fnJson = JSON.stringify(fnObj);
    const wrapper = (argsJson) => handler(JSON.parse(argsJson));
    return __spear_tool_register(fnJson, wrapper);
  },
};
