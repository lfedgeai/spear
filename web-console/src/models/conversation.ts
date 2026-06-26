export type ChatRole = 'user' | 'assistant' | 'system'

export type ChatMessage = {
  id: string
  role: ChatRole
  text: string
  createdAt: number
  streaming?: boolean
}

export type Conversation = {
  id: string
  title: string
  connect_kind: 'execution' | 'endpoint'
  taskId: string
  instanceId: string
  executionId: string
  gatewayEndpoint: string
  conn_status: 'disconnected' | 'connecting' | 'connected'
  conn_error: string
  createdAt: number
  messages: ChatMessage[]
}

export type ConversationInit = {
  connect_kind?: 'execution' | 'endpoint'
  taskId?: string
  instanceId?: string
  executionId?: string
  gatewayEndpoint?: string
}

export function createConversation(
  init?: ConversationInit,
  defaults?: { executionId?: string },
): Conversation {
  return {
    id: newId(),
    title: '',
    connect_kind: init?.connect_kind ?? 'execution',
    taskId: init?.taskId ?? '',
    instanceId: init?.instanceId ?? '',
    executionId: init?.executionId ?? defaults?.executionId ?? '',
    gatewayEndpoint: init?.gatewayEndpoint ?? '',
    conn_status: 'disconnected',
    conn_error: '',
    createdAt: Date.now(),
    messages: [],
  }
}

export function guessTitleFromText(text: string): string {
  const t = text.replace(/\s+/g, ' ').trim()
  if (!t) return ''
  if (t.length <= 40) return t
  return t.slice(0, 40) + '…'
}

export function newId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`
}
