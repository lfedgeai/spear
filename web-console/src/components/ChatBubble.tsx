import { useMemo } from 'react'
import { type ChatMessage } from '../models/conversation'
import { splitCodeBlocks } from '../utils/text'

export function ChatBubble(props: { message: ChatMessage }) {
  const { message } = props
  const isUser = message.role === 'user'
  const parts = useMemo(() => splitCodeBlocks(message.text), [message.text])
  return (
    <div className={isUser ? 'cw-row cw-rowUser' : 'cw-row'}>
      <div className={isUser ? 'cw-bubble cw-bubbleUser' : 'cw-bubble'}>
        {parts.map((p, idx) => {
          if (p.type === 'code') {
            return (
              <div key={idx} className="cw-codeWrap">
                <div className="cw-codeHeader">
                  <div className="cw-codeLang">{p.lang ?? ''}</div>
                  <button
                    className="cw-codeBtn"
                    onClick={() => navigator.clipboard?.writeText(p.code).catch(() => {})}
                  >
                    Copy
                  </button>
                </div>
                <pre className="cw-code">
                  <code>{p.code}</code>
                </pre>
              </div>
            )
          }
          return (
            <div key={idx} className="cw-text">
              {p.text}
            </div>
          )
        })}
      </div>
    </div>
  )
}

