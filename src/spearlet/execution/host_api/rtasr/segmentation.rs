use crate::spearlet::execution::host_api::errno::EINVAL;
use crate::spearlet::execution::hostcall::types::{
    RtAsrClientCommitConfig, RtAsrClientCommitMode, RtAsrSegmentationConfig,
    RtAsrSegmentationStrategy, RtAsrSendItem, RtAsrState, RtAsrVadConfig,
};

pub(super) fn parse_segmentation_config(
    v: &serde_json::Value,
) -> Result<RtAsrSegmentationConfig, i32> {
    let obj = v.as_object().ok_or(-EINVAL)?;

    let flush_on_close = obj
        .get("flush_on_close")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);

    if let Some(strategy_s) = obj.get("strategy").and_then(|x| x.as_str()) {
        let strategy = match strategy_s {
            "manual" | "off" => RtAsrSegmentationStrategy::Manual,
            "server_vad" => RtAsrSegmentationStrategy::ServerVad,
            "client_commit" => RtAsrSegmentationStrategy::ClientCommit,
            _ => return Err(-EINVAL),
        };

        let vad = match obj.get("vad") {
            Some(vad_v) => {
                let vad_o = vad_v.as_object().ok_or(-EINVAL)?;
                let silence_ms = vad_o
                    .get("silence_ms")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(500);
                let threshold = vad_o.get("threshold").and_then(|x| x.as_f64());
                let prefix_padding_ms = vad_o.get("prefix_padding_ms").and_then(|x| x.as_u64());
                Some(RtAsrVadConfig {
                    silence_ms,
                    threshold,
                    prefix_padding_ms,
                })
            }
            None => None,
        };

        let client_commit = match obj.get("client_commit") {
            Some(cc_v) => {
                let cc_o = cc_v.as_object().ok_or(-EINVAL)?;
                let flush_interval_ms = cc_o.get("flush_interval_ms").and_then(|x| x.as_u64());
                let max_buffer_bytes = cc_o
                    .get("max_buffer_bytes")
                    .and_then(|x| x.as_u64())
                    .map(|x| x as usize);
                let silence_ms = cc_o.get("silence_ms").and_then(|x| x.as_u64());
                let min_flush_gap_ms = cc_o
                    .get("min_flush_gap_ms")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(500);
                Some(RtAsrClientCommitConfig {
                    mode: RtAsrClientCommitMode::Hybrid,
                    flush_interval_ms,
                    max_buffer_bytes,
                    silence_ms,
                    min_flush_gap_ms,
                })
            }
            None => None,
        };

        return Ok(RtAsrSegmentationConfig {
            strategy,
            vad,
            client_commit,
            flush_on_close,
        });
    }

    let enabled = obj
        .get("enabled")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let mode_s = obj.get("mode").and_then(|x| x.as_str()).unwrap_or("off");
    let flush_interval_ms = obj.get("flush_interval_ms").and_then(|x| x.as_u64());
    let max_buffer_bytes = obj
        .get("max_buffer_bytes")
        .and_then(|x| x.as_u64())
        .map(|x| x as usize);
    let silence_ms = obj.get("silence_ms").and_then(|x| x.as_u64());
    let min_flush_gap_ms = obj
        .get("min_flush_gap_ms")
        .and_then(|x| x.as_u64())
        .unwrap_or(500);

    if !enabled || mode_s == "off" {
        return Ok(RtAsrSegmentationConfig {
            strategy: RtAsrSegmentationStrategy::Manual,
            vad: None,
            client_commit: None,
            flush_on_close,
        });
    }

    if mode_s == "silence" {
        let vad = RtAsrVadConfig {
            silence_ms: silence_ms.or(flush_interval_ms).unwrap_or(500),
            threshold: Some(0.5),
            prefix_padding_ms: Some(300),
        };
        return Ok(RtAsrSegmentationConfig {
            strategy: RtAsrSegmentationStrategy::ServerVad,
            vad: Some(vad),
            client_commit: None,
            flush_on_close,
        });
    }

    let cc = RtAsrClientCommitConfig {
        mode: RtAsrClientCommitMode::Hybrid,
        flush_interval_ms: if mode_s == "bytes" {
            None
        } else {
            flush_interval_ms
        },
        max_buffer_bytes: if mode_s == "time" {
            None
        } else {
            max_buffer_bytes
        },
        silence_ms: if mode_s == "bytes" || mode_s == "time" {
            None
        } else {
            silence_ms
        },
        min_flush_gap_ms,
    };
    Ok(RtAsrSegmentationConfig {
        strategy: RtAsrSegmentationStrategy::ClientCommit,
        vad: None,
        client_commit: Some(cc),
        flush_on_close,
    })
}

