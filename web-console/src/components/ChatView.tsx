import { type ChatMessage } from '../models/conversation'
import { ChatBubble } from './ChatBubble'
import type { RefObject } from 'react'

export function ChatView(props: {
  hasActiveChat: boolean
  messages: ChatMessage[]
  endRef: RefObject<HTMLDivElement | null>
  onNewChat: () => void
  onNewChatAndConnect: () => void
}) {
  return (
    <div className="cw-chat">
      {props.messages.length ? (
        props.messages.map((m) => <ChatBubble key={m.id} message={m} />)
      ) : !props.hasActiveChat ? (
        <div className="cw-empty">
          <div className="cw-emptyTitle">No chat selected</div>
          <div className="cw-emptyText">
            Create a chat first, then connect it to an execution or endpoint when needed.
          </div>
          <div className="cw-emptyActions">
            <button className="cw-btn" onClick={props.onNewChat}>
              <span className="cw-btnGlyph">+</span>
              <span className="cw-btnText">New chat</span>
            </button>
            <button className="cw-btn cw-btnPrimary" onClick={props.onNewChatAndConnect}>
              <span className="cw-btnGlyph">↗</span>
              <span className="cw-btnText">New chat and connect</span>
            </button>
          </div>
        </div>
      ) : (
        <div className="cw-empty">
          <div className="cw-emptyTitle">Connect and start chatting</div>
          <div className="cw-emptyText">
            Connect by execution (stream session) or by endpoint gateway. The chat window stays the same.
          </div>
        </div>
      )}
      <div ref={props.endRef} />
    </div>
  )
}
