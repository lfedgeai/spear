import { fetchJson } from '@/api/client'

export type CredentialInfo = {
  name: string
  provider_kind: string
  version: number
  description?: string
  disabled: boolean
  created_at_ms: number
  updated_at_ms: number
  referenced_by_count?: number
}

export type ListCredentialsResponse = {
  success: boolean
  message?: string
  revision?: number
  credentials?: CredentialInfo[]
}

export type UpsertCredentialBody = {
  name: string
  secret?: string
  description?: string
  disabled?: boolean
}

export type UpsertCredentialResponse = {
  success: boolean
  message?: string
  revision?: number
}

export type DeleteCredentialResponse = {
  success: boolean
  message?: string
  revision?: number
  deleted?: boolean
}

export function listCredentials() {
  return fetchJson<ListCredentialsResponse>('/admin/api/ai/credentials')
}

export function upsertCredential(body: UpsertCredentialBody) {
  return fetchJson<UpsertCredentialResponse>('/admin/api/ai/credentials', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
}

export function deleteCredential(name: string) {
  return fetchJson<DeleteCredentialResponse>(
    `/admin/api/ai/credentials/${encodeURIComponent(name)}`,
    { method: 'DELETE' },
  )
}
