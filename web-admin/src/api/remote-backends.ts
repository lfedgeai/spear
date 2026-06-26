import { fetchJson } from '@/api/client'

export type RemoteBackendConfig = {
  name: string
  kind: string
  base_url: string
  model?: string
  credential_ref?: string
  weight?: number
  priority?: number
  operations: string[]
  features?: string[]
  transports?: string[]
  provider?: string
}

export type ListRemoteBackendsResponse = {
  success: boolean
  message?: string
  revision?: number
  backends?: RemoteBackendConfig[]
}

export type UpsertRemoteBackendResponse = {
  success: boolean
  message?: string
  revision?: number
}

export type DeleteRemoteBackendResponse = {
  success: boolean
  message?: string
  revision?: number
  deleted?: boolean
}

export function listRemoteBackends() {
  return fetchJson<ListRemoteBackendsResponse>('/admin/api/ai/remote-backends')
}

export function upsertRemoteBackend(backend: RemoteBackendConfig) {
  return fetchJson<UpsertRemoteBackendResponse>('/admin/api/ai/remote-backends', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(backend),
  })
}

export function deleteRemoteBackend(name: string) {
  return fetchJson<DeleteRemoteBackendResponse>(
    `/admin/api/ai/remote-backends/${encodeURIComponent(name)}`,
    { method: 'DELETE' },
  )
}
