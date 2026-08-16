import { type Conversation } from '../models/conversation'
import { type MicMode } from '../voice'
import { Button } from '../../../web-admin/src/shared/components/ui/button'

function DetailRow(props: {
  label: string
  value: string
  mono?: boolean
  tone?: 'default' | 'status'
  status?: 'disconnected' | 'connecting' | 'connected'
}) {
  return (
    <div className="cw-detailRow">
      <div className="cw-detailLabel">{props.label}</div>
      <div className={props.mono ? 'cw-detailValue cw-detailValueMono' : 'cw-detailValue'}>
        {props.tone === 'status' ? (
          <span className="cw-statusRow">
            <span
              className={
                props.status === 'connected'
                  ? 'cw-dot cw-dotOk'
                  : props.status === 'connecting'
                    ? 'cw-dot cw-dotWarn'
                    : 'cw-dot'
              }
            />
            <span>{props.value}</span>
          </span>
        ) : (
          props.value
        )}
      </div>
    </div>
  )
}

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
          <div className="cw-inspectorHeading">
            {!collapsed ? <div className="cw-inspectorEyebrow">Session Details</div> : null}
            {!collapsed ? <div className="cw-inspectorTitle">Inspector</div> : <div className="cw-inspectorTitle">Info</div>}
            {!collapsed ? (
              <div className="cw-inspectorMeta">Connection, streams, voice and runtime health</div>
            ) : null}
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="cw-btnIconToggle rounded-md border border-[hsl(var(--border))] bg-[hsl(var(--secondary))]"
            onClick={props.onToggle}
            title={collapsed ? 'Expand info' : 'Collapse info'}
            aria-label={collapsed ? 'Expand info' : 'Collapse info'}
          >
            {collapsed ? '«' : '»'}
          </Button>
        </div>
      </div>
      {!collapsed ? <div className="cw-inspectorBody">
        <div className="cw-inspectorCard">
          <div className="cw-sectionTitle">Connection</div>
          <div className="cw-detailList">
            <DetailRow label="Target" value={props.targetText || '—'} mono />
            <DetailRow
              label="Status"
              value={props.status}
              tone="status"
              status={props.status}
            />
            {props.active?.connect_kind ? (
              <DetailRow label="Kind" value={props.active.connect_kind} />
            ) : null}
            {props.active?.taskId.trim() ? (
              <DetailRow label="Task" value={props.active.taskId} mono />
            ) : null}
            {props.active?.instanceId.trim() ? (
              <DetailRow label="Instance" value={props.active.instanceId} mono />
            ) : null}
            {props.active?.executionId.trim() ? (
              <DetailRow label="Execution" value={props.active.executionId} mono />
            ) : null}
          </div>
        </div>

        <div className="cw-inspectorCard">
          <div className="cw-sectionTitle">Streams</div>
          <div className="cw-detailList">
            <DetailRow label="Text" value={props.status === 'connected' ? 'open' : '—'} />
            <DetailRow label="Voice" value={props.status === 'connected' ? 'open' : '—'} />
          </div>
        </div>

        <div className="cw-inspectorCard">
          <div className="cw-sectionTitle">Voice</div>
          <div className="cw-detailList">
            <DetailRow label="Mode" value={props.micMode} />
            <DetailRow label="Mic" value={props.voiceRecording ? 'on' : 'off'} />
          </div>
          {props.voiceError ? <div className="cw-error">{props.voiceError}</div> : null}
        </div>

        <div className="cw-inspectorCard">
          <div className="cw-sectionTitle">Errors</div>
          {props.error ? <div className="cw-error">{props.error}</div> : <div className="cw-detailEmpty">No active errors</div>}
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
