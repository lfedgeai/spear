import { Button } from '../../../web-admin/src/shared/components/ui/button'

export function ChatHeader(props: {
  title: string
  targetText: string
  status: 'disconnected' | 'connecting' | 'connected'
  error: string
  hasActiveChat: boolean
  onOpenConnect: () => void
  onDisconnect: () => void
}) {
  return (
    <header className="cw-header">
      <div className="cw-headerLeft">
        <div className="cw-headerEyebrow">Web Console</div>
        <div className="cw-headerTitle">{props.title}</div>
        <div className="cw-headerMeta">{props.targetText}</div>
      </div>
      <div className="cw-headerRight">
        <div className="cw-headerStatusGroup">
          <div className="cw-connChip" title={props.targetText}>
            <span
              className={
                props.status === 'connected'
                  ? 'cw-dot cw-dotOk'
                  : props.status === 'connecting'
                    ? 'cw-dot cw-dotWarn'
                    : 'cw-dot'
              }
            />
            <span className="cw-connChipText">
              {props.status} • {props.targetText}
            </span>
          </div>
          {props.error ? <div className="cw-error cw-errorInline">{props.error}</div> : null}
        </div>
        {props.hasActiveChat ? (
          <div className="cw-headerActionGroup">
            <Button size="sm" onClick={props.onOpenConnect}>
              <span className="cw-btnGlyph">↗</span>
              <span className="cw-btnText">Connect…</span>
            </Button>
            <Button variant="destructive" size="sm" onClick={props.onDisconnect} disabled={props.status === 'disconnected'}>
              <span className="cw-btnGlyph">×</span>
              <span className="cw-btnText">Disconnect</span>
            </Button>
          </div>
        ) : null}
      </div>
    </header>
  )
}
