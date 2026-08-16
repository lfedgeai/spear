import { useEffect, useMemo, useRef, useState } from 'react'
import { Button } from '../../../web-admin/src/shared/components/ui/button'

export type MicMode = 'hold' | 'open'

export type MicSplitControlProps = {
  disabled: boolean
  recording: boolean
  mode: MicMode
  onModeChange: (mode: MicMode) => void
  onHoldStart: () => void
  onHoldEnd: () => void
  onToggle: () => void
}

export function MicSplitControl(props: MicSplitControlProps) {
  const [menuOpen, setMenuOpen] = useState(false)
  const pressedRef = useRef(false)
  const wrapRef = useRef<HTMLDivElement | null>(null)

  const label = useMemo(() => {
    if (props.mode === 'open') return props.recording ? 'Mic On' : 'Open Mic'
    return props.recording ? 'Recording…' : 'Hold to Talk'
  }, [props.mode, props.recording])

  useEffect(() => {
    if (!menuOpen) return
    const onDown = (e: MouseEvent) => {
      const el = wrapRef.current
      if (!el) return
      if (e.target instanceof Node && el.contains(e.target)) return
      setMenuOpen(false)
    }
    document.addEventListener('pointerdown', onDown)
    return () => document.removeEventListener('pointerdown', onDown)
  }, [menuOpen])

  const end = () => {
    if (!pressedRef.current) return
    pressedRef.current = false
    props.onHoldEnd()
  }

  return (
    <div className="cw-micSplit" ref={wrapRef}>
      <Button
        variant={props.recording ? 'default' : 'secondary'}
        size="sm"
        className="cw-micSplitMain"
        disabled={props.disabled}
        onClick={() => {
          if (props.disabled) return
          if (props.mode === 'open') props.onToggle()
        }}
        onPointerDown={(e) => {
          if (props.disabled) return
          if (props.mode !== 'hold') return
          e.preventDefault()
          pressedRef.current = true
          props.onHoldStart()
        }}
        onPointerUp={(e) => {
          if (props.mode !== 'hold') return
          e.preventDefault()
          end()
        }}
        onPointerCancel={(e) => {
          if (props.mode !== 'hold') return
          e.preventDefault()
          end()
        }}
        onPointerLeave={(e) => {
          if (props.mode !== 'hold') return
          e.preventDefault()
          end()
        }}
      >
        {label}
      </Button>
      <Button
        variant="secondary"
        size="sm"
        className="cw-micSplitArrow"
        disabled={props.disabled}
        onClick={() => {
          if (props.disabled) return
          setMenuOpen((v) => !v)
        }}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        aria-label="Select microphone mode"
      >
        ▾
      </Button>
      {menuOpen ? (
        <div className="cw-menu" role="menu">
          <Button
            variant={props.mode === 'hold' ? 'default' : 'secondary'}
            size="sm"
            className="cw-menuItem"
            onClick={() => {
              setMenuOpen(false)
              if (props.recording && props.mode === 'open') props.onToggle()
              props.onModeChange('hold')
            }}
            role="menuitem"
          >
            Hold to Talk
          </Button>
          <Button
            variant={props.mode === 'open' ? 'default' : 'secondary'}
            size="sm"
            className="cw-menuItem"
            onClick={() => {
              setMenuOpen(false)
              if (props.recording && props.mode === 'hold') props.onHoldEnd()
              props.onModeChange('open')
            }}
            role="menuitem"
          >
            Open Mic
          </Button>
        </div>
      ) : null}
    </div>
  )
}
