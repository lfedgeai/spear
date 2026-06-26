# spear-boa

基于 Boa 的 Spear guest WASM 应用 JS SDK 绑定层。

## 定位

`spear-boa` 会向 Boa runtime 注入一组虚拟模块，例如 `spear`、`spear/chat`、`spear/rtasr`、`spear/user_stream_protocol`，供 JS-first WASM sample / app 直接使用。

现在它也额外暴露了一小层 JS helper，用来承载那些已经足够通用、适合进入 SDK 的 sample/app 复用能力。

## 内置 JS 模块

- `spear`
  - 顶层 `Spear` 对象
- `spear/chat`
  - chat completion 辅助
  - 采用单轮 request/response 语义，而不是长期多轮对话 session。
- `spear/rtasr`
  - RTASR session 辅助
- `spear/ssf`
  - SSF frame 编解码辅助
- `spear/user_stream_protocol`
  - managed user stream 辅助与严格 payload 解析

## JS Helper 模块

- `spear/time_format`
  - `currentHmsPrefix()`
  - `formatHmsFromDate(date)`
  - 适合生成短格式 `[HH:MM:SS]` 展示前缀。
- `spear/rtasr_event`
  - `isSpeechStartedEvent(evt)`
  - `isTranscriptDeltaEvent(evt)`
  - `isTranscriptCompletedEvent(evt)`
  - `getCompletedTranscriptText(evt)`
  - 适合处理旧版与 `conversation.item.*` 两套 RTASR 事件名兼容。
- `spear/stream_gate`
  - `syncStreamsFromControlEvent(streams, evt)`
  - `requireOpenedStream(stream, message, onMissingOpen)`
  - `decodeMessageMetaText(message, decodeUtf8)`
  - 适合封装 JS-first guest app 里常见的 user stream gate / open-handshake 逻辑。

## 最小使用示例

下面这段示例展示了这三个 helper 模块在 JS-first guest app 中如何一起使用：

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
    // 在这里把音频 / commit 转发给 RTASR
  }

  const rtasrEvent = null; // 这里替换成 `rtasr.readJson()`
  if (isTranscriptDeltaEvent(rtasrEvent)) {
    textStream.sendText(`${currentHmsPrefix()}${rtasrEvent.delta}`, Spear.userStream.ssf.encodeUtf8('{"v":1}'));
  } else if (isTranscriptCompletedEvent(rtasrEvent)) {
    textStream.sendCommit(Spear.userStream.ssf.encodeUtf8('{"v":1}'));
  }
}
```

## 边界

放进 `spear-boa` 的 helper 应保持“通用、偏协议层”的特征。

不要把 sample 专属的展示策略直接塞进这里，例如：

- transcript 行布局
- 文本输出格式包装
- app 专属状态机

这些内容更适合继续留在 sample 本地 helper 中，例如 `samples/wasm-js/_shared/`。
