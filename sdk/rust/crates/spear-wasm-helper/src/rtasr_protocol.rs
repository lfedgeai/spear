//! Shared RTASR event parsing helpers.
//! 共享的 RTASR 事件解析辅助模块。

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptEvent {
    SpeechStarted,
    Delta(String),
    Completed(Option<String>),
}

pub fn parse_transcript_event(payload: &[u8]) -> Option<TranscriptEvent> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    let event_type = value.get("type")?.as_str()?;

    match event_type {
        "speech_started" | "input_audio_buffer.speech_started" => {
            Some(TranscriptEvent::SpeechStarted)
        }
        "input_audio_transcription.delta"
        | "conversation.item.input_audio_transcription.delta"
        | "output_audio_transcript.delta" => {
            let delta = value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some(TranscriptEvent::Delta(delta.to_string()))
        }
        "input_audio_transcription.completed"
        | "conversation.item.input_audio_transcription.completed"
        | "output_audio_transcript.done" => {
            let transcript = value
                .get("transcript")
                .and_then(Value::as_str)
                .or_else(|| value.get("text").and_then(Value::as_str))
                .map(str::to_string);
            Some(TranscriptEvent::Completed(transcript))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delta_supports_conversation_item_namespace() {
        let payload =
            br#"{"type":"conversation.item.input_audio_transcription.delta","delta":"hello"}"#;
        let event = parse_transcript_event(payload).unwrap();
        assert_eq!(event, TranscriptEvent::Delta("hello".to_string()));
    }

    #[test]
    fn parse_completed_supports_text_fallback() {
        let payload = br#"{"type":"output_audio_transcript.done","text":"done"}"#;
        let event = parse_transcript_event(payload).unwrap();
        assert_eq!(event, TranscriptEvent::Completed(Some("done".to_string())));
    }

    #[test]
    fn parse_speech_started_supports_input_audio_buffer_namespace() {
        let payload = br#"{"type":"input_audio_buffer.speech_started"}"#;
        let event = parse_transcript_event(payload).unwrap();
        assert_eq!(event, TranscriptEvent::SpeechStarted);
    }
}
