/// How to combine SMS-provided remote backends with the local Spearlet runtime.
/// 如何将 SMS 下发的 remote backends 与本地 Spearlet runtime 进行合并。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteBackendMergePolicy {
    /// SMS entries override local entries with the same `name`.
    /// SMS 条目覆盖同名本地条目。
    SmsWinsByName,
    /// Local entries override SMS entries with the same `name`.
    /// 本地条目覆盖同名 SMS 条目。
    LocalWinsByName,
    /// Only append SMS entries that do not exist locally.
    /// 仅追加本地不存在的 SMS 条目。
    AppendIfMissing,
}

impl RemoteBackendMergePolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sms_wins_by_name" => Some(Self::SmsWinsByName),
            "local_wins_by_name" => Some(Self::LocalWinsByName),
            "append_if_missing" => Some(Self::AppendIfMissing),
            _ => None,
        }
    }
}
