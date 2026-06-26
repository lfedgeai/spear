import { Spear } from "spear";
import * as ssf from "spear/ssf";

export const ControlEventKind = {
  STREAM_CONNECTED: 1,
  SESSION_CLOSED: 2,
};

export function buildMetaV1(fields = {}) {
  return ssf.encodeUtf8(JSON.stringify({ v: 1, ...fields }));
}

export function parseOpenControl(message, modality) {
  if (!message || message.kind !== "ctrl") return null;
  const ctrl = ssf.parseCtrlMetaV1(message.meta);
  if (!ctrl || ctrl.kind !== "open") return null;
  if (ctrl.modality !== modality) return null;
  return ctrl;
}

export function parseStrictPayloadMeta(message) {
  if (!message) return null;
  return ssf.parseDataMetaV1(message.meta);
}

// Tracks one known user stream across ctl events and the mandatory CTRL(open) handshake.
export class ManagedUserStream {
  constructor({ streamId, modality, direction = Spear.userStream.Direction.BIDIRECTIONAL }) {
    this.streamId = Number(streamId) >>> 0;
    this.modality = String(modality);
    this.direction = direction;
    this.stream = null;
    this.opened = false;
  }

  onControlEvent(evt) {
    if (!evt || evt.kind !== ControlEventKind.STREAM_CONNECTED) return false;
    if ((Number(evt.streamId) >>> 0) !== this.streamId) return false;
    if (this.stream) return false;
    this.stream = Spear.userStream.open(this.streamId, this.direction);
    this.opened = false;
    return true;
  }

  readMessage() {
    if (!this.stream) return null;
    return this.stream.readMessage();
  }

  acceptOpenHandshake(message) {
    if (this.opened) return false;
    if (!parseOpenControl(message, this.modality)) return false;
    this.opened = true;
    return true;
  }

  isReady() {
    return !!this.stream && this.opened;
  }

  sendTextV1(text, meta = {}) {
    if (!this.stream) throw new Error(`stream ${this.streamId} not connected`);
    this.stream.sendText(String(text), buildMetaV1(meta));
  }

  sendDataV1(data, meta = {}) {
    if (!this.stream) throw new Error(`stream ${this.streamId} not connected`);
    this.stream.sendData(data, buildMetaV1(meta));
  }

  sendCommitV1(meta = {}) {
    if (!this.stream) throw new Error(`stream ${this.streamId} not connected`);
    this.stream.sendCommit(buildMetaV1(meta));
  }

  close() {
    try {
      this.stream?.close();
    } finally {
      this.stream = null;
      this.opened = false;
    }
  }
}

// Buffer text output until the text stream handshake completes.
// This keeps sample code simple while preserving protocol correctness.
export class BufferedTextOutput {
  constructor(targetStream) {
    this.targetStream = targetStream;
    this.queue = [];
  }

  write(text, meta = {}) {
    this.queue.push({ kind: "text", text: String(text), meta });
    this.flush();
  }

  commit(meta = {}) {
    this.queue.push({ kind: "commit", meta });
    this.flush();
  }

  flush() {
    if (!this.targetStream?.isReady()) return false;
    while (this.queue.length > 0) {
      const item = this.queue.shift();
      if (item.kind === "text") {
        this.targetStream.sendTextV1(item.text, item.meta);
      } else {
        this.targetStream.sendCommitV1(item.meta);
      }
    }
    return true;
  }
}
