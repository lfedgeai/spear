import { useCallback, useRef } from 'react'
import { SpearStreamClient, createStreamSession, sameOriginWebSocketUrl } from '../spearStream'
import { type ChatMessage, guessTitleFromText, newId } from '../models/conversation'
import { decodeAsUtf8, formatFramePrefix } from '../utils/text'
import { SsfMsgType } from '../ssf'
import type { AudioOpenMetaV1, TextCommitMetaV1, TextDataMetaV1, TextOpenMetaV1, VoiceConfig } from '../voice'

const DEFAULT_STREAM_ID = 1
const VOICE_STREAM_ID = 2

type ConnectionTarget =
  | { kind: 'execution'; executionId: string }
  | { kind: 'endpoint'; gatewayEndpoint: string }

export function useSpearConnection(params: {
  getVoiceConfig: () => VoiceConfig
  updateConversation: (
    id: string,
    fn: (c: {
      title: string
      messages: ChatMessage[]
      conn_status?: 'disconnected' | 'connecting' | 'connected'
      conn_error?: string
    }) => any,
  ) => void
  patchConversation: (id: string, patch: Partial<{ conn_status: 'disconnected' | 'connecting' | 'connected'; conn_error: string }>) => void
  onBeforeDisconnect?: (conversationId: string) => void
}) {
  const clientMapRef = useRef<Map<string, SpearStreamClient>>(new Map())
  const autoConnectKeyRef = useRef<Map<string, string>>(new Map())

  const appendAssistantChunk = useCallback(
    (conversationId: string, msgType: number, meta: string, chunk: string) => {
      if (!chunk) return
      params.updateConversation(conversationId, (conversation) => {
        const last = conversation.messages[conversation.messages.length - 1]
        if (last && last.role === 'assistant' && last.streaming) {
          return {
            ...conversation,
            messages: [
              ...conversation.messages.slice(0, -1),
              { ...last, text: last.text + chunk },
            ],
          }
        }
        const prefix = msgType || meta ? formatFramePrefix(msgType, meta) : ''
        const text = prefix ? `${prefix}${chunk}` : chunk
        return {
          ...conversation,
          messages: [
            ...conversation.messages,
            { id: newId(), role: 'assistant', text, createdAt: Date.now(), streaming: true },
          ],
        }
      })
    },
    [params],
  )

  const disconnectConversation = useCallback(
    (conversationId: string) => {
      params.onBeforeDisconnect?.(conversationId)
      const client = clientMapRef.current.get(conversationId)
      if (client) {
        client.disconnect()
        clientMapRef.current.delete(conversationId)
      }
      params.updateConversation(conversationId, (conversation) => ({
        ...conversation,
        conn_status: 'disconnected',
        conn_error: '',
        messages: conversation.messages.map((m) => (m.streaming ? { ...m, streaming: false } : m)),
      }))
    },
    [params],
  )

  const getClient = useCallback((conversationId: string) => clientMapRef.current.get(conversationId), [])
  const isConnected = useCallback((conversationId: string) => !!clientMapRef.current.get(conversationId)?.isConnected(), [])

  const markAutoConnectKey = useCallback((conversationId: string, key: string) => {
    if (autoConnectKeyRef.current.get(conversationId) === key) return false
    autoConnectKeyRef.current.set(conversationId, key)
    return true
  }, [])

  const connectTo = useCallback(
    async (conversationId: string, target: ConnectionTarget) => {
      disconnectConversation(conversationId)
      params.patchConversation(conversationId, { conn_status: 'connecting', conn_error: '' })

      let wsUrl = ''
      let subprotocol: string | undefined
      try {
        if (target.kind === 'endpoint') {
          const endpoint = target.gatewayEndpoint.trim()
          if (!endpoint) throw new Error('no endpoint selected')
          wsUrl = sameOriginWebSocketUrl(`/e/${encodeURIComponent(endpoint)}/ws`)
          subprotocol = 'ssf.v1'
        } else {
          const executionId = target.executionId.trim()
          if (!executionId) throw new Error('no execution selected')
          const session = await createStreamSession({ executionId })
          wsUrl = session.ws_url
        }
      } catch (e) {
        params.patchConversation(conversationId, {
          conn_status: 'disconnected',
          conn_error: String(e instanceof Error ? e.message : e),
        })
        return
      }

      const client = new SpearStreamClient({
        onOpen: () => {
          if (clientMapRef.current.get(conversationId) !== client) return
          console.info('[spear-conn] ws open', { conversationId, target, wsUrl })
          params.patchConversation(conversationId, { conn_status: 'connected', conn_error: '' })
        },
        onClose: (ev) => {
          console.warn('[spear-conn] ws close', {
            conversationId,
            target,
            wsUrl,
            code: ev.code,
            reason: ev.reason,
            wasClean: ev.wasClean,
          })
          params.updateConversation(conversationId, (conversation) => ({
            ...conversation,
            conn_status: 'disconnected',
            messages: conversation.messages.map((m) => (m.streaming ? { ...m, streaming: false } : m)),
          }))
        },
        onError: (ev) => {
          if (clientMapRef.current.get(conversationId) !== client) return
          console.warn('[spear-conn] ws error', { conversationId, target, wsUrl, eventType: ev.type })
          params.patchConversation(conversationId, { conn_error: 'websocket error' })
        },
        onFrame: ({ streamId, msgType, data, meta }) => {
          if (clientMapRef.current.get(conversationId) !== client) return
          if (streamId !== DEFAULT_STREAM_ID && streamId !== VOICE_STREAM_ID) return
          if (msgType === SsfMsgType.COMMIT) {
            params.updateConversation(conversationId, (conversation) => {
              const last = conversation.messages[conversation.messages.length - 1]
              if (last && last.role === 'assistant' && last.streaming) {
                return {
                  ...conversation,
                  messages: [...conversation.messages.slice(0, -1), { ...last, streaming: false }],
                }
              }
              return conversation
            })
            return
          }
          const text = decodeAsUtf8(data)
          const metaText = decodeAsUtf8(meta).trim()
          appendAssistantChunk(conversationId, msgType, metaText, text)
        },
      })

      clientMapRef.current.set(conversationId, client)
      try {
        const now = Date.now()
        const voiceCfg = params.getVoiceConfig()
        const textOpen: TextOpenMetaV1 = { v: 1, kind: 'open', modality: 'text', encoding: 'utf-8', ts_ms: now }
        const audioOpen: AudioOpenMetaV1 = {
          v: 1,
          kind: 'open',
          modality: 'audio',
          audio: { format: 'pcm_s16le', sample_rate_hz: voiceCfg.sampleRateHz, channels: voiceCfg.channels },
          ts_ms: now,
        }
        const autoOpenStreams = [
          { streamId: DEFAULT_STREAM_ID, meta: textOpen },
          { streamId: VOICE_STREAM_ID, meta: audioOpen },
        ]
        await client.connect(wsUrl, subprotocol ? { subprotocol, autoOpenStreams } : { autoOpenStreams })
      } catch (e) {
        if (clientMapRef.current.get(conversationId) === client) {
          clientMapRef.current.delete(conversationId)
        }
        params.patchConversation(conversationId, {
          conn_status: 'disconnected',
          conn_error: String(e instanceof Error ? e.message : e),
        })
      }
    },
    [appendAssistantChunk, disconnectConversation, params],
  )

  const sendUserText = useCallback(
    (conversationId: string, text: string) => {
      const client = clientMapRef.current.get(conversationId)
      if (!client?.isConnected()) {
        params.patchConversation(conversationId, { conn_error: 'not connected' })
        return false
      }
      params.updateConversation(conversationId, (conversation) => {
        const userMsg: ChatMessage = { id: newId(), role: 'user', text, createdAt: Date.now() }
        const asstMsg: ChatMessage = { id: newId(), role: 'assistant', text: '', createdAt: Date.now(), streaming: true }
        const title = conversation.title.trim() ? conversation.title : guessTitleFromText(text)
        return { ...conversation, title, messages: [...conversation.messages, userMsg, asstMsg] }
      })
      const messageId = newId()
      const now = Date.now()
      const dataMeta: TextDataMetaV1 = { v: 1, message_id: messageId, seq_in_message: 1, ts_ms: now }
      const commitMeta: TextCommitMetaV1 = { v: 1, message_id: messageId, ts_ms: now }
      client.sendText(DEFAULT_STREAM_ID, text, dataMeta)
      client.sendCommit(DEFAULT_STREAM_ID, commitMeta)
      return true
    },
    [params],
  )

  return {
    connectTo,
    disconnectConversation,
    getClient,
    isConnected,
    markAutoConnectKey,
    sendUserText,
  }
}
