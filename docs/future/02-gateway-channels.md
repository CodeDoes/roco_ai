# Gateway & Channels

## Gateway

Central routing layer. Single process, multiple entry points.

```
Gateway
├── CLI (stdin/stdout)        ← current
├── HTTP REST API             ← planned
├── WebSocket (monitor)      ← partial (dev/agent monitor/)
├── Telegram Bot              ← planned
├── Discord Bot               ← planned
├── Signal                     ← planned
└── iMessage                   ← stretch
```

Gateway handles:
- Authentication (DM pairing, allowlist)
- Session routing (resolve story slug from channel + user)
- Rate limiting
- Message logging

## Channel Interface

```typescript
interface Channel {
  name: string
  send(text: string): Promise<void>
  sendStream(stream: AsyncIterable<string>): AsyncIterable<void>
  onMessage(handler: (msg: ChannelMessage) => void): void
  capabilities: {
    streaming: boolean
    richText: boolean
    fileUpload: boolean
    interactive: boolean
  }
}
```

## Session Routing

```
Telegram user "alice" → session "alice_default"
Telegram user "alice" with --story=mybook → session "alice_mybook"
Discord user "alice" → same session "alice_default" (cross-channel)
```

Sessions are channel-agnostic. Alice can start a story on Telegram, continue on Discord, and monitor via Web.

## Message Flow

```
User [Telegram]
  → Telegram Bot WebHook
    → Gateway.parseMessage()
      → resolve session
        → ChannelHandler.generate()
          → RwkvEngine.generate()
            → stream tokens back through channel
```
