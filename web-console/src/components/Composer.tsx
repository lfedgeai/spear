import { MicSplitControl, type MicMode } from '../voice'
import { Button } from '../../../web-admin/src/shared/components/ui/button'
import { Textarea } from '../../../web-admin/src/shared/components/ui/textarea'

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
      <Textarea
        className="cw-textarea min-h-[52px] resize-none"
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
        <Button size="sm" disabled={props.status !== 'connected' || !props.input.trim()} onClick={props.onSend}>
          <span className="cw-btnGlyph">→</span>
          <span className="cw-btnText">Send</span>
        </Button>
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
          <Button variant="destructive" size="sm" disabled={props.status !== 'connected'} onClick={props.onCancel}>
            <span className="cw-btnGlyph">■</span>
            <span className="cw-btnText">Cancel</span>
          </Button>
        ) : null}
      </div>
      {props.voiceError ? <div className="cw-error cw-errorInline">{props.voiceError}</div> : null}
    </div>
  )
}
