import { fetchJson } from '@/api/client'
import { buildAdminPath } from '@/api/query'
import type { DeleteTaskResponse, TaskDetail, TaskSummary } from '@/api/types'

export type ListTasksParams = {
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
 * List tasks from SMS.
 * 从 SMS 列出任务。
 */
export async function listTasks(params: ListTasksParams) {
  return fetchJson<{ tasks: TaskSummary[]; total_count: number }>(
    buildAdminPath('/admin/api/tasks', {
      q: params.q,
      sort_by: params.sort_by,
      order: params.order,
      limit: params.limit,
    }),
  )
}

/**
 * Get task detail by task id.
 * 通过 task id 获取任务详情。
 */
export function getTaskDetail(taskId: string) {
  return fetchJson<TaskDetail>(`/admin/api/tasks/${encodeURIComponent(taskId)}`)
}

export type CreateTaskPayload = {
  /** Task name / 任务名称 */
  name: string
  /** Task description (optional) / 任务描述（可选） */
  description?: string
  /** Task priority (optional) / 任务优先级（可选） */
  priority?: string
  /** Task endpoint / 任务端点 */
  endpoint: string
  /** Task version / 任务版本 */
  version: string
  /** Task capabilities (optional) / 任务能力（可选） */
  capabilities?: string[]
  /** Executable descriptor / 可执行描述 */
  executable?: {
    /** Executable type / 可执行类型 */
    type: string
    /** Executable uri (optional) / 可执行 uri（可选） */
    uri?: string
    /** Executable name (optional) / 可执行名称（可选） */
    name?: string
    /** SHA256 checksum (optional) / SHA256 校验（可选） */
    checksum_sha256?: string
    /** Default args (optional) / 默认参数（可选） */
    args?: string[]
    /** Default environment variables (optional) / 默认环境变量（可选） */
    env?: Record<string, string>
  }
  /** Task config map / Task 配置（map<string,string>） */
  config?: Record<string, string>
  /** Desired replicas / 期望副本数 */
  desired_replicas?: number
  /** Scheduling strategy / 调度策略 */
  scheduling_strategy?: string
}

/**
 * Create a task.
 * 创建任务。
 */
export function createTask(payload: CreateTaskPayload) {
  return fetchJson<{ success: boolean; task_id?: string; message?: string }>(
    '/admin/api/tasks',
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        name: payload.name,
        description: payload.description,
        priority: payload.priority,
        endpoint: payload.endpoint,
        version: payload.version,
        capabilities: payload.capabilities,
        config: payload.config,
        executable: payload.executable,
        desired_replicas: payload.desired_replicas ?? 1,
        scheduling_strategy: payload.scheduling_strategy ?? 'spread',
      }),
    },
  )
}

export type DeleteTaskPayload = {
  /** Task id / 任务 ID */
  task_id: string
  /** Deletion reason (optional) / 删除原因（可选） */
  reason?: string
  /** Whether to force runtime cleanup / 是否强制回收运行态 */
  force?: boolean
}

/**
 * Request coordinated task deletion.
 * 请求协同删除 task。
 */
export function deleteTask(payload: DeleteTaskPayload) {
  return fetchJson<DeleteTaskResponse>(`/admin/api/tasks/${encodeURIComponent(payload.task_id)}`, {
    method: 'DELETE',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      reason: payload.reason,
      force: payload.force ?? true,
    }),
  })
}
