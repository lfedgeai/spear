// SSF v1 constants (browser).
// SSF v1 常量定义（浏览器）。

export const SSF_MAGIC = [0x53, 0x50, 0x53, 0x54] as const
export const SSF_VERSION_V1 = 1 as const
export const SSF_HEADER_LEN_V1 = 32 as const

// SSF v1 msg_type values.
// SSF v1 的 msg_type 定义。
export const SsfMsgType = {
  CTRL: 1,
  DATA: 2,
  COMMIT: 3,
} as const

