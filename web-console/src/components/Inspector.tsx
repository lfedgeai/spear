import { type Conversation } from '../models/conversation'
import { type MicMode } from '../voice'

export function Inspector(props: {
  mode: 'expanded' | 'collapsed'
  onToggle: () => void
  active: Conversation | null
  targetText: string
  status: 'disconnected' | 'connecting' | 'connected'
  error: string
  micMode: MicMode
  voiceRecording: boolean
  voiceError: string
}) {
  const collapsed = props.mode === 'collapsed'
  return (
    <aside className={collapsed ? 'cw-inspector cw-inspectorCollapsed' : 'cw-inspector'}>
      <div className="cw-inspectorHeader">
        <div className="cw-panelHeader">
          {!collapsed ? <div className="cw-inspectorTitle">Inspector</div> : <div className="cw-inspectorTitle">Info</div>}
          <button
            className="cw-btn cw-btnIconToggle"
            onClick={props.onToggle}
            title={collapsed ? 'Expand info' : 'Collapse info'}
            aria-label={collapsed ? 'Expand info' : 'Collapse info'}
          >
            {collapsed ? '«' : '»'}
          </button>
        </div>
      </div>
      {!collapsed ? <div className="cw-inspectorBody">
        <div className="cw-inspectorCard">
          <div className="cw-label">Connection</div>
          <div className="cw-readonly">{props.targetText}</div>
          <div className="cw-statusRow">
            <span
              className={
                props.status === 'connected'
                  ? 'cw-dot cw-dotOk'
                  : props.status === 'connecting'
                    ? 'cw-dot cw-dotWarn'
                    : 'cw-dot'
              }
            />
            <span>{props.status}</span>
          </div>
          {props.active?.connect_kind ? <div className="cw-readonly">kind: {props.active.connect_kind}</div> : null}
          {props.active?.taskId.trim() ? <div className="cw-readonly">task: {props.active.taskId}</div> : null}
          {props.active?.instanceId.trim() ? <div className="cw-readonly">instance: {props.active.instanceId}</div> : null}
          {props.active?.executionId.trim() ? <div className="cw-readonly">execution: {props.active.executionId}</div> : null}
        </div>

        <div className="cw-inspectorCard">
          <div className="cw-label">Streams</div>
          <div className="cw-readonly">1 text: {props.status === 'connected' ? 'open' : '—'}</div>
          <div className="cw-readonly">2 voice: {props.status === 'connected' ? 'open' : '—'}</div>
        </div>

        <div className="cw-inspectorCard">
          <div className="cw-label">Voice</div>
          <div className="cw-readonly">mode: {props.micMode}</div>
          <div className="cw-readonly">mic: {props.voiceRecording ? 'on' : 'off'}</div>
          {props.voiceError ? <div className="cw-error">{props.voiceError}</div> : null}
        </div>

        <div className="cw-inspectorCard">
          <div className="cw-label">Errors</div>
          {props.error ? <div className="cw-error">{props.error}</div> : <div className="cw-readonly">—</div>}
        </div>
      </div> : (
        <div className="cw-inspectorMini">
          <div className="cw-miniRow" title={props.status}>
            <span
              className={
                props.status === 'connected'
                  ? 'cw-dot cw-dotOk'
                  : props.status === 'connecting'
                    ? 'cw-dot cw-dotWarn'
                    : 'cw-dot'
              }
            />
          </div>
          <div className="cw-miniRow" title={`mic: ${props.voiceRecording ? 'on' : 'off'}`}>
            <span className={props.voiceRecording ? 'cw-miniIcon cw-miniIconOn' : 'cw-miniIcon'}>🎙</span>
          </div>
          <div className="cw-miniRow" title={props.error || 'ok'}>
            <span className={props.error ? 'cw-miniIcon cw-miniIconErr' : 'cw-miniIcon'}>!</span>
          </div>
        </div>
      )}
    </aside>
  )
}
