import { MicSplitControl, type MicMode } from '../voice'

export function Composer(props: {
  status: 'disconnected' | 'connecting' | 'connected'
  input: string
  onChangeInput: (v: string) => void
  onSend: () => void
  micMode: MicMode
  onChangeMicMode: (m: MicMode) => void
  voiceRecording: boolean
  onHoldStart: () => void
  onHoldEnd: () => void
  onToggleMic: () => void
  voiceError: string
  showCancel: boolean
  onCancel: () => void
}) {
  return (
    <div className="cw-composer">
      <textarea
        className="cw-textarea"
        value={props.input}
        onChange={(e) => props.onChangeInput(e.target.value)}
        placeholder={props.status === 'connected' ? 'Message…' : 'Connect to send messages…'}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault()
            props.onSend()
          }
        }}
      />
      <div className="cw-composerActions">
        <button className="cw-btn cw-btnPrimary" disabled={props.status !== 'connected' || !props.input.trim()} onClick={props.onSend}>
          <span className="cw-btnGlyph">→</span>
          <span className="cw-btnText">Send</span>
        </button>
        <MicSplitControl
          disabled={props.status !== 'connected'}
          recording={props.voiceRecording}
          mode={props.micMode}
          onModeChange={props.onChangeMicMode}
          onHoldStart={props.onHoldStart}
          onHoldEnd={props.onHoldEnd}
          onToggle={props.onToggleMic}
        />
        {props.showCancel ? (
          <button className="cw-btn cw-danger" disabled={props.status !== 'connected'} onClick={props.onCancel}>
            <span className="cw-btnGlyph">■</span>
            <span className="cw-btnText">Cancel</span>
          </button>
        ) : null}
      </div>
      {props.voiceError ? <div className="cw-error cw-errorInline">{props.voiceError}</div> : null}
    </div>
  )
}
