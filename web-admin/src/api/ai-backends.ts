/**
 * Unified AI backend control-plane API client.
 * 统一 AI backend 控制面的 API 客户端。
 */

import { fetchJson } from '@/api/client'
import { buildAdminPath, buildQueryString } from '@/api/query'

export type AiBackendDesiredState = 'enabled' | 'disabled'
export type AiBackendHosting = 'remote' | 'local'
export type AiBackendManagementMode = 'sms_remote' | 'sms_local'
export type AiBackendNodeStatus =
  | 'pending'
  | 'reconciling'
  | 'ready'
  | 'degraded'
  | 'error'
  | 'disabled'
  | 'unspecified'

export type AiBackendSpec = {
  name: string
  kind: string
  operations: string[]
  features: string[]
  transports: string[]
  weight: number
  priority: number
  base_url: string
  provider: string
  model: string
  credential_ref?: string | null
  origin: number
  deployment_id: string
}

export type AiBackendSummary = {
  backend_id: string
  display_name: string
  provider: string
  model: string
  hosting: AiBackendHosting | 'unspecified'
  backend_kind: string
  desired_state: AiBackendDesiredState | 'unspecified'
  management_mode: AiBackendManagementMode | 'unspecified'
  credential_ref?: string | null
  spec?: AiBackendSpec | null
  labels?: Record<string, string>
  metadata?: Record<string, unknown>
  generation: number
  created_at_ms: number
  updated_at_ms: number
}

export type AiBackendPlacement = {
  placement_id: string
  backend_id: string
  node_uuid: string
  desired_state: AiBackendDesiredState | 'unspecified'
  weight_override?: number | null
  priority_override?: number | null
  generation: number
  created_at_ms: number
  updated_at_ms: number
}

export type AiBackendNodeStatusSnapshot = {
  backend_id: string
  node_uuid: string
  observed_generation: number
  status: AiBackendNodeStatus
  status_reason?: string
  runtime_backend_name?: string | null
  endpoint?: string | null
  available: boolean
  operations: string[]
  features: string[]
  transports: string[]
  last_heartbeat_at_ms: number
}

export type ResolvedAiBackendAssignment = {
  backend?: AiBackendSummary | null
  placement?: AiBackendPlacement | null
}

export type AiModelViewInstance = {
  backend_id: string
  node_uuid: string
  placement_state: AiBackendDesiredState | 'unspecified'
  backend_state: AiBackendDesiredState | 'unspecified'
  runtime_status?: AiBackendNodeStatus | null
  runtime_backend_name?: string | null
  endpoint?: string | null
  available: boolean
}

export type AiModelView = {
  provider: string
  model: string
  hosting: AiBackendHosting | 'unspecified'
  backend_ids: string[]
  operations: string[]
  features: string[]
  transports: string[]
  enabled_nodes: number
  ready_nodes: number
  total_nodes: number
  instances: AiModelViewInstance[]
}

export type ListAiBackendsResponse = {
  success: boolean
  backends?: AiBackendSummary[]
  total_count?: number
  message?: string
}

export type ListAiBackendsParams = {
  limit?: number
  offset?: number
  q?: string
  hosting?: AiBackendHosting
  desired_state?: AiBackendDesiredState
  provider?: string
  model?: string
}

export type GetAiBackendResponse = {
  success: boolean
  found: boolean
  backend?: AiBackendSummary | null
  message?: string
}

export type MutationAiBackendResponse = {
  success: boolean
  backend?: AiBackendSummary | null
  deleted?: boolean
  message?: string
}

export type ListAiBackendPlacementsResponse = {
  success: boolean
  placements?: AiBackendPlacement[]
  total_count?: number
  message?: string
}

export type MutationAiBackendPlacementResponse = {
  success: boolean
  placement?: AiBackendPlacement | null
  deleted?: boolean
  message?: string
}

export type ListAiBackendAssignmentsResponse = {
  success: boolean
  assignments?: ResolvedAiBackendAssignment[]
  total_count?: number
  message?: string
}

export type ListAiBackendNodeStatusesResponse = {
  success: boolean
  statuses?: AiBackendNodeStatusSnapshot[]
  total_count?: number
  message?: string
}

export type ListAiModelViewsResponse = {
  success: boolean
  views?: AiModelView[]
  total_count?: number
  message?: string
}

export type ListAiModelViewsParams = {
  limit?: number
  offset?: number
  q?: string
  hosting?: AiBackendHosting
  status?: 'available' | 'unavailable'
  provider?: string
  model?: string
}

