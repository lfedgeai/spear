import { Button } from '../../../web-admin/src/shared/components/ui/button'
import { Input } from '../../../web-admin/src/shared/components/ui/input'

export function RenameDialog(props: {
  value: string
  onChange: (v: string) => void
  onClose: () => void
  onConfirm: () => void
}) {
  return (
    <div className="cw-modalBackdrop" role="dialog" aria-modal="true">
      <div className="cw-modal">
        <div className="cw-modalHeader">
          <div className="cw-modalTitle">Rename chat</div>
          <div className="cw-modalHeaderRight">
            <Button variant="secondary" size="sm" onClick={props.onClose}>
              <span className="cw-btnGlyph">×</span>
              <span className="cw-btnText">Close</span>
            </Button>
          </div>
        </div>
        <div className="cw-modalBody">
          <div className="cw-modalRow">
            <div className="cw-label">Title</div>
            <Input
              value={props.value}
              onChange={(e) => props.onChange(e.target.value)}
              autoFocus
              placeholder="Chat title"
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault()
                  props.onConfirm()
                }
              }}
            />
          </div>
        </div>
        <div className="cw-modalFooter">
          <Button variant="secondary" size="sm" onClick={props.onClose}>
            <span className="cw-btnGlyph">↩</span>
            <span className="cw-btnText">Cancel</span>
          </Button>
          <Button size="sm" onClick={props.onConfirm} disabled={!props.value.trim()}>
            <span className="cw-btnGlyph">✓</span>
            <span className="cw-btnText">Save</span>
          </Button>
        </div>
      </div>
    </div>
  )
}
