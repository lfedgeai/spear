export type TaskSummary = { task_id: string; name: string; endpoint?: string }
export type InstanceSummary = { instance_id: string; status: string; current_execution_id: string }
export type ExecutionSummary = {
  execution_id: string
  status: string
  started_at_ms: number
  completed_at_ms: number
  function_name: string
}

type ListTasksResponse = {
  tasks: Array<{ task_id: string; name: string; endpoint?: string }>
  total_count: number
}
type ListTaskInstancesResponse = {
  instances: Array<{ instance_id: string; status: string; current_execution_id: string }>
  next_page_token: string
}
type ListInstanceExecutionsResponse = { executions: ExecutionSummary[]; next_page_token: string }

export async function fetchJson<T>(path: string): Promise<T> {
  const url = new URL(path, window.location.origin)
  const resp = await fetch(url)
  if (!resp.ok) {
    const text = await resp.text().catch(() => '')
    throw new Error(`${resp.status} ${text}`)
  }
  return (await resp.json()) as T
}

export async function fetchTasks(): Promise<TaskSummary[]> {
  const r = await fetchJson<ListTasksResponse>('/api/v1/tasks')
  return r.tasks.map((t) => ({
    task_id: t.task_id,
    name: t.name,
    endpoint: t.endpoint,
  }))
}

export async function fetchTaskInstances(taskId: string): Promise<InstanceSummary[]> {
  const r = await fetchJson<ListTaskInstancesResponse>(
    `/api/v1/tasks/${encodeURIComponent(taskId)}/instances?limit=100`,
  )
  return r.instances.map((i) => ({
    instance_id: i.instance_id,
    status: i.status,
    current_execution_id: i.current_execution_id,
  }))
}

export async function fetchInstanceExecutions(instanceId: string): Promise<ExecutionSummary[]> {
  const r = await fetchJson<ListInstanceExecutionsResponse>(
    `/api/v1/instances/${encodeURIComponent(instanceId)}/executions?limit=100`,
  )
  return r.executions
}

