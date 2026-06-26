// Press-and-hold to talk button (browser).
// 按住说话按钮（浏览器）。

import { useCallback, useRef } from 'react'

export type PressToTalkButtonProps = {
  disabled: boolean
  recording: boolean
  onPressStart: () => void
  onPressEnd: () => void
}

export function PressToTalkButton(props: PressToTalkButtonProps) {
  const pressedRef = useRef(false)

  const end = useCallback(() => {
    if (!pressedRef.current) return
    pressedRef.current = false
    props.onPressEnd()
  }, [props])

  return (
    <button
      className={props.recording ? 'cw-btn cw-btnPrimary' : 'cw-btn'}
      disabled={props.disabled}
      onPointerDown={(e) => {
        if (props.disabled) return
        e.preventDefault()
        pressedRef.current = true
        props.onPressStart()
      }}
      onPointerUp={(e) => {
        e.preventDefault()
        end()
      }}
      onPointerCancel={(e) => {
        e.preventDefault()
        end()
      }}
      onPointerLeave={(e) => {
        e.preventDefault()
        end()
      }}
    >
      {props.recording ? 'Recording…' : 'Hold to Talk'}
    </button>
  )
}

