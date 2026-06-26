import { type Theme } from '../hooks/useTheme'

export function SettingsDialog(props: {
  theme: Theme
  onChangeTheme: (theme: Theme) => void
  onClose: () => void
}) {
  return (
    <div className="cw-modalBackdrop" role="dialog" aria-modal="true">
      <div className="cw-modal cw-settingsModal">
        <div className="cw-modalHeader">
          <div className="cw-modalTitle">Settings</div>
          <div className="cw-modalHeaderRight">
            <button className="cw-iconBtn" onClick={props.onClose}>
              <span className="cw-btnGlyph">×</span>
              <span className="cw-btnText">Close</span>
            </button>
          </div>
        </div>
        <div className="cw-modalBody">
          <div className="cw-modalRow">
            <div className="cw-label">Appearance</div>
            <div className="cw-settingsGroup">
              <button
                className={props.theme === 'dark' ? 'cw-btn cw-btnPrimary' : 'cw-btn'}
                onClick={() => props.onChangeTheme('dark')}
              >
                <span className="cw-btnGlyph">◐</span>
                <span className="cw-btnText">Dark</span>
              </button>
              <button
                className={props.theme === 'light' ? 'cw-btn cw-btnPrimary' : 'cw-btn'}
                onClick={() => props.onChangeTheme('light')}
              >
                <span className="cw-btnGlyph">☼</span>
                <span className="cw-btnText">Light</span>
              </button>
            </div>
          </div>
        </div>
        <div className="cw-modalFooter">
          <button className="cw-btn cw-btnPrimary" onClick={props.onClose}>
            <span className="cw-btnGlyph">✓</span>
            <span className="cw-btnText">Done</span>
          </button>
        </div>
      </div>
    </div>
  )
}
