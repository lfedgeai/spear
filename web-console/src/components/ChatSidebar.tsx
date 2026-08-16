import { type Conversation } from '../models/conversation'
import { Button } from '../../../web-admin/src/shared/components/ui/button'

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
          <div className="cw-brandBlock">
            {!collapsed ? <div className="cw-brandEyebrow">Workspace</div> : null}
            {collapsed ? <div className="cw-brandTitle" title={props.appTitle}>SC</div> : <div className="cw-brandTitle">{props.appTitle}</div>}
            {!collapsed ? <div className="cw-brandMeta">Live console sessions and targets</div> : null}
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 rounded-md border border-[hsl(var(--border))] bg-[hsl(var(--secondary))]"
            onClick={props.onToggle}
            title={collapsed ? 'Expand chats' : 'Collapse chats'}
            aria-label={collapsed ? 'Expand chats' : 'Collapse chats'}
          >
            {collapsed ? '»' : '«'}
          </Button>
        </div>
        <Button
          size="sm"
          className="justify-center"
          onClick={props.onNewChat}
          title="New chat"
        >
          <span className="cw-btnGlyph">+</span>
          {!collapsed ? <span className="cw-btnText">New chat</span> : null}
        </Button>
      </div>

      {!collapsed ? <div className="cw-listSectionTitle">Recent chats</div> : null}
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
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2"
                    onClick={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      props.onRename(c.id, c.title)
                    }}
                  >
                    <span className="cw-btnGlyph">✎</span>
                    <span className="cw-btnText">Rename</span>
                  </Button>
                  <Button
                    variant="destructive"
                    size="sm"
                    className="h-7 px-2"
                    onClick={(e) => {
                      e.preventDefault()
                      e.stopPropagation()
                      props.onDelete(c.id)
                    }}
                  >
                    <span className="cw-btnGlyph">⌫</span>
                    <span className="cw-btnText">Delete</span>
                  </Button>
                </div>
              </>
            )}
          </button>
        ))}
      </div>
      <div className="cw-sidebarFooter">
        {!collapsed ? <div className="cw-sidebarFooterLabel">Preferences</div> : null}
        <Button
          variant="secondary"
          size="sm"
          className="cw-btnSidebarSettings justify-center"
          onClick={props.onOpenSettings}
          title="Settings"
        >
          <span className="cw-btnGlyph cw-btnGlyphLarge">⚙</span>
          {!collapsed ? <span className="cw-btnText">Settings</span> : null}
        </Button>
      </div>
    </aside>
  )
}
