import { type Theme } from '../hooks/useTheme'
import { Button } from '../../../web-admin/src/shared/components/ui/button'

export function SettingsDialog(props: {
  theme: Theme
  onChangeTheme: (theme: Theme) => void
  onClose: () => void
}) {
  return (
    <div className="cw-modalBackdrop" role="dialog" aria-modal="true">
      <div className="cw-modal cw-settingsModal">
        <div className="cw-modalHeader">
          <div className="cw-modalHeading">
            <div className="cw-modalEyebrow">Preferences</div>
            <div className="cw-modalTitle">Settings</div>
            <div className="cw-modalDescription">
              Adjust the console appearance to match the environment you prefer.
            </div>
          </div>
          <div className="cw-modalHeaderRight">
            <Button variant="secondary" size="sm" onClick={props.onClose}>
              <span className="cw-btnGlyph">×</span>
              <span className="cw-btnText">Close</span>
            </Button>
          </div>
        </div>
        <div className="cw-modalBody">
          <div className="cw-modalRow">
            <div className="cw-label">Appearance</div>
            <div className="cw-settingsGroup">
              <Button
                variant={props.theme === 'dark' ? 'default' : 'secondary'}
                size="sm"
                onClick={() => props.onChangeTheme('dark')}
              >
                <span className="cw-btnGlyph">◐</span>
                <span className="cw-btnText">Dark</span>
              </Button>
              <Button
                variant={props.theme === 'light' ? 'default' : 'secondary'}
                size="sm"
                onClick={() => props.onChangeTheme('light')}
              >
                <span className="cw-btnGlyph">☼</span>
                <span className="cw-btnText">Light</span>
              </Button>
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
