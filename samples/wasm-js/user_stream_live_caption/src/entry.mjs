// Live caption sample (JS):
// - opens the text + voice user streams announced by the runtime
// - forwards PCM voice DATA into RTASR after `utterance_begin`
// - converts RTASR partial/final events into one incremental text transcript
// - emits text DATA + COMMIT back to stream_id=1 with strict v1 meta

import { Spear } from "spear";
import { RtasrSession } from "spear/rtasr";
import {
  BufferedTextOutput,
  ControlEventKind,
  ManagedUserStream,
  parseStrictPayloadMeta,
} from "spear/user_stream_protocol";
import {
  isSpeechStartedEvent,
  isTranscriptCompletedEvent,
  isTranscriptDeltaEvent,
} from "spear/rtasr_event";
import { decodeMessageMetaText, requireOpenedStream, syncStreamsFromControlEvent } from "spear/stream_gate";
import { TextOutputWriter } from "app/lib/text_output";
import { TranscriptWriter } from "app/lib/transcript_writer";

const STREAM_TEXT = 1;
const STREAM_VOICE = 2;
const LIVE_CAPTION_AUDIO_BYTES_PER_MS = 32; // 16kHz * mono * s16le
// Keep VAD responsive without fragmenting common short pauses.
// 保持 VAD 响应速度，同时避免把常见的短停顿切得过碎。
const LIVE_CAPTION_AUTOFLUSH_MS = 600;
const LIVE_CAPTION_ENABLE_PERIODIC_FLUSH = false;
const LIVE_CAPTION_PERIODIC_FLUSH_MS = 3000;
const LIVE_CAPTION_MIN_COMMIT_AUDIO_BYTES = 100 * LIVE_CAPTION_AUDIO_BYTES_PER_MS;
const LIVE_CAPTION_AGGREGATE_WRITE_BYTES = 240 * LIVE_CAPTION_AUDIO_BYTES_PER_MS;
const LIVE_CAPTION_TEXT_BATCH_SIZE = 8;
const LIVE_CAPTION_RTASR_DRAIN_INTERVAL = 1;

// This sample keeps state in one small coordinator object so the top-level loop can stay linear.
// The class owns:
// - the user-stream ctl plane
// - one text stream and one voice stream
// - one optional RTASR session
// - transcript buffering / protocol warnings
class LiveCaptionApp {
  constructor() {
    this.ctl = Spear.userStream.ctlOpen();
    this.textStream = new ManagedUserStream({ streamId: STREAM_TEXT, modality: "text" });
    this.voiceStream = new ManagedUserStream({ streamId: STREAM_VOICE, modality: "audio" });
    this.textOut = new BufferedTextOutput(this.textStream);
    this.textWriter = new TextOutputWriter(this.textOut);
    this.transcriptWriter = new TranscriptWriter(this.textWriter);
    this.rtasr = null;
    this.protocolWarnings = new Set();
    this.lastRtasrFlushAt = 0;
    this.audioBytesSinceFlush = 0;
    this.pendingAudioBytes = 0;
    this.pendingAudioChunks = [];
  }

  ensureRtasr() {
    if (this.rtasr) return this.rtasr;
    const session = RtasrSession.create();
    session.setAutoflush({
      strategy: "server_vad",
      vad: { silence_ms: LIVE_CAPTION_AUTOFLUSH_MS },
    });
    session.setParamString("transport", "websocket");
    session.connect();
    this.rtasr = session;
    this.lastRtasrFlushAt = Date.now();
    this.resetPendingAudio();
    return session;
  }

  resetPendingAudio() {
    this.audioBytesSinceFlush = 0;
    this.pendingAudioBytes = 0;
    this.pendingAudioChunks = [];
  }

  appendPendingAudio(chunk) {
    if (!(chunk instanceof Uint8Array) || chunk.length === 0) return false;
    this.pendingAudioChunks.push(chunk);
    this.pendingAudioBytes += chunk.length;
    return true;
  }

  writePendingAudio() {
    if (!this.rtasr || this.pendingAudioBytes === 0) return false;

    const merged = new Uint8Array(this.pendingAudioBytes);
    let offset = 0;
    for (const chunk of this.pendingAudioChunks) {
      merged.set(chunk, offset);
      offset += chunk.length;
    }

    this.rtasr.writeAudio(merged);
    this.audioBytesSinceFlush += merged.length;

    this.pendingAudioBytes = 0;
    this.pendingAudioChunks = [];
    return true;
  }

  flushRtasr() {
    if (!this.rtasr) return false;
    const wrotePending = this.writePendingAudio();
    if (this.audioBytesSinceFlush < LIVE_CAPTION_MIN_COMMIT_AUDIO_BYTES) {
      return wrotePending;
    }
    this.rtasr.flush();
    this.lastRtasrFlushAt = Date.now();
    this.audioBytesSinceFlush = 0;
    return true;
  }

  maybeFlushRtasr() {
    if (!this.rtasr) return false;
    if (this.pendingAudioBytes >= LIVE_CAPTION_AGGREGATE_WRITE_BYTES) {
      this.writePendingAudio();
    }
    if (!LIVE_CAPTION_ENABLE_PERIODIC_FLUSH) return false;
    const elapsed = Date.now() - this.lastRtasrFlushAt;
    if (elapsed < LIVE_CAPTION_PERIODIC_FLUSH_MS) return false;
    if (this.audioBytesSinceFlush + this.pendingAudioBytes < LIVE_CAPTION_MIN_COMMIT_AUDIO_BYTES) {
      return false;
    }
    return this.flushRtasr();
  }

