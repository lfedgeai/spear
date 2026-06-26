import { type Conversation } from '../models/conversation'

export function ChatSidebar(props: {
  mode: 'expanded' | 'collapsed'
  appTitle: string
  conversations: Conversation[]
  activeId: string
  onToggle: () => void
  onNewChat: () => void
  onOpenSettings: () => void
  onSwitch: (id: string) => void
  onRename: (id: string, title: string) => void
  onDelete: (id: string) => void
}) {
  const collapsed = props.mode === 'collapsed'
  return (
    <aside className={collapsed ? 'cw-sidebar cw-sidebarCollapsed' : 'cw-sidebar'}>
      <div className="cw-brand">
        <div className="cw-panelHeader">
          {collapsed ? <div className="cw-brandTitle" title={props.appTitle}>SC</div> : <div className="cw-brandTitle">{props.appTitle}</div>}
          <button
            className="cw-btn cw-btnIconToggle"
            onClick={props.onToggle}
            title={collapsed ? 'Expand chats' : 'Collapse chats'}
            aria-label={collapsed ? 'Expand chats' : 'Collapse chats'}
          >
            {collapsed ? '»' : '«'}
          </button>
        </div>
        <button className="cw-btn cw-btnPrimary" onClick={props.onNewChat} title="New chat">
          <span className="cw-btnGlyph">+</span>
          {!collapsed ? <span className="cw-btnText">New chat</span> : null}
        </button>
      </div>

      <div className="cw-list">
        {props.conversations.map((c) => (
          <button
            key={c.id}
            className={c.id === props.activeId ? 'cw-conv cw-convActive' : 'cw-conv'}
            onClick={() => props.onSwitch(c.id)}
            title={c.title.trim() ? c.title : '(untitled)'}
          >
            {collapsed ? (
              <div className="cw-convMini">
                {(c.title.trim() ? c.title : 'C').slice(0, 1).toUpperCase()}
              </div>
            ) : (
              <>
                <div className="cw-convTitle">{c.title.trim() ? c.title : '(untitled)'}</div>
                <div className="cw-convMeta">
                  {c.connect_kind === 'endpoint' && c.gatewayEndpoint
                    ? `endpoint: ${c.gatewayEndpoint}`
                    : c.executionId
                      ? `execution: ${c.executionId}`
                      : 'no target'}
                </div>
                <div className="cw-convActions">
                  <button
                    className="cw-iconBtn"
                    onClick={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      props.onRename(c.id, c.title)
                    }}
                  >
                    <span className="cw-btnGlyph">✎</span>
                    <span className="cw-btnText">Rename</span>
                  </button>
                  <button
                    className="cw-iconBtn cw-danger"
                    onClick={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      props.onDelete(c.id)
                    }}
                  >
                    <span className="cw-btnGlyph">⌫</span>
                    <span className="cw-btnText">Delete</span>
                  </button>
                </div>
              </>
            )}
          </button>
        ))}
      </div>
      <div className="cw-sidebarFooter">
        <button className="cw-btn cw-btnSidebarSettings" onClick={props.onOpenSettings} title="Settings">
          <span className="cw-btnGlyph cw-btnGlyphLarge">⚙</span>
          {!collapsed ? <span className="cw-btnText">Settings</span> : null}
        </button>
      </div>
    </aside>
  )
}