pub(super) fn segmentation_config_to_json(cfg: &RtAsrSegmentationConfig) -> serde_json::Value {
    let strategy = match cfg.strategy {
        RtAsrSegmentationStrategy::Manual => "manual",
        RtAsrSegmentationStrategy::ServerVad => "server_vad",
        RtAsrSegmentationStrategy::ClientCommit => "client_commit",
    };

    let vad = cfg.vad.as_ref().map(|v| {
        let mut o = serde_json::json!({
            "silence_ms": v.silence_ms,
        });
        if let Some(th) = v.threshold {
            o["threshold"] = serde_json::Value::from(th);
        }
        if let Some(pp) = v.prefix_padding_ms {
            o["prefix_padding_ms"] = serde_json::Value::from(pp);
        }
        o
    });

    let client_commit = cfg.client_commit.as_ref().map(|c| {
        serde_json::json!({
            "flush_interval_ms": c.flush_interval_ms,
            "max_buffer_bytes": c.max_buffer_bytes,
            "silence_ms": c.silence_ms,
            "min_flush_gap_ms": c.min_flush_gap_ms,
        })
    });

    serde_json::json!({
        "strategy": strategy,
        "flush_on_close": cfg.flush_on_close,
        "vad": vad,
        "client_commit": client_commit,
    })
}

pub(super) fn rtasr_flush_event_text() -> String {
    serde_json::to_string(&serde_json::json!({"type":"input_audio_buffer.commit"}))
        .unwrap_or_else(|_| "{\"type\":\"input_audio_buffer.commit\"}".to_string())
}

pub(super) fn rtasr_clear_event_text() -> String {
    serde_json::to_string(&serde_json::json!({"type":"input_audio_buffer.clear"}))
        .unwrap_or_else(|_| "{\"type\":\"input_audio_buffer.clear\"}".to_string())
}

pub(super) fn maybe_enqueue_autoflush_locked(st: &mut RtAsrState, now: std::time::Instant) {
    if st.segmentation.strategy != RtAsrSegmentationStrategy::ClientCommit {
        return;
    }
    let Some(cfg) = st.segmentation.client_commit.as_ref() else {
        return;
    };
    if st.pending_flush {
        return;
    }
    if st.buffered_audio_bytes_since_flush == 0 {
        return;
    }
    if now.duration_since(st.last_flush_at).as_millis() < (cfg.min_flush_gap_ms as u128) {
        return;
    }

    let mut triggered = false;
    if let Some(max) = cfg.max_buffer_bytes {
        triggered |= st.buffered_audio_bytes_since_flush >= max;
    }
    if let Some(ms) = cfg.flush_interval_ms {
        triggered |= now.duration_since(st.last_flush_at) >= std::time::Duration::from_millis(ms);
    }
    if let Some(ms) = cfg.silence_ms {
        triggered |= now.duration_since(st.last_audio_at) >= std::time::Duration::from_millis(ms);
    }

    if !triggered {
        return;
    }

    let txt = rtasr_flush_event_text();
    let n = txt.len();
    if st.send_queue_bytes.saturating_add(n) > st.max_send_queue_bytes {
        return;
    }
    st.send_queue.push_back(RtAsrSendItem::WsText(txt));
    st.send_queue_bytes = st.send_queue_bytes.saturating_add(n);
    st.pending_flush = true;
    st.buffered_audio_bytes_since_flush = 0;
    st.last_flush_at = now;
}

pub(super) fn apply_turn_detection_to_client_events(
    client_events: &mut [serde_json::Value],
    cfg: &RtAsrSegmentationConfig,
) {
    let turn_detection = if cfg.strategy == RtAsrSegmentationStrategy::ServerVad {
        let vad = cfg.vad.clone().unwrap_or_default();
        let silence_duration_ms = vad.silence_ms;
        let threshold = vad.threshold.unwrap_or(0.5);
        let prefix_padding_ms = vad.prefix_padding_ms.unwrap_or(300);
        serde_json::json!({
            "type": "server_vad",
            "threshold": threshold,
            "prefix_padding_ms": prefix_padding_ms,
            "silence_duration_ms": silence_duration_ms,
        })
    } else {
        serde_json::Value::Null
    };

    for ev in client_events.iter_mut() {
        let Some(obj) = ev.as_object_mut() else {
            continue;
        };
        if obj.get("type").and_then(|x| x.as_str()) != Some("session.update") {
            continue;
        }
        let Some(session) = obj.get_mut("session").and_then(|x| x.as_object_mut()) else {
            continue;
        };
        let mut applied = false;
        if let Some(audio) = session.get_mut("audio").and_then(|x| x.as_object_mut()) {
            if let Some(input) = audio.get_mut("input").and_then(|x| x.as_object_mut()) {
                input.insert("turn_detection".to_string(), turn_detection.clone());
                applied = true;
            }
        }
        if !applied {
            session.insert("turn_detection".to_string(), turn_detection.clone());
        }
    }
}
