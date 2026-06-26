//! Shared time-format helpers for Rust-first guest apps.
//! Rust-first guest app 共享的时间格式化辅助模块。

use spear_wasm::{time_now_ms, SpearError};

pub fn format_hms_prefix_from_ms(now_ms: i64) -> String {
    let total_seconds = now_ms.max(0) / 1000;
    let hh = (total_seconds / 3600) % 24;
    let mm = (total_seconds / 60) % 60;
    let ss = total_seconds % 60;
    format!("[{hh:02}:{mm:02}:{ss:02}] ")
}

pub fn current_hms_prefix() -> Result<String, SpearError> {
    Ok(format_hms_prefix_from_ms(time_now_ms()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_hms_prefix_uses_second_precision() {
        assert_eq!(format_hms_prefix_from_ms(3_661_999), "[01:01:01] ");
    }

    #[test]
    fn format_hms_prefix_clamps_negative_values() {
        assert_eq!(format_hms_prefix_from_ms(-1), "[00:00:00] ");
    }
}
