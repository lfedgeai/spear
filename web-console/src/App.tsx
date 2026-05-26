// SPEAR Console main page.
// SPEAR Console 主页面。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { SpearStreamClient, createStreamSession } from './spearStream'

const DEFAULT_STREAM_ID = 1
const STORAGE_KEY = 'spear.console.v1'
const APP_TITLE = 'SPEAR Console'

type ChatRole = 'user' | 'assistant' | 'system'

type ChatMessage = {
  id: string
  role: ChatRole
  text: string
  createdAt: number
  streaming?: boolean
}

type Conversation = {
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

export default function App() {
  const [input, setInput] = useState<string>('')
  const [conversations, setConversations] = useState<Conversation[]>(() => loadConversations())
  const [activeId, setActiveId] = useState<string>(() => loadActiveId() ?? '')

  const clientMapRef = useRef<Map<string, SpearStreamClient>>(new Map())
  const autoConnectKeyRef = useRef<Map<string, string>>(new Map())
  const endRef = useRef<HTMLDivElement | null>(null)

  const active = useMemo(
    () => conversations.find((c) => c.id === activeId) ?? conversations[0],
    [activeId, conversations],
  )

  const activeExecutionId = active?.executionId ?? ''
  const activeGatewayEndpoint = active?.gatewayEndpoint ?? ''
  const activeConnectKind = active?.connect_kind ?? 'execution'
  const activeMessagesLen = active?.messages.length ?? 0
  const activeStatus = active?.conn_status ?? 'disconnected'
  const activeError = active?.conn_error ?? ''

  const activeTargetText = useMemo(() => {
    if (activeConnectKind === 'endpoint' && activeGatewayEndpoint.trim()) {
      return `endpoint: ${activeGatewayEndpoint}`
    }
    if (activeExecutionId.trim()) return `execution: ${activeExecutionId}`
    return 'target: —'
  }, [activeConnectKind, activeExecutionId, activeGatewayEndpoint])

  const [isConnectOpen, setIsConnectOpen] = useState(false)
  const [connectError, setConnectError] = useState<string>('')
  const [connectLoading, setConnectLoading] = useState(false)
  const [connectTab, setConnectTab] = useState<'execution' | 'endpoint'>('execution')
  const [tasks, setTasks] = useState<TaskSummary[]>([])
  const [instances, setInstances] = useState<InstanceSummary[]>([])
  const [executions, setExecutions] = useState<ExecutionSummary[]>([])
  const [selectedTaskId, setSelectedTaskId] = useState<string>('')
  const [selectedInstanceId, setSelectedInstanceId] = useState<string>('')
  const [selectedExecutionId, setSelectedExecutionId] = useState<string>('')
  const [endpointSearch, setEndpointSearch] = useState<string>('')
  const [selectedGatewayEndpoint, setSelectedGatewayEndpoint] = useState<string>('')
  const [selectedEndpointTaskId, setSelectedEndpointTaskId] = useState<string>('')

  const [renameOpen, setRenameOpen] = useState(false)
  const [renameId, setRenameId] = useState<string>('')
  const [renameValue, setRenameValue] = useState<string>('')

  const [isInfoOpen, setIsInfoOpen] = useState(false)
  const [infoLoading, setInfoLoading] = useState(false)
  const [infoError, setInfoError] = useState<string>('')
  const [infoTask, setInfoTask] = useState<unknown>(null)
  const [infoExecution, setInfoExecution] = useState<unknown>(null)
  const [infoIds, setInfoIds] = useState<{ taskId: string; instanceId: string; executionId: string } | null>(null)

  const disconnectConversation = useCallback((conversationId: string) => {
    const client = clientMapRef.current.get(conversationId)
    if (client) {
      client.disconnect()
      clientMapRef.current.delete(conversationId)
    }
    autoConnectKeyRef.current.delete(conversationId)
    setConversations((prev) =>
      prev.map((c) => {
        if (c.id !== conversationId) return c
        return {
          ...c,
          conn_status: 'disconnected',
          conn_error: '',
          messages: c.messages.map((m) => (m.streaming ? { ...m, streaming: false } : m)),
        }
      }),
    )
  }, [])

  const appendAssistantChunk = useCallback((conversationId: string, msgType: number, meta: string, chunk: string) => {
    if (!chunk) return
    setConversations((prev) =>
      prev.map((c) => {
        if (c.id !== conversationId) return c
        const last = c.messages[c.messages.length - 1]
        if (last && last.role === 'assistant' && last.streaming) {
          return {
            ...c,
            messages: [...c.messages.slice(0, -1), { ...last, text: last.text + chunk }],
          }
        }
        const prefix = msgType || meta ? formatFramePrefix(msgType, meta) : ''
        const text = prefix ? `${prefix}${chunk}` : chunk
        return {
          ...c,
          messages: [
            ...c.messages,
            { id: newId(), role: 'assistant', text, createdAt: Date.now(), streaming: true },
          ],
        }
      }),
    )
  }, [])

  useEffect(() => {
    saveConversations(conversations)
  }, [conversations])

  useEffect(() => {
    if (activeId) {
      saveActiveId(activeId)
    }
  }, [activeId])

  useEffect(() => {
    if (!activeId && conversations.length > 0) {
      setActiveId(conversations[0].id)
    }
  }, [activeId, conversations])

  useEffect(() => {
    localStorage.setItem(`${STORAGE_KEY}.executionId`, activeExecutionId)
  }, [activeExecutionId])

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' })
  }, [activeMessagesLen])

  const updateActive = useCallback((patch: Partial<Conversation>) => {
    setConversations((prev) => prev.map((c) => (c.id === activeId ? { ...c, ...patch } : c)))
  }, [activeId])

  const switchConversation = useCallback((id: string) => {
    setActiveId(id)
  }, [])

  const openConnectDrawer = useCallback(async (init?: { taskId?: string; instanceId?: string }) => {
    setConnectError('')
    setConnectLoading(true)
    setIsConnectOpen(true)
    try {
      const t = await fetchTasks()
      setTasks(t)
      const taskId = init?.taskId?.trim() ?? ''
      const instanceId = init?.instanceId?.trim() ?? ''
      if (taskId) {
        const ins = await fetchTaskInstances(taskId)
        setInstances(ins)
      }
      if (instanceId) {
        const ex = await fetchInstanceExecutions(instanceId)
        setExecutions(ex)
      }
    } catch (e) {
      setConnectError(String(e instanceof Error ? e.message : e))
    } finally {
      setConnectLoading(false)
    }
  }, [])

  const openInfoDialog = useCallback(async (ids: { taskId: string; instanceId: string; executionId: string }) => {
    setInfoIds(ids)
    setIsInfoOpen(true)
    setInfoError('')
    setInfoLoading(true)
    setInfoTask(null)
    setInfoExecution(null)
    try {
      const [task, execution] = await Promise.all([
        ids.taskId ? fetchJson<unknown>(`/api/v1/tasks/${encodeURIComponent(ids.taskId)}`) : Promise.resolve(null),
        ids.executionId
          ? fetchJson<unknown>(`/api/v1/executions/${encodeURIComponent(ids.executionId)}`)
          : Promise.resolve(null),
      ])
      setInfoTask(task)
      setInfoExecution(execution)
    } catch (e) {
      setInfoError(String(e instanceof Error ? e.message : e))
    } finally {
      setInfoLoading(false)
    }
  }, [])

  const connectTo = useCallback(async (
    conversationId: string,
    target: { kind: 'execution'; executionId: string } | { kind: 'endpoint'; gatewayEndpoint: string },
  ) => {
    disconnectConversation(conversationId)
    setConversations((prev) =>
      prev.map((c) => (c.id === conversationId ? { ...c, conn_status: 'connecting', conn_error: '' } : c)),
    )

    let wsUrl = ''
    let subprotocol: string | undefined
    try {
      if (target.kind === 'endpoint') {
        const endpoint = target.gatewayEndpoint.trim()
        if (!endpoint) throw new Error('no endpoint selected')
        wsUrl = new URL(`/e/${encodeURIComponent(endpoint)}/ws`, window.location.origin).toString()
        subprotocol = 'ssf.v1'
      } else {
        const executionId = target.executionId.trim()
        if (!executionId) throw new Error('no execution selected')
        const session = await createStreamSession({ executionId })
        wsUrl = session.ws_url
      }
    } catch (e) {
      setConversations((prev) =>
        prev.map((c) =>
          c.id === conversationId
            ? { ...c, conn_status: 'disconnected', conn_error: String(e instanceof Error ? e.message : e) }
            : c,
        ),
      )
      return
    }
    const client = new SpearStreamClient({
      onOpen: () => {
        if (clientMapRef.current.get(conversationId) !== client) return
        setConversations((prev) =>
          prev.map((c) => (c.id === conversationId ? { ...c, conn_status: 'connected', conn_error: '' } : c)),
        )
      },
      onClose: () => {
        setConversations((prev) =>
          prev.map((c) => {
            if (c.id !== conversationId) return c
            return {
              ...c,
              conn_status: 'disconnected',
              messages: c.messages.map((m) => (m.streaming ? { ...m, streaming: false } : m)),
            }
          }),
        )
      },
      onError: () => {
        if (clientMapRef.current.get(conversationId) !== client) return
        setConversations((prev) =>
          prev.map((c) => (c.id === conversationId ? { ...c, conn_error: 'websocket error' } : c)),
        )
      },
      onFrame: ({ streamId, msgType, data, meta }) => {
        if (clientMapRef.current.get(conversationId) !== client) return
        if (streamId !== DEFAULT_STREAM_ID) return
        const text = decodeAsUtf8(data)
        const metaText = decodeAsUtf8(meta).trim()
        appendAssistantChunk(conversationId, msgType, metaText, text)
      },
    })
    clientMapRef.current.set(conversationId, client)
    try {
      await client.connect(wsUrl, subprotocol ? { subprotocol, autoOpenStreamId: DEFAULT_STREAM_ID } : { autoOpenStreamId: DEFAULT_STREAM_ID })
    } catch (e) {
      if (clientMapRef.current.get(conversationId) === client) {
        clientMapRef.current.delete(conversationId)
      }
      setConversations((prev) =>
        prev.map((c) =>
          c.id === conversationId
            ? { ...c, conn_status: 'disconnected', conn_error: String(e instanceof Error ? e.message : e) }
            : c,
        ),
      )
    }
  }, [appendAssistantChunk, disconnectConversation])

  useEffect(() => {
    // Auto-connect when a target is already selected (show stream output immediately).
    // 若对话已选择 target，则自动建立连接（便于立即看到 stream 输出）。
    if (!activeId || !active) return
    if (active.conn_status !== 'disconnected') return
    if (active.conn_error) return
    if (clientMapRef.current.get(activeId)) return
    const target =
      active.connect_kind === 'endpoint'
        ? active.gatewayEndpoint.trim()
          ? ({ kind: 'endpoint', gatewayEndpoint: active.gatewayEndpoint } as const)
          : null
        : active.executionId.trim()
          ? ({ kind: 'execution', executionId: active.executionId } as const)
          : null
    if (!target) return
    const key =
      target.kind === 'endpoint'
        ? `endpoint:${target.gatewayEndpoint.trim()}`
        : `execution:${target.executionId.trim()}`
    if (autoConnectKeyRef.current.get(activeId) === key) return
    autoConnectKeyRef.current.set(activeId, key)
    void connectTo(activeId, target)
  }, [active, activeId, connectTo])

  const send = useCallback(async () => {
    const text = input.trim()
    if (!text) return
    const conversationId = activeId
    if (!conversationId) return
    const client = clientMapRef.current.get(conversationId)
    if (!client?.isConnected()) {
      setConversations((prev) =>
        prev.map((c) => (c.id === conversationId ? { ...c, conn_error: 'not connected' } : c)),
      )
      return
    }
    setInput('')
    setConversations((prev) =>
      prev.map((c) => {
        if (c.id !== conversationId) return c
        const userMsg: ChatMessage = { id: newId(), role: 'user', text, createdAt: Date.now() }
        const asstMsg: ChatMessage = { id: newId(), role: 'assistant', text: '', createdAt: Date.now(), streaming: true }
        const title = c.title.trim() ? c.title : guessTitleFromText(text)
        return { ...c, title, messages: [...c.messages, userMsg, asstMsg] }
      }),
    )
    client.sendText(DEFAULT_STREAM_ID, text)
    client.sendCommit(DEFAULT_STREAM_ID)
  }, [activeId, input])

  return (
    <div className="cw-page">
      <aside className="cw-sidebar">
        <div className="cw-brand">
          <div className="cw-brandTitle">{APP_TITLE}</div>
          <button
            className="cw-btn cw-btnPrimary"
            onClick={() => {
              const c = newConversation()
              setConversations((prev) => [c, ...prev])
              switchConversation(c.id)
              setConnectTab('execution')
              setEndpointSearch('')
              setSelectedTaskId('')
              setSelectedInstanceId('')
              setSelectedExecutionId('')
              setSelectedGatewayEndpoint('')
              setSelectedEndpointTaskId('')
              void openConnectDrawer()
            }}
          >
            New chat
          </button>
        </div>

        <div className="cw-list">
          {conversations.map((c) => (
            <button
              key={c.id}
              className={c.id === activeId ? 'cw-conv cw-convActive' : 'cw-conv'}
              onClick={() => switchConversation(c.id)}
            >
              <div className="cw-convTitle">{c.title.trim() ? c.title : '(untitled)'}</div>
              <div className="cw-convMeta">
                {c.connect_kind === 'endpoint' && c.gatewayEndpoint
                  ? `endpoint: ${c.gatewayEndpoint}`
                  : c.executionId
                    ? `execution: ${c.executionId}`
                    : 'no target'}
              </div>
              <div className="cw-convActions">
                <button
                  className="cw-iconBtn"
                  onClick={(e) => {
                    e.preventDefault()
                    e.stopPropagation()
                    setRenameId(c.id)
                    setRenameValue(c.title)
                    setRenameOpen(true)
                  }}
                >
                  Rename
                </button>
                <button
                  className="cw-iconBtn cw-danger"
                  onClick={(e) => {
                    e.preventDefault()
                    e.stopPropagation()
                    disconnectConversation(c.id)
                    setConversations((prev) => prev.filter((x) => x.id !== c.id))
                    if (activeId === c.id) {
                      const next = conversations.find((x) => x.id !== c.id)?.id
                      switchConversation(next ?? '')
                    }
                  }}
                >
                  Delete
                </button>
              </div>
            </button>
          ))}
        </div>
      </aside>

      <main className="cw-main">
        <header className="cw-header">
          <div className="cw-headerLeft">
            <div className="cw-headerTitle">{active?.title.trim() ? active.title : 'Chat'}</div>
            <div className="cw-headerMeta">{activeTargetText}</div>
          </div>
          <div className="cw-headerRight">
            <div className="cw-connChip" title={activeTargetText}>
              <span
                className={
                  activeStatus === 'connected'
                    ? 'cw-dot cw-dotOk'
                    : activeStatus === 'connecting'
                      ? 'cw-dot cw-dotWarn'
                      : 'cw-dot'
                }
              />
              <span className="cw-connChipText">
                {activeStatus} • {activeTargetText}
              </span>
            </div>
            {activeError ? <div className="cw-error cw-errorInline">{activeError}</div> : null}
            <button
              className="cw-btn"
              onClick={() => {
                setConnectTab(active?.connect_kind ?? 'execution')
                setEndpointSearch('')
                setSelectedTaskId(active?.taskId ?? '')
                setSelectedInstanceId(active?.instanceId ?? '')
                setSelectedExecutionId(active?.executionId ?? '')
                setSelectedGatewayEndpoint(active?.gatewayEndpoint ?? '')
                setSelectedEndpointTaskId(active?.taskId ?? '')
                void openConnectDrawer({ taskId: active?.taskId ?? '', instanceId: active?.instanceId ?? '' })
              }}
            >
              Connect…
            </button>
            <button className="cw-btn" onClick={() => disconnectConversation(activeId)} disabled={activeStatus === 'disconnected'}>
              Disconnect
            </button>
          </div>
        </header>

        <div className="cw-chat">
          {active?.messages.length ? (
            active.messages.map((m) => (
              <ChatBubble key={m.id} message={m} />
            ))
          ) : (
            <div className="cw-empty">
              <div className="cw-emptyTitle">Connect and start chatting</div>
              <div className="cw-emptyText">
                Connect by execution (stream session) or by endpoint gateway. The chat window stays the same.
              </div>
            </div>
          )}
          <div ref={endRef} />
        </div>

        <div className="cw-composer">
          <textarea
            className="cw-textarea"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={activeStatus === 'connected' ? 'Message…' : 'Connect to send messages…'}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault()
                send()
              }
            }}
          />
          <div className="cw-composerActions">
            <button className="cw-btn cw-btnPrimary" disabled={activeStatus !== 'connected' || !input.trim()} onClick={send}>
              Send
            </button>
            <button className="cw-btn" disabled={activeStatus !== 'connected'} onClick={() => disconnectConversation(activeId)}>
              Stop
            </button>
          </div>
        </div>
      </main>
      {isConnectOpen ? (
        <ConnectDrawer
          tab={connectTab}
          tasks={tasks}
          instances={instances}
          executions={executions}
          selectedTaskId={selectedTaskId}
          selectedInstanceId={selectedInstanceId}
          selectedExecutionId={selectedExecutionId}
          endpointSearch={endpointSearch}
          selectedGatewayEndpoint={selectedGatewayEndpoint}
          loading={connectLoading}
          error={connectError}
          status={activeStatus}
          onChangeTab={setConnectTab}
          onClose={() => {
            setIsConnectOpen(false)
            setConnectError('')
          }}
          onOpenInfo={() =>
            void openInfoDialog({
              taskId: connectTab === 'endpoint' ? selectedEndpointTaskId.trim() : selectedTaskId.trim(),
              instanceId: connectTab === 'endpoint' ? '' : selectedInstanceId.trim(),
              executionId: connectTab === 'endpoint' ? '' : selectedExecutionId.trim(),
            })
          }
          onChangeTask={(taskId) => {
            setSelectedTaskId(taskId)
            setSelectedInstanceId('')
            setSelectedExecutionId('')
            setInstances([])
            setExecutions([])
            if (!taskId.trim()) return
            setConnectError('')
            setConnectLoading(true)
            void fetchTaskInstances(taskId.trim())
              .then((ins) => setInstances(ins))
              .catch((e) => setConnectError(String(e instanceof Error ? e.message : e)))
              .finally(() => setConnectLoading(false))
          }}
          onChangeInstance={(instanceId) => {
            setSelectedInstanceId(instanceId)
            setSelectedExecutionId('')
            setExecutions([])
            if (!instanceId.trim()) return
            setConnectError('')
            setConnectLoading(true)
            void fetchInstanceExecutions(instanceId.trim())
              .then((ex) => setExecutions(ex))
              .catch((e) => setConnectError(String(e instanceof Error ? e.message : e)))
              .finally(() => setConnectLoading(false))
          }}
          onChangeExecution={setSelectedExecutionId}
          onChangeEndpointSearch={setEndpointSearch}
          onSelectEndpoint={(e) => {
            setSelectedGatewayEndpoint(e.gatewayEndpoint)
            setSelectedEndpointTaskId(e.taskId)
          }}
          onConnect={async () => {
            if (!activeId) return
            if (connectTab === 'endpoint') {
              const endpoint = selectedGatewayEndpoint.trim()
              if (!endpoint) {
                setConnectError('Please select an endpoint')
                return
              }
              updateActive({
                connect_kind: 'endpoint',
                taskId: selectedEndpointTaskId.trim(),
                instanceId: '',
                executionId: '',
                gatewayEndpoint: endpoint,
              })
              setIsConnectOpen(false)
              setConnectError('')
              await connectTo(activeId, { kind: 'endpoint', gatewayEndpoint: endpoint })
              return
            }

            const taskId = selectedTaskId.trim()
            const instanceId = selectedInstanceId.trim()
            const executionId = selectedExecutionId.trim()
            if (!executionId) {
              setConnectError('Please select an execution')
              return
            }
            updateActive({ connect_kind: 'execution', taskId, instanceId, executionId, gatewayEndpoint: '' })
            setIsConnectOpen(false)
            setConnectError('')
            await connectTo(activeId, { kind: 'execution', executionId })
          }}
        />
      ) : null}
      {isInfoOpen ? (
        <InfoDialog
          ids={infoIds}
          loading={infoLoading}
          error={infoError}
          task={infoTask}
          execution={infoExecution}
          onClose={() => {
            setIsInfoOpen(false)
            setInfoError('')
            setInfoLoading(false)
          }}
        />
      ) : null}
      {renameOpen ? (
        <RenameDialog
          value={renameValue}
          onChange={setRenameValue}
          onClose={() => setRenameOpen(false)}
          onConfirm={() => {
            const name = renameValue.trim()
            if (!renameId) return
            setConversations((prev) => prev.map((x) => (x.id === renameId ? { ...x, title: name } : x)))
            setRenameOpen(false)
          }}
        />
      ) : null}
    </div>
  )
}

