// SPEAR Console main page.
// SPEAR Console 主页面。

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { type VoiceConfig } from './voice'
import {
  type ExecutionSummary,
  type InstanceSummary,
  type TaskSummary,
  fetchInstanceExecutions,
  fetchJson,
  fetchTaskInstances,
  fetchTasks,
} from './api/spearApi'
import { useConversationStore } from './hooks/useConversationStore'
import { ChatHeader } from './components/ChatHeader'
import { ChatSidebar } from './components/ChatSidebar'
import { ChatView } from './components/ChatView'
import { Composer } from './components/Composer'
import { ConnectDrawer } from './components/ConnectDrawer'
import { InfoDialog } from './components/InfoDialog'
import { Inspector } from './components/Inspector'
import { RenameDialog } from './components/RenameDialog'
import { SettingsDialog } from './components/SettingsDialog'
import { useSpearConnection } from './hooks/useSpearConnection'
import { useTheme } from './hooks/useTheme'
import { useVoiceController } from './hooks/useVoiceController'

const APP_TITLE = 'SPEAR Console'

export default function App() {
  const pageRef = useRef<HTMLDivElement | null>(null)
  const [input, setInput] = useState<string>('')
  const {
    conversations,
    active,
    activeId,
    switchConversation,
    createAndActivateConversation,
    updateConversation,
    patchConversation,
    patchActiveConversation,
    renameConversation,
    removeConversation,
  } = useConversationStore()

  const endRef = useRef<HTMLDivElement | null>(null)
  const prevActiveConnRef = useRef<{ conversationId: string; status: string } | null>(null)

  const voiceConfigRef = useRef<VoiceConfig>({ sampleRateHz: 16000, channels: 1, chunkMs: 300 })
  const getVoiceConfig = useCallback(() => voiceConfigRef.current, [])
  const beforeDisconnectRef = useRef<(conversationId: string) => void>(() => {})
  const conn = useSpearConnection({
    getVoiceConfig,
    updateConversation,
    patchConversation,
    onBeforeDisconnect: (id) => beforeDisconnectRef.current(id),
  })
  const voice = useVoiceController({
    activeId,
    getClient: conn.getClient,
    getVoiceConfig,
  })
  const [sidebarMode, setSidebarMode] = useState<'expanded' | 'collapsed'>('expanded')
  const [inspectorMode, setInspectorMode] = useState<'expanded' | 'collapsed'>('expanded')
  const [sidebarWidth, setSidebarWidth] = useState(292)
  const [inspectorWidth, setInspectorWidth] = useState(276)
  const [resizingPane, setResizingPane] = useState<'sidebar' | 'inspector' | null>(null)
  const theme = useTheme()
  const [settingsOpen, setSettingsOpen] = useState(false)

  const activeExecutionId = active?.executionId ?? ''
  const activeGatewayEndpoint = active?.gatewayEndpoint ?? ''
  const activeConnectKind = active?.connect_kind ?? 'execution'
  const activeMessages = active?.messages ?? []
  const activeMessagesLen = active?.messages.length ?? 0
  const activeStatus = active?.conn_status ?? 'disconnected'
  const activeError = active?.conn_error ?? ''

  useEffect(() => {
    beforeDisconnectRef.current = () => {
      void voice.stopVoice()
    }
  }, [voice.stopVoice])

  useEffect(() => {
    const prev = prevActiveConnRef.current
    const current = activeId
      ? {
          conversationId: activeId,
          status: activeStatus,
        }
      : null

    const shouldStopVoice =
      !!prev &&
      !!current &&
      prev.conversationId === current.conversationId &&
      prev.status !== 'disconnected' &&
      current.status === 'disconnected' &&
      voice.voiceRecording

    prevActiveConnRef.current = current

    if (!shouldStopVoice) return

    console.warn('[voice] stopping open mic because stream disconnected', {
      conversationId: activeId,
      previousStatus: prev?.status,
      currentStatus: activeStatus,
    })
    void voice.stopVoice()
  }, [activeId, activeStatus, voice.stopVoice, voice.voiceRecording])

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

  const resetConnectSelection = useCallback(() => {
    setConnectTab('execution')
    setEndpointSearch('')
    setSelectedTaskId('')
    setSelectedInstanceId('')
    setSelectedExecutionId('')
    setSelectedGatewayEndpoint('')
    setSelectedEndpointTaskId('')
  }, [])

  const createBlankChat = useCallback(() => {
    createAndActivateConversation({ executionId: '' })
  }, [createAndActivateConversation])

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: 'end' })
  }, [activeMessagesLen])

  useEffect(() => {
    if (!resizingPane) return

    function onMouseMove(event: MouseEvent) {
      const pageRect = pageRef.current?.getBoundingClientRect()
      if (!pageRect) return

      if (resizingPane === 'sidebar') {
        const nextWidth = Math.min(360, Math.max(240, event.clientX - pageRect.left))
        setSidebarWidth(nextWidth)
        return
      }

      const nextWidth = Math.min(360, Math.max(240, pageRect.right - event.clientX))
      setInspectorWidth(nextWidth)
    }

    function onMouseUp() {
      setResizingPane(null)
    }

    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [resizingPane])

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

  const createChatAndOpenConnect = useCallback(() => {
    createAndActivateConversation({ executionId: '' })
    resetConnectSelection()
    void openConnectDrawer()
  }, [createAndActivateConversation, openConnectDrawer, resetConnectSelection])

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

  useEffect(() => {
    if (!activeId || !active) return
    if (active.conn_status !== 'disconnected') return
    if (active.conn_error) return
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
    if (!conn.markAutoConnectKey(activeId, key)) return
    void conn.connectTo(activeId, target)
  }, [active, activeId, conn])

  const send = useCallback(() => {
    const text = input.trim()
    if (!text) return
    const conversationId = activeId
    if (!conversationId) return
    if (!conn.sendUserText(conversationId, text)) return
    setInput('')
  }, [activeId, conn, input])

  const disconnectConversation = useCallback(
    (conversationId: string) => {
      void voice.stopVoice()
      conn.disconnectConversation(conversationId)
    },
    [conn, voice.stopVoice],
  )

  return (
    <div className={resizingPane ? 'cw-page cw-pageResizing' : 'cw-page'} ref={pageRef}>
      <div
        className="cw-sidebarShell"
        style={
          sidebarMode === 'expanded'
            ? { width: `${sidebarWidth}px`, minWidth: `${sidebarWidth}px` }
            : undefined
        }
      >
        <ChatSidebar
          mode={sidebarMode}
          appTitle={APP_TITLE}
          conversations={conversations}
          activeId={activeId}
          onToggle={() => setSidebarMode((m) => (m === 'expanded' ? 'collapsed' : 'expanded'))}
          onNewChat={createBlankChat}
          onOpenSettings={() => setSettingsOpen(true)}
          onSwitch={switchConversation}
          onRename={(id, title) => {
            setRenameId(id)
            setRenameValue(title)
            setRenameOpen(true)
          }}
          onDelete={(id) => {
            disconnectConversation(id)
            removeConversation(id)
          }}
        />
        {sidebarMode === 'expanded' ? (
          <div
            className="cw-resizeHandle cw-resizeHandleSidebar"
            onMouseDown={() => setResizingPane('sidebar')}
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize left sidebar"
          />
        ) : null}
      </div>

      <main className="cw-main">
        <ChatHeader
          title={active?.title.trim() ? active.title : 'Chat'}
          targetText={activeTargetText}
          status={activeStatus}
          error={activeError}
          hasActiveChat={!!active}
          onOpenConnect={() => {
            setConnectTab(active?.connect_kind ?? 'execution')
            setEndpointSearch('')
            setSelectedTaskId(active?.taskId ?? '')
            setSelectedInstanceId(active?.instanceId ?? '')
            setSelectedExecutionId(active?.executionId ?? '')
            setSelectedGatewayEndpoint(active?.gatewayEndpoint ?? '')
            setSelectedEndpointTaskId(active?.taskId ?? '')
            void openConnectDrawer({ taskId: active?.taskId ?? '', instanceId: active?.instanceId ?? '' })
          }}
          onDisconnect={() => disconnectConversation(activeId)}
        />

        <ChatView
          hasActiveChat={!!active}
          messages={activeMessages}
          endRef={endRef}
          onNewChat={createBlankChat}
          onNewChatAndConnect={createChatAndOpenConnect}
        />

        <Composer
          status={activeStatus}
          input={input}
          onChangeInput={setInput}
          onSend={send}
          micMode={voice.micMode}
          onChangeMicMode={voice.setMicMode}
          voiceRecording={voice.voiceRecording}
          onHoldStart={() => void voice.startHold()}
          onHoldEnd={() => void voice.endHold()}
          onToggleMic={() => {
            void voice.toggleOpenMic()
          }}
          voiceError={voice.voiceError}
          showCancel={activeMessages.some((m) => m.role === 'assistant' && m.streaming)}
          onCancel={() => disconnectConversation(activeId)}
        />
      </main>
      <div
        className="cw-inspectorShell"
        style={
          inspectorMode === 'expanded'
            ? { width: `${inspectorWidth}px`, minWidth: `${inspectorWidth}px` }
            : undefined
        }
      >
        {inspectorMode === 'expanded' ? (
          <div
            className="cw-resizeHandle cw-resizeHandleInspector"
            onMouseDown={() => setResizingPane('inspector')}
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize right sidebar"
          />
        ) : null}
        <Inspector
          mode={inspectorMode}
          onToggle={() => setInspectorMode((m) => (m === 'expanded' ? 'collapsed' : 'expanded'))}
          active={active ?? null}
          targetText={activeTargetText}
          status={activeStatus}
          error={activeError}
          micMode={voice.micMode}
          voiceRecording={voice.voiceRecording}
          voiceError={voice.voiceError}
        />
      </div>
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
              patchActiveConversation({
                connect_kind: 'endpoint',
                taskId: selectedEndpointTaskId.trim(),
                instanceId: '',
                executionId: '',
                gatewayEndpoint: endpoint,
              })
              setIsConnectOpen(false)
              setConnectError('')
              await conn.connectTo(activeId, { kind: 'endpoint', gatewayEndpoint: endpoint })
              return
            }

            const taskId = selectedTaskId.trim()
            const instanceId = selectedInstanceId.trim()
            const executionId = selectedExecutionId.trim()
            if (!executionId) {
              setConnectError('Please select an execution')
              return
            }
            patchActiveConversation({
              connect_kind: 'execution',
              taskId,
              instanceId,
              executionId,
              gatewayEndpoint: '',
            })
            setIsConnectOpen(false)
            setConnectError('')
            await conn.connectTo(activeId, { kind: 'execution', executionId })
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
            renameConversation(renameId, name)
            setRenameOpen(false)
          }}
        />
      ) : null}
      {settingsOpen ? (
        <SettingsDialog
          theme={theme.theme}
          onChangeTheme={theme.setTheme}
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}
    </div>
  )
}
