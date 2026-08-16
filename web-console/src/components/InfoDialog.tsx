import { useMemo } from 'react'
import { Button } from '../../../web-admin/src/shared/components/ui/button'
import { jsonPretty } from '../utils/text'

export function InfoDialog(props: {
  ids: { taskId: string; instanceId: string; executionId: string } | null
  loading: boolean
  error: string
  task: unknown
  execution: unknown
  onClose: () => void
}) {
  const ids = props.ids ?? { taskId: '', instanceId: '', executionId: '' }
  const taskText = useMemo(() => jsonPretty(props.task), [props.task])
  const executionText = useMemo(() => jsonPretty(props.execution), [props.execution])

  return (
    <div className="cw-modalBackdrop" role="dialog" aria-modal="true">
      <div className="cw-modal">
        <div className="cw-modalHeader">
          <div className="cw-modalTitle">Execution info</div>
          <div className="cw-modalHeaderRight">
            <Button variant="secondary" size="sm" onClick={props.onClose}>
              <span className="cw-btnGlyph">×</span>
              <span className="cw-btnText">Close</span>
            </Button>
          </div>
        </div>
        <div className="cw-modalBody">
          <div className="cw-modalRow">
            <div className="cw-label">Task</div>
            <div className="cw-readonly">{ids.taskId || 'n/a'}</div>
          </div>
          <div className="cw-modalRow">
            <div className="cw-label">Instance</div>
            <div className="cw-readonly">{ids.instanceId || 'n/a'}</div>
          </div>
          <div className="cw-modalRow">
            <div className="cw-label">Execution</div>
            <div className="cw-readonly">{ids.executionId || 'n/a'}</div>
          </div>
          {props.loading ? <div className="cw-modalHint">Loading…</div> : null}
          {props.error ? <div className="cw-error">{props.error}</div> : null}
          <div className="cw-modalRow">
            <div className="cw-label">Task detail</div>
            <div className="cw-codeWrap">
              <div className="cw-codeHeader">
                <div className="cw-codeLang">json</div>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2"
                  onClick={() => navigator.clipboard?.writeText(taskText).catch(() => {})}
                >
                  Copy
                </Button>
              </div>
              <pre className="cw-code">
                <code>{taskText}</code>
              </pre>
            </div>
          </div>
          <div className="cw-modalRow">
            <div className="cw-label">Execution detail</div>
            <div className="cw-codeWrap">
              <div className="cw-codeHeader">
                <div className="cw-codeLang">json</div>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2"
                  onClick={() => navigator.clipboard?.writeText(executionText).catch(() => {})}
                >
                  Copy
                </Button>
              </div>
              <pre className="cw-code">
                <code>{executionText}</code>
              </pre>
            </div>
          </div>
        </div>
        <div className="cw-modalFooter">
          <Button size="sm" onClick={props.onClose}>
            <span className="cw-btnGlyph">✓</span>
            <span className="cw-btnText">Done</span>
          </Button>
        </div>
      </div>
    </div>
  )
}
