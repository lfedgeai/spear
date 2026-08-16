type ApiError = {
  /** HTTP status code / HTTP 状态码 */
  status: number
  /** Error message from server or synthesized message / 服务端错误信息或本地拼接信息 */
  message: string
}

/**
 * Read admin token from browser storage.
 * 从浏览器存储读取管理端 token。
 */
export function getAdminToken() {
  return localStorage.getItem('ADMIN_TOKEN') || ''
}

/**
 * Fetch JSON from SMS Web Admin API.
 * 从 SMS Web Admin API 获取 JSON。
 */
export async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const baseUrl = (import.meta.env.VITE_SMS_ADMIN_BASE_URL as string | undefined) || ''
  const url = baseUrl ? `${baseUrl}${path}` : path

  const token = getAdminToken()
  const headers = new Headers(init?.headers)
  if (token) headers.set('Authorization', `Bearer ${token}`)

  const resp = await fetch(url, { ...init, headers })
  if (!resp.ok) {
    const text = await resp.text().catch(() => '')
    throw { status: resp.status, message: text || `HTTP ${resp.status}` } satisfies ApiError
  }
  return (await resp.json()) as T
}
