import { fetchJson } from '@/api/client'

export type CreateExecutionPayload = {
  /** Task ID / 任务 ID */
  task_id: string
  /** Direct target node uuid (optional) / 直接目标节点 uuid（可选） */
  node_uuid?: string
  /** Execution mode (optional) / 执行模式（可选） */
  execution_mode?: 'sync' | 'async'
}

export type CreateExecutionResponse = {
  /** Whether the request succeeded / 请求是否成功 */
  success: boolean
  /** Human readable message (optional) / 人类可读信息（可选） */
  message?: string
  /** Created execution id (optional) / 创建的 execution id（可选） */
  execution_id?: string
  /** Selected node uuid (optional) / 选中的节点 uuid（可选） */
  node_uuid?: string
}

/**
 * Create an invocation via SMS Web Admin API.
 * 通过 SMS Web Admin API 创建一次 invocation。
 */
export function createExecution(payload: CreateExecutionPayload) {
  return fetchJson<CreateExecutionResponse>('/admin/api/invocations', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      task_id: payload.task_id,
      node_uuid: payload.node_uuid,
      execution_mode: payload.execution_mode,
    }),
  })
}
