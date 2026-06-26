export function syncStreamsFromControlEvent(streams, evt) {
  for (const stream of streams) {
    stream.onControlEvent(evt);
  }
}

export function requireOpenedStream(stream, message, onMissingOpen) {
  if (stream.opened) return false;
  if (stream.acceptOpenHandshake(message)) return true;
  if (typeof onMissingOpen === "function") {
    onMissingOpen();
  }
  return true;
}

export function decodeMessageMetaText(message, decodeUtf8) {
  try {
    if (!message?.meta) return "";
    return decodeUtf8(message.meta);
  } catch (_) {
    return "<meta-decode-failed>";
  }
}
