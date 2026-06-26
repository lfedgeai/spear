import { useCallback, useRef, useState } from 'react'
import { createMicCapture, createVoiceStreamSender, type MicMode, type VoiceConfig, type VoiceSession } from '../voice'
import { newId } from '../models/conversation'
import type { SpearStreamClient } from '../spearStream'

const VOICE_STREAM_ID = 2

export function useVoiceController(params: {
  activeId: string
  getClient: (conversationId: string) => SpearStreamClient | undefined
  getVoiceConfig: () => VoiceConfig
}) {
  const micRef = useRef<ReturnType<typeof createMicCapture> | null>(null)
  const voiceSenderRef = useRef<ReturnType<typeof createVoiceStreamSender> | null>(null)
  const voiceSessionRef = useRef<VoiceSession | null>(null)
  const voiceUnsubRef = useRef<(() => void) | null>(null)
  const voiceOpTokenRef = useRef(0)

  const [voiceRecording, setVoiceRecording] = useState(false)
  const [voiceError, setVoiceError] = useState<string>('')
  const [micMode, setMicMode] = useState<MicMode>('hold')

  const stopVoice = useCallback(async () => {
    voiceOpTokenRef.current++
    voiceUnsubRef.current?.()
    voiceUnsubRef.current = null

    const session = voiceSessionRef.current
    voiceSessionRef.current = null

    setVoiceRecording(false)
    setVoiceError('')

    try {
      await micRef.current?.stop()
    } catch {
      // ignore
    }

    micRef.current = null
    voiceSenderRef.current = null
    return session
  }, [])

  const start = useCallback(async () => {
    setVoiceError('')
    const conversationId = params.activeId
    if (!conversationId) return
    const client = params.getClient(conversationId)
    if (!client?.isConnected()) {
      setVoiceError('not connected')
      return
    }
    if (voiceRecording) return

    const opToken = voiceOpTokenRef.current + 1
    voiceOpTokenRef.current = opToken

    setVoiceRecording(true)
    const cfg = params.getVoiceConfig()
    const mic = createMicCapture(cfg)
    micRef.current = mic
    const sender = createVoiceStreamSender({ client, streamId: VOICE_STREAM_ID, config: cfg, assumeStreamOpened: true })
    voiceSenderRef.current = sender

    const utteranceId = newId()
    const session = sender.beginUtterance(Date.now(), utteranceId)
    voiceSessionRef.current = session

    voiceUnsubRef.current = mic.onChunk((c) => {
      try {
        sender.sendChunk(session, c)
      } catch {
        // ignore
      }
    })

    try {
      await mic.start()
      if (voiceOpTokenRef.current !== opToken) {
        await mic.stop().catch(() => {})
        return
      }
    } catch (e) {
      await stopVoice()
      setVoiceError(String(e instanceof Error ? e.message : e))
    }
  }, [params, stopVoice, voiceRecording])

  const end = useCallback(async () => {
    const session = voiceSessionRef.current
    const sender = voiceSenderRef.current
    await stopVoice()
    if (!session || !sender) return
    try {
      sender.commitUtterance(session, Date.now())
    } catch (e) {
      setVoiceError(String(e instanceof Error ? e.message : e))
    }
  }, [stopVoice])

  const toggle = useCallback(async () => {
    if (voiceRecording) await end()
    else await start()
  }, [end, start, voiceRecording])

  return {
    micMode,
    setMicMode,
    voiceRecording,
    voiceError,
    startHold: start,
    endHold: end,
    toggleOpenMic: toggle,
    stopVoice,
  }
}

