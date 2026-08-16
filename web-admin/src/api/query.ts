type QueryValue =
  | string
  | number
  | boolean
  | null
  | undefined
  | Array<string | number | boolean>

export function buildQueryString(params: Record<string, QueryValue>) {
  const qs = new URLSearchParams()

  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '') continue
    if (Array.isArray(value)) {
      for (const item of value) {
        qs.append(key, String(item))
      }
      continue
    }
    qs.set(key, String(value))
  }

  const search = qs.toString()
  return search ? `?${search}` : ''
}

export function buildAdminPath(
  pathname: string,
  params?: Record<string, QueryValue>,
) {
  const normalizedPath = pathname.startsWith('/admin/api')
    ? pathname
    : `/admin/api${pathname.startsWith('/') ? pathname : `/${pathname}`}`
  return `${normalizedPath}${params ? buildQueryString(params) : ''}`
}
