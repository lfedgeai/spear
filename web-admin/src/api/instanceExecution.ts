import { fetchJson } from '@/api/client'
import type {
  ListInstanceExecutionsResponse,
  ListTaskInstancesResponse,
  GetInstanceResponse,
  GetExecutionResponse,
} from '@/api/types'

export function listTaskInstances(input: {
  task_id: string
  limit?: number
  page_token?: string
}) {
  const qs = new URLSearchParams()
  if (input.limit) qs.set('limit', String(input.limit))
  if (input.page_token) qs.set('page_token', input.page_token)
  const suffix = qs.toString() ? `?${qs.toString()}` : ''
  return fetchJson<ListTaskInstancesResponse>(
    `/admin/api/tasks/${encodeURIComponent(input.task_id)}/instances${suffix}`,
  )
}

export function listInstanceExecutions(input: {
  instance_id: string
  limit?: number
  page_token?: string
}) {
  const qs = new URLSearchParams()
  if (input.limit) qs.set('limit', String(input.limit))
  if (input.page_token) qs.set('page_token', input.page_token)
  const suffix = qs.toString() ? `?${qs.toString()}` : ''
  return fetchJson<ListInstanceExecutionsResponse>(
    `/admin/api/instances/${encodeURIComponent(input.instance_id)}/executions${suffix}`,
  )
}

export function getInstance(instance_id: string) {
  return fetchJson<GetInstanceResponse>(
    `/admin/api/instances/${encodeURIComponent(instance_id)}`,
  )
}

export function getExecution(execution_id: string) {
  return fetchJson<GetExecutionResponse>(
    `/admin/api/executions/${encodeURIComponent(execution_id)}`,
  )
}
