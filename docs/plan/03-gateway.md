# Phase 3: Gateway

## Goal
Move from single CLI process to a gateway daemon that supports multiple channels.

## Architecture

```
Gateway Daemon (background process)
├── CLI channel (stdin/stdout)
├── HTTP API (REST + WebSocket)
├── File watcher (session changes → broadcast)
└── Channel router (incoming → session → outgoing)

Sessions stored in s/<story>/
Monitor dashboard on port 3030 (from dev/agent)
```

## HTTP API

```
GET  /api/sessions              → list active sessions
GET  /api/session/:story        → get session state
POST /api/session/:story/chat   → send message, stream response
POST /api/session/:story/steer  → inject instruction
POST /api/session/:story/checkpoint → save checkpoint
```

## Components

### Server
Existing `monitor/server.mjs` from dev/agent can be adapted. Uses Express + WebSocket. Serves React dashboard.

### Session Manager
Current `session.ts` is already channel-agnostic. Just needs HTTP wrapper.

### Channel Adapters
- `channels/terminal.ts` — process.stdin/stdout (current CLI)
- `channels/http.ts` — REST API (planned)
- `channels/telegram.ts` — Telegram bot (future)
- `channels/discord.ts` — Discord bot (future)

### Status
[ ] Split CLI from agent logic (separate parse from execute)
[ ] Implement HTTP gateway
[ ] Add WebSocket for streaming responses
[ ] Adapt monitor dashboard for RWKV backend
