import { fetchJson } from '@/api/client'
import { buildAdminPath } from '@/api/query'
import type {
  ListInstanceExecutionsResponse,
  ListExecutionHistoryResponse,
  ListTaskInstancesResponse,
  GetInstanceResponse,
  GetExecutionResponse,
} from '@/api/types'

export function listTaskInstances(input: {
  task_id: string
  limit?: number
  page_token?: string
}) {
  return fetchJson<ListTaskInstancesResponse>(
    buildAdminPath(`/admin/api/tasks/${encodeURIComponent(input.task_id)}/instances`, {
      limit: input.limit,
      page_token: input.page_token,
    }),
  )
}

export function listInstanceExecutions(input: {
  instance_id: string
  limit?: number
  page_token?: string
}) {
  return fetchJson<ListInstanceExecutionsResponse>(
    buildAdminPath(
      `/admin/api/instances/${encodeURIComponent(input.instance_id)}/executions`,
      {
        limit: input.limit,
        page_token: input.page_token,
      },
    ),
  )
}

export function listExecutionHistory(input?: {
  task_id?: string
  status?: string
  limit?: number
  page_token?: string
}) {
  return fetchJson<ListExecutionHistoryResponse>(
    buildAdminPath('/admin/api/executions', {
      task_id: input?.task_id,
      status: input?.status,
      limit: input?.limit,
      page_token: input?.page_token,
    }),
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
