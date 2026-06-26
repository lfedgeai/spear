# spear-boa

Boa-based JS SDK bindings for Spear guest WASM apps.

## Purpose

`spear-boa` injects virtual modules such as `spear`, `spear/chat`, `spear/rtasr`, and `spear/user_stream_protocol` into the Boa runtime used by JS-first WASM samples.

It now also exposes a small JS helper layer for reusable sample/app logic that is generic enough to live in the SDK.

## Builtin JS Modules

- `spear`
  - top-level `Spear` object
- `spear/chat`
  - chat completion helpers
  - Follows a single-turn request/response model, not a persistent multi-turn conversation session.
- `spear/rtasr`
  - RTASR session helpers
- `spear/ssf`
  - SSF frame encode/decode helpers
- `spear/user_stream_protocol`
  - managed user-stream helpers and strict payload parsing

## JS Helper Modules

- `spear/time_format`
  - `currentHmsPrefix()`
  - `formatHmsFromDate(date)`
  - Use for short `[HH:MM:SS]` display prefixes.
- `spear/rtasr_event`
  - `isSpeechStartedEvent(evt)`
  - `isTranscriptDeltaEvent(evt)`
  - `isTranscriptCompletedEvent(evt)`
  - `getCompletedTranscriptText(evt)`
  - Use for RTASR event compatibility across legacy and `conversation.item.*` event names.
- `spear/stream_gate`
  - `syncStreamsFromControlEvent(streams, evt)`
  - `requireOpenedStream(stream, message, onMissingOpen)`
  - `decodeMessageMetaText(message, decodeUtf8)`
  - Use for common user-stream gate/open-handshake behavior in JS-first guest apps.

## Minimal Example

The snippet below shows how the three helper modules can be used together in a JS-first guest app:

```js
import { Spear } from "spear";
import { isTranscriptDeltaEvent, isTranscriptCompletedEvent } from "spear/rtasr_event";
import { requireOpenedStream, syncStreamsFromControlEvent } from "spear/stream_gate";
import { currentHmsPrefix } from "spear/time_format";

const textStream = Spear.userStream.open(1);
const voiceStream = Spear.userStream.open(2);
const ctl = Spear.userStream.ctlOpen();

for (;;) {
  const evt = ctl.readEvent();
  if (evt) {
    syncStreamsFromControlEvent([textStream, voiceStream], evt);
  }

  const voiceMessage = voiceStream.readMessage();
  if (voiceMessage && !requireOpenedStream(voiceStream, voiceMessage)) {
    // forward audio / commit into RTASR here
  }

  const rtasrEvent = null; // replace with `rtasr.readJson()`
  if (isTranscriptDeltaEvent(rtasrEvent)) {
    textStream.sendText(`${currentHmsPrefix()}${rtasrEvent.delta}`, Spear.userStream.ssf.encodeUtf8('{"v":1}'));
  } else if (isTranscriptCompletedEvent(rtasrEvent)) {
    textStream.sendCommit(Spear.userStream.ssf.encodeUtf8('{"v":1}'));
  }
}
```

## Boundary

Helpers in `spear-boa` should stay generic and protocol-oriented.

Do **not** put sample-specific presentation policy here, such as:

- transcript line layout
- text-output formatting wrappers
- app-specific state machines

Those should remain in sample-local helpers, such as `samples/wasm-js/_shared/`.
