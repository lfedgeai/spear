function isEventType(evt, type) {
  return !!evt && typeof evt === "object" && evt.type === type;
}

export function isSpeechStartedEvent(evt) {
  return isEventType(evt, "speech_started") || isEventType(evt, "input_audio_buffer.speech_started");
}

export function isTranscriptDeltaEvent(evt) {
  return (
    isEventType(evt, "input_audio_transcription.delta") ||
    isEventType(evt, "conversation.item.input_audio_transcription.delta") ||
    isEventType(evt, "output_audio_transcript.delta")
  );
}

export function isTranscriptCompletedEvent(evt) {
  return (
    isEventType(evt, "input_audio_transcription.completed") ||
    isEventType(evt, "conversation.item.input_audio_transcription.completed") ||
    isEventType(evt, "output_audio_transcript.done")
  );
}

export function getCompletedTranscriptText(evt) {
  if (!evt || typeof evt !== "object") return "";
  if (typeof evt.transcript === "string" && evt.transcript.length > 0) return evt.transcript;
  if (typeof evt.text === "string" && evt.text.length > 0) return evt.text;
  return "";
}