type TaskSummary = { task_id: string; name: string; endpoint?: string }
type InstanceSummary = { instance_id: string; status: string; current_execution_id: string }
type ExecutionSummary = {
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
type ListTaskInstancesResponse = { instances: Array<{ instance_id: string; status: string; current_execution_id: string }>; next_page_token: string }
type ListInstanceExecutionsResponse = { executions: ExecutionSummary[]; next_page_token: string }

async function fetchJson<T>(path: string): Promise<T> {
  const url = new URL(path, window.location.origin)
  const resp = await fetch(url)
  if (!resp.ok) {
    const text = await resp.text().catch(() => '')
    throw new Error(`${resp.status} ${text}`)
  }
  return (await resp.json()) as T
}

async function fetchTasks(): Promise<TaskSummary[]> {
  const r = await fetchJson<ListTasksResponse>('/api/v1/tasks')
  return r.tasks.map((t) => ({
    task_id: t.task_id,
    name: t.name,
    endpoint: t.endpoint,
  }))
}

async function fetchTaskInstances(taskId: string): Promise<InstanceSummary[]> {
  const r = await fetchJson<ListTaskInstancesResponse>(`/api/v1/tasks/${encodeURIComponent(taskId)}/instances?limit=100`)
  return r.instances.map((i) => ({
    instance_id: i.instance_id,
    status: i.status,
    current_execution_id: i.current_execution_id,
  }))
}

async function fetchInstanceExecutions(instanceId: string): Promise<ExecutionSummary[]> {
  const r = await fetchJson<ListInstanceExecutionsResponse>(
    `/api/v1/instances/${encodeURIComponent(instanceId)}/executions?limit=100`,
  )
  return r.executions
}

function ConnectDrawer(props: {
  tab: 'execution' | 'endpoint'
  tasks: TaskSummary[]
  instances: InstanceSummary[]
  executions: ExecutionSummary[]
  selectedTaskId: string
  selectedInstanceId: string
  selectedExecutionId: string
  endpointSearch: string
  selectedGatewayEndpoint: string
  loading: boolean
  error: string
  status: 'disconnected' | 'connecting' | 'connected'
  onChangeTab: (tab: 'execution' | 'endpoint') => void
  onOpenInfo: () => void
  onClose: () => void
  onChangeTask: (taskId: string) => void
  onChangeInstance: (instanceId: string) => void
  onChangeExecution: (executionId: string) => void
  onChangeEndpointSearch: (q: string) => void
  onSelectEndpoint: (e: { taskId: string; gatewayEndpoint: string }) => void
  onConnect: () => void | Promise<void>
}) {
  const endpointItems = useMemo(() => {
    const q = props.endpointSearch.trim().toLowerCase()
    return props.tasks
      .map((t) => ({
        taskId: t.task_id,
        taskName: t.name,
        gatewayEndpoint: (t.endpoint ?? '').trim(),
      }))
      .filter((x) => x.gatewayEndpoint.trim())
      .filter((x) => {
        if (!q) return true
        return x.gatewayEndpoint.toLowerCase().includes(q) || x.taskName.toLowerCase().includes(q)
      })
      .sort((a, b) => a.gatewayEndpoint.localeCompare(b.gatewayEndpoint))
  }, [props.endpointSearch, props.tasks])

  const targetLabel =
    props.tab === 'endpoint'
      ? props.selectedGatewayEndpoint.trim()
        ? `/e/${props.selectedGatewayEndpoint.trim()}/ws`
        : '—'
      : props.selectedExecutionId.trim()
        ? `execution: ${props.selectedExecutionId.trim()}`
        : '—'

  const canConnect =
    !props.loading &&
    props.status !== 'connecting' &&
    (props.tab === 'endpoint' ? !!props.selectedGatewayEndpoint.trim() : !!props.selectedExecutionId.trim())

  return (
    <div
      className="cw-drawerBackdrop"
      role="dialog"
      aria-modal="true"
      onClick={() => props.onClose()}
    >
      <div
        className="cw-drawer"
        onClick={(e) => {
          e.stopPropagation()
        }}
      >
        <div className="cw-drawerHeader">
          <div className="cw-drawerTitle">Connect</div>
          <div className="cw-drawerHeaderRight">
            <button className="cw-iconBtn" onClick={props.onOpenInfo} disabled={props.tab === 'execution' ? !props.selectedExecutionId.trim() : false}>
              Info
            </button>
            <button className="cw-iconBtn" onClick={props.onClose}>
              Close
            </button>
          </div>
        </div>

        <div className="cw-drawerBody">
          <div className="cw-tabs">
            <button
              className={props.tab === 'execution' ? 'cw-tab cw-tabActive' : 'cw-tab'}
              onClick={() => props.onChangeTab('execution')}
            >
              By Execution
            </button>
            <button
              className={props.tab === 'endpoint' ? 'cw-tab cw-tabActive' : 'cw-tab'}
              onClick={() => props.onChangeTab('endpoint')}
            >
              By Endpoint
            </button>
          </div>

          {props.tab === 'execution' ? (
            <div className="cw-drawerSection">
              <div className="cw-modalRow">
                <div className="cw-label">Task</div>
                <select
                  className="cw-select"
                  value={props.selectedTaskId}
                  onChange={(e) => props.onChangeTask(e.target.value)}
                >
                  <option value="">Select a task…</option>
                  {props.tasks.map((t) => (
                    <option key={t.task_id} value={t.task_id}>
                      {t.name} ({t.task_id})
                    </option>
                  ))}
                </select>
              </div>

              <div className="cw-modalRow">
                <div className="cw-label">Instance</div>
                <select
                  className="cw-select"
                  value={props.selectedInstanceId}
                  onChange={(e) => props.onChangeInstance(e.target.value)}
                  disabled={!props.selectedTaskId}
                >
                  <option value="">Select an instance…</option>
                  {props.instances.map((i) => (
                    <option key={i.instance_id} value={i.instance_id}>
                      {i.instance_id} ({i.status})
                    </option>
                  ))}
                </select>
              </div>

              <div className="cw-modalRow">
                <div className="cw-label">Execution</div>
                <select
                  className="cw-select"
                  value={props.selectedExecutionId}
                  onChange={(e) => props.onChangeExecution(e.target.value)}
                  disabled={!props.selectedInstanceId}
                >
                  <option value="">Select an execution…</option>
                  {props.executions.map((x) => (
                    <option key={x.execution_id} value={x.execution_id}>
                      {x.execution_id} ({x.status}) {x.function_name ? `- ${x.function_name}` : ''}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          ) : (
            <div className="cw-drawerSection">
              <div className="cw-modalRow">
                <div className="cw-label">Endpoint</div>
                <input
                  className="cw-textInput"
                  value={props.endpointSearch}
                  onChange={(e) => props.onChangeEndpointSearch(e.target.value)}
                  placeholder="Search gateway_endpoint…"
                />
              </div>

              <div className="cw-endpointList" role="list">
                {endpointItems.length ? (
                  endpointItems.map((it) => (
                    <button
                      key={`${it.taskId}:${it.gatewayEndpoint}`}
                      className={it.gatewayEndpoint === props.selectedGatewayEndpoint ? 'cw-endpointItem cw-endpointItemActive' : 'cw-endpointItem'}
                      onClick={() => props.onSelectEndpoint({ taskId: it.taskId, gatewayEndpoint: it.gatewayEndpoint })}
                      role="listitem"
                    >
                      <div className="cw-endpointLeft">
                        <div className="cw-endpointName">{it.gatewayEndpoint}</div>
                        <div className="cw-endpointMeta">{it.taskName}</div>
                      </div>
                      <div className="cw-endpointRight">{it.gatewayEndpoint === props.selectedGatewayEndpoint ? 'Selected' : ''}</div>
                    </button>
                  ))
                ) : (
                  <div className="cw-modalHint">No endpoints found.</div>
                )}
              </div>
            </div>
          )}

          <div className="cw-drawerSection">
            <div className="cw-label">Connection details</div>
            <div className="cw-connDetails">
              <div className="cw-connDetailsRow">
                <div className="cw-connDetailsKey">Target</div>
                <div className="cw-connDetailsVal">{targetLabel}</div>
              </div>
              <div className="cw-connDetailsRow">
                <div className="cw-connDetailsKey">Protocol</div>
                <div className="cw-connDetailsVal">{props.tab === 'endpoint' ? 'ssf.v1 (binary WS)' : 'stream session (binary WS)'}</div>
              </div>
            </div>
          </div>

          {props.loading ? <div className="cw-modalHint">Loading…</div> : null}
          {props.error ? <div className="cw-error">{props.error}</div> : null}
        </div>

        <div className="cw-drawerFooter">
          <button className="cw-btn" onClick={props.onClose}>
            Cancel
          </button>
          <button className="cw-btn cw-btnPrimary" onClick={props.onConnect} disabled={!canConnect}>
            Connect
          </button>
        </div>
      </div>
    </div>
  )
}

function InfoDialog(props: {
  ids: { taskId: string; instanceId: string; executionId: string } | null
  loading: boolean
  error: string
  task: unknown
  execution: unknown
  onClose: () => void
}) {
  const ids = props.ids ?? { taskId: '', instanceId: '', executionId: '' }
  const taskText = useMemo(() => jsonPretty(props.task), [props.task])
  const executionText = useMemo(() => jsonPretty(props.execution), [props.execution])

  return (
    <div className="cw-modalBackdrop" role="dialog" aria-modal="true">
      <div className="cw-modal">
        <div className="cw-modalHeader">
          <div className="cw-modalTitle">Execution info</div>
          <div className="cw-modalHeaderRight">
            <button className="cw-iconBtn" onClick={props.onClose}>
              Close
            </button>
          </div>
        </div>
        <div className="cw-modalBody">
          <div className="cw-modalRow">
            <div className="cw-label">Task</div>
            <div className="cw-readonly">{ids.taskId || 'n/a'}</div>
          </div>
          <div className="cw-modalRow">
            <div className="cw-label">Instance</div>
            <div className="cw-readonly">{ids.instanceId || 'n/a'}</div>
          </div>
          <div className="cw-modalRow">
            <div className="cw-label">Execution</div>
            <div className="cw-readonly">{ids.executionId || 'n/a'}</div>
          </div>
          {props.loading ? <div className="cw-modalHint">Loading…</div> : null}
          {props.error ? <div className="cw-error">{props.error}</div> : null}
          <div className="cw-modalRow">
            <div className="cw-label">Task detail</div>
            <div className="cw-codeWrap">
              <div className="cw-codeHeader">
                <div className="cw-codeLang">json</div>
                <button className="cw-codeBtn" onClick={() => navigator.clipboard?.writeText(taskText).catch(() => {})}>
                  Copy
                </button>
              </div>
              <pre className="cw-code">
                <code>{taskText}</code>
              </pre>
            </div>
          </div>
          <div className="cw-modalRow">
            <div className="cw-label">Execution detail</div>
            <div className="cw-codeWrap">
              <div className="cw-codeHeader">
                <div className="cw-codeLang">json</div>
                <button
                  className="cw-codeBtn"
                  onClick={() => navigator.clipboard?.writeText(executionText).catch(() => {})}
                >
                  Copy
                </button>
              </div>
              <pre className="cw-code">
                <code>{executionText}</code>
              </pre>
            </div>
          </div>
        </div>
        <div className="cw-modalFooter">
          <button className="cw-btn cw-btnPrimary" onClick={props.onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  )
}

function jsonPretty(v: unknown): string {
  if (v == null) return 'null'
  try {
    return JSON.stringify(v, null, 2)
  } catch {
    return String(v)
  }
}

function RenameDialog(props: {
  value: string
  onChange: (v: string) => void
  onClose: () => void
  onConfirm: () => void
}) {
  return (
    <div className="cw-modalBackdrop" role="dialog" aria-modal="true">
      <div className="cw-modal">
        <div className="cw-modalHeader">
          <div className="cw-modalTitle">Rename chat</div>
          <div className="cw-modalHeaderRight">
            <button className="cw-iconBtn" onClick={props.onClose}>
              Close
            </button>
          </div>
        </div>
        <div className="cw-modalBody">
          <div className="cw-modalRow">
            <div className="cw-label">Title</div>
            <input
              className="cw-textInput"
              value={props.value}
              onChange={(e) => props.onChange(e.target.value)}
              autoFocus
              placeholder="Chat title"
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault()
                  props.onConfirm()
                }
              }}
            />
          </div>
        </div>
        <div className="cw-modalFooter">
          <button className="cw-btn" onClick={props.onClose}>
            Cancel
          </button>
          <button className="cw-btn cw-btnPrimary" onClick={props.onConfirm} disabled={!props.value.trim()}>
            Save
          </button>
        </div>
      </div>
    </div>
  )
}

function ChatBubble(props: { message: ChatMessage }) {
  const { message } = props
  const isUser = message.role === 'user'
  const parts = useMemo(() => splitCodeBlocks(message.text), [message.text])
  return (
    <div className={isUser ? 'cw-row cw-rowUser' : 'cw-row'}>
      <div className={isUser ? 'cw-bubble cw-bubbleUser' : 'cw-bubble'}>
        {parts.map((p, idx) => {
          if (p.type === 'code') {
            return (
              <div key={idx} className="cw-codeWrap">
                <div className="cw-codeHeader">
                  <div className="cw-codeLang">{p.lang ?? ''}</div>
                  <button
                    className="cw-codeBtn"
                    onClick={() => navigator.clipboard?.writeText(p.code).catch(() => {})}
                  >
                    Copy
                  </button>
                </div>
                <pre className="cw-code">
                  <code>{p.code}</code>
                </pre>
              </div>
            )
          }
          return (
            <div key={idx} className="cw-text">
              {p.text}
            </div>
          )
        })}
      </div>
    </div>
  )
}

function decodeAsUtf8(data: Uint8Array): string {
  try {
    return new TextDecoder().decode(data)
  } catch {
    return ''
  }
}

function formatFramePrefix(msgType: number, metaText: string): string {
  if (!metaText && !msgType) return ''
  if (!metaText) return `[type=${msgType}] `
  return `[type=${msgType} meta=${metaText}] `
}

function newConversation(init?: {
  connect_kind?: 'execution' | 'endpoint'
  taskId?: string
  instanceId?: string
  executionId?: string
  gatewayEndpoint?: string
}): Conversation {
  return {
    id: newId(),
    title: '',
    connect_kind: init?.connect_kind ?? 'execution',
    taskId: init?.taskId ?? '',
    instanceId: init?.instanceId ?? '',
    executionId: init?.executionId ?? localStorage.getItem(`${STORAGE_KEY}.executionId`) ?? '',
    gatewayEndpoint: init?.gatewayEndpoint ?? '',
    conn_status: 'disconnected',
    conn_error: '',
    createdAt: Date.now(),
    messages: [],
  }
}

function loadConversations(): Conversation[] {
  const raw = localStorage.getItem(`${STORAGE_KEY}.conversations`)
  if (!raw) return []
  try {
    const parsed = JSON.parse(raw) as Array<Partial<Conversation>>
    if (!Array.isArray(parsed) || !parsed.length) return []
    return parsed.map((c) => ({
      id: c.id ?? newId(),
      title: c.title ?? '',
      connect_kind:
        c.connect_kind === 'endpoint' || c.connect_kind === 'execution'
          ? c.connect_kind
          : (c as unknown as { executionId?: string }).executionId
            ? 'execution'
            : (c as unknown as { gatewayEndpoint?: string }).gatewayEndpoint
              ? 'endpoint'
              : 'execution',
      taskId: (c as unknown as { taskId?: string }).taskId ?? '',
      instanceId: (c as unknown as { instanceId?: string }).instanceId ?? '',
      executionId: c.executionId ?? '',
      gatewayEndpoint: (c as unknown as { gatewayEndpoint?: string }).gatewayEndpoint ?? '',
      conn_status: 'disconnected',
      conn_error: '',
      createdAt: c.createdAt ?? Date.now(),
      messages: Array.isArray(c.messages) ? c.messages : [],
    }))
  } catch {
    return []
  }
}

function saveConversations(conversations: Conversation[]) {
  localStorage.setItem(`${STORAGE_KEY}.conversations`, JSON.stringify(conversations))
}

function loadActiveId(): string | null {
  return localStorage.getItem(`${STORAGE_KEY}.activeId`)
}

function saveActiveId(id: string) {
  localStorage.setItem(`${STORAGE_KEY}.activeId`, id)
}

function guessTitleFromText(text: string): string {
  const t = text.replace(/\s+/g, ' ').trim()
  if (!t) return ''
  if (t.length <= 40) return t
  return t.slice(0, 40) + '…'
}

function newId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`
}

type TextPart = { type: 'text'; text: string }
type CodePart = { type: 'code'; code: string; lang?: string }

function splitCodeBlocks(input: string): Array<TextPart | CodePart> {
  const out: Array<TextPart | CodePart> = []
  const re = /```([a-zA-Z0-9_-]+)?\n([\s\S]*?)```/g
  let last = 0
  for (;;) {
    const m = re.exec(input)
    if (!m) break
    const idx = m.index
    if (idx > last) {
      out.push({ type: 'text', text: input.slice(last, idx) })
    }
    out.push({ type: 'code', lang: m[1], code: m[2] })
    last = idx + m[0].length
  }
  if (last < input.length) {
    out.push({ type: 'text', text: input.slice(last) })
  }
  if (!out.length) out.push({ type: 'text', text: input })
  return out
}