export type WriteAiBackendInput = {
  display_name: string
  provider: string
  model: string
  hosting: AiBackendHosting
  backend_kind: string
  management_mode?: AiBackendManagementMode
  credential_ref?: string
  desired_state?: AiBackendDesiredState
  spec: {
    base_url?: string
    operations: string[]
    features?: string[]
    transports?: string[]
    weight?: number
    priority?: number
  }
  labels?: Record<string, string>
  metadata?: Record<string, unknown>
}

export type WriteAiBackendPlacementInput = {
  placement_id?: string
  backend_id: string
  node_uuid: string
  desired_state?: AiBackendDesiredState
  weight_override?: number
  priority_override?: number
}

/**
 * List unified AI backends.
 * 列出统一 AI backend 控制面资源。
 */
export function listAiBackends(params?: ListAiBackendsParams) {
  return fetchJson<ListAiBackendsResponse>(
    buildAdminPath('/ai-backends', {
      limit: params?.limit,
      offset: params?.offset,
      q: params?.q,
      hosting: params?.hosting,
      desired_state: params?.desired_state,
      provider: params?.provider,
      model: params?.model,
    }),
  )
}

/**
 * Get one unified AI backend by id.
 * 按 id 获取单个统一 AI backend。
 */
export function getAiBackend(backendId: string) {
  return fetchJson<GetAiBackendResponse>(buildAdminPath(`/ai-backends/${encodeURIComponent(backendId)}`))
}

/**
 * Create one unified AI backend.
 * 创建单个统一 AI backend。
 */
export function createAiBackend(input: WriteAiBackendInput) {
  return fetchJson<MutationAiBackendResponse>(buildAdminPath('/ai-backends'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  })
}

/**
 * Update one unified AI backend.
 * 更新单个统一 AI backend。
 */
export function updateAiBackend(backendId: string, input: WriteAiBackendInput) {
  return fetchJson<MutationAiBackendResponse>(
    buildAdminPath(`/ai-backends/${encodeURIComponent(backendId)}`),
    {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(input),
    },
  )
}

/**
 * Delete one unified AI backend.
 * 删除单个统一 AI backend。
 */
export function deleteAiBackend(backendId: string) {
  return fetchJson<MutationAiBackendResponse>(
    buildAdminPath(`/ai-backends/${encodeURIComponent(backendId)}`),
    {
      method: 'DELETE',
    },
  )
}

/**
 * Change one backend desired state.
 * 修改单个 backend 的期望状态。
 */
export function setAiBackendDesiredState(
  backendId: string,
  desired_state: AiBackendDesiredState,
) {
  return fetchJson<MutationAiBackendResponse>(
    buildAdminPath(`/ai-backends/${encodeURIComponent(backendId)}/desired-state`),
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ desired_state }),
    },
  )
}

/**
 * List placements, optionally filtered by backend or node.
 * 列出 placement，可按 backend 或节点过滤。
 */
export function listAiBackendPlacements(params?: {
  backend_id?: string
  node_uuid?: string
}) {
  return fetchJson<ListAiBackendPlacementsResponse>(
    buildAdminPath(`/ai-backend-placements${buildQueryString(params || {})}`),
  )
}

/**
 * Create or update one placement.
 * 创建或更新单个 placement。
 */
export function upsertAiBackendPlacement(input: WriteAiBackendPlacementInput) {
  return fetchJson<MutationAiBackendPlacementResponse>(buildAdminPath('/ai-backend-placements'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  })
}

/**
 * Delete one placement.
 * 删除单个 placement。
 */
export function deleteAiBackendPlacement(placementId: string) {
  return fetchJson<MutationAiBackendPlacementResponse>(
    buildAdminPath(`/ai-backend-placements/${encodeURIComponent(placementId)}`),
    {
      method: 'DELETE',
    },
  )
}

/**
 * List resolved assignments for one node.
 * 列出某个节点的解析后 assignment。
 */
export function listAiBackendAssignments(nodeUuid: string) {
  return fetchJson<ListAiBackendAssignmentsResponse>(
    buildAdminPath(`/ai-backend-assignments/${encodeURIComponent(nodeUuid)}`),
  )
}

/**
 * List node statuses for one backend.
 * 列出某个 backend 的节点状态。
 */
export function listAiBackendNodeStatuses(backendId: string) {
  return fetchJson<ListAiBackendNodeStatusesResponse>(
    buildAdminPath(`/ai-backend-statuses/${encodeURIComponent(backendId)}`),
  )
}

/**
 * List read-only AI model views.
 * 列出只读 AI Models 聚合视图。
 */
export function listAiModelViews(params?: ListAiModelViewsParams) {
  return fetchJson<ListAiModelViewsResponse>(
    buildAdminPath('/ai-model-views', {
      limit: params?.limit,
      offset: params?.offset,
      q: params?.q,
      hosting: params?.hosting,
      status: params?.status,
      provider: params?.provider,
      model: params?.model,
    }),
  )
}
