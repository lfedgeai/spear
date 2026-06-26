import { fetchJson } from '@/api/client'
import type { NodeCredentialSyncDetail, NodeDetail, NodeSummary } from '@/api/types'

export type ListNodesParams = {
  /** Query string for fuzzy search / 模糊搜索关键词 */
  q?: string
  /** Sort field name / 排序字段 */
  sort_by?: string
  /** Sort order / 排序方向 */
  order?: 'asc' | 'desc'
  /** Maximum number of items / 最大返回条数 */
  limit?: number
}

/**
 * List nodes from SMS.
 * 从 SMS 列出节点。
 */
export async function listNodes(params: ListNodesParams) {
  const url = new URL('/admin/api/nodes', window.location.origin)
  if (params.q) url.searchParams.set('q', params.q)
  if (params.sort_by) url.searchParams.set('sort_by', params.sort_by)
  if (params.order) url.searchParams.set('order', params.order)
  if (params.limit) url.searchParams.set('limit', String(params.limit))
  return fetchJson<{ nodes: NodeSummary[]; total_count: number }>(
    url.pathname + url.search,
  )
}

/**
 * Get node detail by uuid.
 * 通过 uuid 获取节点详情。
 */
export function getNodeDetail(uuid: string) {
  return fetchJson<NodeDetail>(`/admin/api/nodes/${encodeURIComponent(uuid)}`)
}

/**
 * Get credential sync detail for one node.
 * 获取单个节点的 credential 同步状态。
 */
export function getNodeCredentialSync(uuid: string) {
  return fetchJson<NodeCredentialSyncDetail>(
    `/admin/api/nodes/${encodeURIComponent(uuid)}/ai/credentials`,
  )
}
