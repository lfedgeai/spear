/**
 * Web Admin API response types.
 * Web Admin API 响应类型。
 */

export type Stats = {
  /** Total nodes count / 节点总数 */
  total_count: number
  /** Online nodes count / 在线节点数 */
  online_count: number
  /** Offline nodes count / 离线节点数 */
  offline_count: number
  /** Nodes seen within last 60s / 最近 60 秒出现过的节点数 */
  recent_60s_count: number
}

export type NodeSummary = {
  /** Node UUID / 节点 UUID */
  uuid: string
  /** Human readable node name (optional) / 节点名称（可选） */
  name?: string
  /** IP address / IP 地址 */
  ip_address: string
  /** gRPC port / gRPC 端口 */
  port: number
  /** Node status string / 节点状态字符串 */
  status: string
  /** Last heartbeat timestamp (epoch seconds) / 最后心跳时间戳（秒） */
  last_heartbeat: number
  /** Registration timestamp (epoch seconds) / 注册时间戳（秒） */
  registered_at: number
  /** Extra metadata / 额外元数据 */
  metadata?: Record<string, string>
}

export type NodeResource = {
  /** CPU usage percent / CPU 使用率百分比 */
  cpu_usage_percent?: number
  /** Memory usage percent / 内存使用率百分比 */
  memory_usage_percent?: number
  /** Disk usage percent / 磁盘使用率百分比 */
  disk_usage_percent?: number
  /** Total memory bytes / 总内存字节数 */
  total_memory_bytes?: number
  /** Used memory bytes / 已用内存字节数 */
  used_memory_bytes?: number
  /** Available memory bytes / 可用内存字节数 */
  available_memory_bytes?: number
}

export type NodeDetail = {
  /** Whether the node is found / 是否找到节点 */
  found: boolean
  /** Node summary / 节点摘要 */
  node?: NodeSummary
  /** Resource usage / 资源使用情况 */
  resource?: NodeResource
}

export type CredentialSyncSnapshot = {
  status: string
  started: boolean
  watch_connected: boolean
  applied_revision: number
  local_store_epoch: number
  credential_count: number
  last_success_at_ms?: number | null
  last_error?: string | null
}

export type NodeCredentialSyncDetail = {
  success: boolean
  node_uuid?: string
  monitoring?: {
    credential_sync?: CredentialSyncSnapshot
  }
  message?: string
}

export type TaskSummary = {
  /** Task ID / 任务 ID */
  task_id: string
  /** Task name / 任务名称 */
  name: string
  /** Task description (optional) / 任务描述（可选） */
  description?: string
  /** Task status / 任务状态 */
  status: string
  /** Task priority / 任务优先级 */
  priority: string
  /** Node UUID / 节点 UUID */
  node_uuid: string
  /** Task endpoint / 任务端点 */
  endpoint: string
  /** Task version / 任务版本 */
  version: string
  /** Executable type / 可执行类型 */
  executable_type?: string
  /** Executable URI / 可执行 URI */
  executable_uri?: string
  /** Executable name / 可执行名称 */
  executable_name?: string
  /** Registration timestamp (epoch seconds) / 注册时间戳（秒） */
  registered_at: number
  /** Last heartbeat timestamp (epoch seconds) / 最后心跳时间戳（秒） */
  last_heartbeat: number
}

export type TaskDetail = {
  /** Whether the task is found / 是否找到任务 */
  found: boolean
  /** Full task object / 完整任务对象 */
  task?: Record<string, unknown>
}

export type FileItem = {
  /** Object ID / 对象 ID */
  id: string
  /** File name (optional) / 文件名（可选） */
  name?: string | null
  /** Length in bytes / 字节长度 */
  len: number
  /** Modified time (epoch seconds) / 修改时间（秒） */
  modified_at: number
}

export type InstanceSummary = {
  instance_id: string
  node_uuid: string
  status: string
  last_seen_ms: number
  current_execution_id: string
}

export type ExecutionSummary = {
  execution_id: string
  task_id: string
  status: string
  started_at_ms: number
  completed_at_ms: number
  function_name: string
}

export type LogRef = {
  backend: string
  uri_prefix: string
  content_type: string
  compression: string
}

export type ExecutionDetail = {
  execution_id: string
  invocation_id: string
  task_id: string
  function_name: string
  node_uuid: string
  instance_id: string
  status: string
  started_at_ms: number
  completed_at_ms: number
  updated_at_ms: number
  metadata: Record<string, string>
  log_ref?: LogRef | null
}

export type InstanceDetail = {
  instance_id: string
  task_id: string
  node_uuid: string
  status: string
  created_at_ms: number
  updated_at_ms: number
  last_seen_ms: number
  current_execution_id: string
  metadata: Record<string, string>
}

export type ListTaskInstancesResponse = {
  success: boolean
  message?: string
  instances?: InstanceSummary[]
  next_page_token?: string
}

export type ListInstanceExecutionsResponse = {
  success: boolean
  message?: string
  executions?: ExecutionSummary[]
  next_page_token?: string
}

export type GetExecutionResponse = {
  success: boolean
  message?: string
  found?: boolean
  execution?: ExecutionDetail
}

export type GetInstanceResponse = {
  success: boolean
  message?: string
  found?: boolean
  active?: boolean
  instance?: InstanceDetail
}