  warnOnce(key, text) {
    if (this.protocolWarnings.has(key)) return;
    this.protocolWarnings.add(key);
    this.textWriter.write(text);
  }

  handleControlEvent(evt) {
    if (!evt || typeof evt.kind !== "number") return false;
    if (evt.kind === ControlEventKind.SESSION_CLOSED) {
      return "session_closed";
    }

    syncStreamsFromControlEvent([this.textStream, this.voiceStream], evt);
    this.textWriter.flush();
    return true;
  }

  handleRtasrEvent(evt) {
    if (isSpeechStartedEvent(evt)) {
      this.transcriptWriter.beginLine();
      return;
    }

    if (isTranscriptDeltaEvent(evt) && typeof evt.delta === "string") {
      this.transcriptWriter.appendDelta(evt.delta);
      return;
    }

    if (isTranscriptCompletedEvent(evt)) {
      this.transcriptWriter.completeLine();
    }
  }

  drainRtasr() {
    let didWork = false;
    if (!this.rtasr) return didWork;

    while (true) {
      const evt = this.rtasr.readJson();
      if (evt == null) break;
      didWork = true;
      this.handleRtasrEvent(evt);
    }
    return didWork;
  }

  handleVoiceMessage(message) {
    if (
      requireOpenedStream(this.voiceStream, message, () =>
        this.warnOnce(
          "voice_missing_open",
          "protocol: voice stream must start with CTRL(open); dropping frames until open\n",
        ),
      )
    ) {
      return true;
    }

    if (message.kind === "ctrl") {
      const ctrl = this.voiceStream.acceptOpenHandshake(message);
      if (!ctrl && message.meta) {
        const metaText = decodeMessageMetaText(message, Spear.userStream.ssf.decodeUtf8);
        if (metaText.includes('"kind":"utterance_begin"')) {
          try {
            this.ensureRtasr();
            this.lastRtasrFlushAt = Date.now();
            this.resetPendingAudio();
          } catch (e) {
            this.textWriter.writeLine(`rtasr_connect failed: ${String(e)}`);
          }
        }
      }
      return true;
    }

    if (message.kind === "commit") {
      if (!parseStrictPayloadMeta(message)) {
        this.warnOnce("voice_bad_commit_meta", "protocol: voice COMMIT meta invalid; dropping frame\n");
        return true;
      }
      if (!this.rtasr) return true;
      this.flushRtasr();
      return true;
    }

    if (message.kind !== "data") return false;
    const payloadMeta = parseStrictPayloadMeta(message);
    if (!payloadMeta) {
      this.warnOnce("voice_bad_meta", "protocol: voice DATA meta invalid; dropping frame\n");
      return true;
    }
    if (this.rtasr && message.data instanceof Uint8Array && message.data.length > 0) {
      this.appendPendingAudio(message.data);
      this.maybeFlushRtasr();
    }
    return true;
  }

  pollKnownStreams() {
    let didWork = false;
    let drainedVoiceMessages = 0;
    let drainedVoiceBytes = 0;
    let drainedTextMessages = 0;
    while (true) {
      const voiceMessage = this.voiceStream.readMessage();
      if (!voiceMessage) break;
      didWork = true;
      drainedVoiceMessages += 1;
      if (voiceMessage.data instanceof Uint8Array) {
        drainedVoiceBytes += voiceMessage.data.length;
      }
      this.handleVoiceMessage(voiceMessage);
      if (drainedVoiceMessages % LIVE_CAPTION_RTASR_DRAIN_INTERVAL === 0) {
        if (this.drainRtasr()) didWork = true;
      }
    }

    if (this.drainRtasr()) didWork = true;

    while (drainedTextMessages < LIVE_CAPTION_TEXT_BATCH_SIZE) {
      const textMessage = this.textStream.readMessage();
      if (!textMessage) break;
      didWork = true;
      drainedTextMessages += 1;
      if (requireOpenedStream(this.textStream, textMessage)) {
        this.textWriter.flush();
      }
    }

    return didWork;
  }

  close() {
    try {
      this.rtasr?.close();
    } catch (_) {}
    try {
      this.textStream.close();
    } catch (_) {}
    try {
      this.voiceStream.close();
    } catch (_) {}
    try {
      this.ctl.close();
    } catch (_) {}
  }

  async run() {
    try {
      for (;;) {
        let didWork = false;

        if (this.drainRtasr()) didWork = true;

        const evt = this.ctl.readEvent();
        const controlResult = this.handleControlEvent(evt);
        if (controlResult === "session_closed") break;
        if (controlResult) didWork = true;

        if (this.drainRtasr()) didWork = true;
        if (this.pollKnownStreams()) didWork = true;
        if (this.drainRtasr()) didWork = true;

        if (!didWork) {
          Spear.sleepMs(this.rtasr ? 2 : 10);
        }
      }
    } finally {
      this.close();
    }

    return "user_stream_live_caption(js) done";
  }
}

export default async function main() {
  const app = new LiveCaptionApp();
  return app.run();
}
