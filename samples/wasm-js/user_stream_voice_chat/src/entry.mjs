// Voice chat sample (JS):
// - opens the text + voice user streams announced by the runtime
// - uploads utterance-scoped PCM chunks to RTASR
// - waits for RTASR completion after COMMIT
// - sends transcript + downstream chat completion response to the text stream

import { Spear } from "spear";
import { RtasrSession } from "spear/rtasr";
import {
  BufferedTextOutput,
  ControlEventKind,
  ManagedUserStream,
  parseStrictPayloadMeta,
} from "spear/user_stream_protocol";
import {
  getCompletedTranscriptText,
  isTranscriptCompletedEvent,
  isTranscriptDeltaEvent,
} from "spear/rtasr_event";
import { decodeMessageMetaText, requireOpenedStream, syncStreamsFromControlEvent } from "spear/stream_gate";
import { TextOutputWriter } from "app/lib/text_output";

const STREAM_TEXT = 1;
const STREAM_VOICE = 2;
const MIN_BYTES_BEFORE_COMMIT = 4800;

const VoiceState = {
  IDLE: "idle",
  RECORDING: "recording",
  COMMITTING: "committing",
};

class VoiceChatApp {
  constructor() {
    this.ctl = Spear.userStream.ctlOpen();
    this.textStream = new ManagedUserStream({ streamId: STREAM_TEXT, modality: "text" });
    this.voiceStream = new ManagedUserStream({ streamId: STREAM_VOICE, modality: "audio" });
    this.textOut = new BufferedTextOutput(this.textStream);
    this.textWriter = new TextOutputWriter(this.textOut);
    this.rtasr = null;
    this.voiceState = VoiceState.IDLE;
    this.voiceBytes = 0;
    this.lastTranscript = "";
    this.protocolWarnings = new Set();
  }

  warnOnce(key, text) {
    if (this.protocolWarnings.has(key)) return;
    this.protocolWarnings.add(key);
    this.textWriter.write(text);
  }

  async sendChatCompletion(prompt) {
    try {
      const response = await Spear.chat.completions.create({
        messages: [{ role: "user", content: prompt }],
        timeoutMs: 30_000,
      });
      this.textWriter.writeLine(response.text());
    } catch (e) {
      this.textWriter.writeLine(`chat completion failed: ${String(e)}`);
    }
  }

  ensureRtasr() {
    if (this.rtasr) return this.rtasr;
    const session = RtasrSession.create();
    session.setParamString("transport", "websocket");
    session.connect();
    this.rtasr = session;
    return session;
  }

  beginUtterance() {
    this.voiceState = VoiceState.RECORDING;
    this.voiceBytes = 0;
    this.lastTranscript = "";

    try {
      if (!this.rtasr) {
        this.ensureRtasr();
      } else {
        this.rtasr.clear();
      }
    } catch (e) {
      this.voiceState = VoiceState.IDLE;
      this.textWriter.writeLine(`rtasr_connect failed: ${String(e)}`);
    }
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

  async handleRtasrEvent(evt) {
    if (isTranscriptDeltaEvent(evt) && typeof evt.delta === "string") {
      this.lastTranscript += evt.delta;
      return;
    }

    if (isTranscriptCompletedEvent(evt)) {
      const completedText = getCompletedTranscriptText(evt);
      if (completedText) {
        this.lastTranscript = completedText;
      }

      if (this.voiceState === VoiceState.COMMITTING && this.lastTranscript) {
        this.textWriter.writeTimestampedLine(this.lastTranscript);
        await this.sendChatCompletion(this.lastTranscript);
        this.voiceState = VoiceState.IDLE;
        this.lastTranscript = "";
      }
    }
  }

  async drainRtasr() {
    let didWork = false;
    if (!this.rtasr) return didWork;

    while (true) {
      const evt = this.rtasr.readJson();
      if (evt == null) break;
      didWork = true;
      await this.handleRtasrEvent(evt);
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
      const metaText = decodeMessageMetaText(message, Spear.userStream.ssf.decodeUtf8);
      if (metaText.includes('"kind":"utterance_begin"')) {
        this.beginUtterance();
      }
      return true;
    }

    if (message.kind === "data" && this.voiceState === VoiceState.RECORDING) {
      if (!parseStrictPayloadMeta(message)) {
        this.warnOnce("voice_bad_data_meta", "protocol: voice DATA meta invalid; dropping frame\n");
        return true;
      }
      if (this.rtasr && message.data instanceof Uint8Array && message.data.length > 0) {
        this.voiceBytes += message.data.length;
        this.rtasr.writeAudio(message.data);
      }
      return true;
    }

    if (message.kind === "commit" && this.voiceState === VoiceState.RECORDING) {
      if (!parseStrictPayloadMeta(message)) {
        this.warnOnce("voice_bad_commit_meta", "protocol: voice COMMIT meta invalid; dropping frame\n");
        return true;
      }

      if (!this.rtasr || this.voiceBytes < MIN_BYTES_BEFORE_COMMIT) {
        this.voiceState = VoiceState.IDLE;
        return true;
      }

      this.rtasr.flush();
      this.voiceState = VoiceState.COMMITTING;
      return true;
    }

    return false;
  }

  pollKnownStreams() {
    let didWork = false;

    const textMessage = this.textStream.readMessage();
    if (textMessage) {
      didWork = true;
      if (requireOpenedStream(this.textStream, textMessage)) {
        this.textWriter.flush();
      }
    }

    const voiceMessage = this.voiceStream.readMessage();
    if (voiceMessage) {
      didWork = true;
      this.handleVoiceMessage(voiceMessage);
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

        const evt = this.ctl.readEvent();
        const controlResult = this.handleControlEvent(evt);
        if (controlResult === "session_closed") break;
        if (controlResult) didWork = true;

        if (await this.drainRtasr()) didWork = true;
        if (this.pollKnownStreams()) didWork = true;

        if (!didWork) {
          Spear.sleepMs(10);
        }
      }
    } finally {
      this.close();
    }

    return "user_stream_voice_chat(js) done";
  }
}

export default async function main() {
  console.log("user_stream_voice_chat(js) started");
  const app = new VoiceChatApp();
  return app.run();
}
