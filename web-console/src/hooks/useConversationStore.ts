import { useCallback, useEffect, useMemo, useState } from 'react'

import {
  type Conversation,
  type ConversationInit,
  createConversation,
} from '../models/conversation'

const STORAGE_KEY = 'spear.console.v1'

function loadSavedExecutionId(): string {
  return localStorage.getItem(`${STORAGE_KEY}.executionId`) ?? ''
}

function loadConversations(): Conversation[] {
  const raw = localStorage.getItem(`${STORAGE_KEY}.conversations`)
  if (!raw) return []
  try {
    const parsed = JSON.parse(raw) as Array<Partial<Conversation>>
    if (!Array.isArray(parsed) || !parsed.length) return []
    return parsed.map((c) => ({
      id: c.id ?? crypto.randomUUID?.() ?? `${Date.now()}`,
      title: c.title ?? '',
      connect_kind:
        c.connect_kind === 'endpoint' || c.connect_kind === 'execution'
          ? c.connect_kind
          : (c as { executionId?: string }).executionId
            ? 'execution'
            : (c as { gatewayEndpoint?: string }).gatewayEndpoint
              ? 'endpoint'
              : 'execution',
      taskId: (c as { taskId?: string }).taskId ?? '',
      instanceId: (c as { instanceId?: string }).instanceId ?? '',
      executionId: c.executionId ?? '',
      gatewayEndpoint: (c as { gatewayEndpoint?: string }).gatewayEndpoint ?? '',
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

export function useConversationStore() {
  const [conversations, setConversations] = useState<Conversation[]>(() => loadConversations())
  const [activeId, setActiveId] = useState<string>(() => loadActiveId() ?? '')

  const active = useMemo(
    () => conversations.find((c) => c.id === activeId) ?? conversations[0],
    [activeId, conversations],
  )

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
    if (active?.executionId) {
      localStorage.setItem(`${STORAGE_KEY}.executionId`, active.executionId)
    }
  }, [active?.executionId])

  const switchConversation = useCallback((id: string) => {
    setActiveId(id)
  }, [])

  const createAndActivateConversation = useCallback((init?: ConversationInit) => {
    const conversation = createConversation(init, { executionId: loadSavedExecutionId() })
    setConversations((prev) => [conversation, ...prev])
    setActiveId(conversation.id)
    return conversation
  }, [])

  const updateConversation = useCallback(
    (conversationId: string, updater: (conversation: Conversation) => Conversation) => {
      setConversations((prev) =>
        prev.map((conversation) =>
          conversation.id === conversationId ? updater(conversation) : conversation,
        ),
      )
    },
    [],
  )

  const patchConversation = useCallback(
    (conversationId: string, patch: Partial<Conversation>) => {
      updateConversation(conversationId, (conversation) => ({
        ...conversation,
        ...patch,
      }))
    },
    [updateConversation],
  )

  const patchActiveConversation = useCallback(
    (patch: Partial<Conversation>) => {
      if (!activeId) return
      patchConversation(activeId, patch)
    },
    [activeId, patchConversation],
  )

  const renameConversation = useCallback(
    (conversationId: string, title: string) => {
      patchConversation(conversationId, { title })
    },
    [patchConversation],
  )

  const removeConversation = useCallback((conversationId: string) => {
    setConversations((prev) => {
      const next = prev.filter((conversation) => conversation.id !== conversationId)
      setActiveId((current) =>
        current === conversationId ? (next[0]?.id ?? '') : current,
      )
      return next
    })
  }, [])

  return {
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
  }
}
