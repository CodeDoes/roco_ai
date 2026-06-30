import express from "express"
import * as http from "http"
import * as path from "path"
import { fileURLToPath } from "url"
import { WebSocketServer, WebSocket } from "ws"
import { AgentEngine } from "../core/agent-engine.ts"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const PROJECT_ROOT = path.resolve(__dirname, "../..")

let channelIdCounter = 0

export class GatewayServer {
  private agent: AgentEngine
  private app: express.Express
  private server: http.Server
  private wss: WebSocketServer
  private channels: Map<number, WebSocket> = new Map()

  constructor(agent: AgentEngine, webappDir?: string) {
    this.agent = agent

    this.app = express()
    this.app.use(express.json())

    const staticDir = webappDir || path.join(PROJECT_ROOT, "webapp")
    this.app.use(express.static(staticDir))

    this.server = http.createServer(this.app)
    this.wss = new WebSocketServer({ server: this.server })

    this.setupRoutes()
    this.setupWebSocket()
  }

  private setupRoutes() {
    this.app.get("/health", (_req, res) => {
      res.json({ status: "ok", channels: this.channels.size })
    })

    this.app.get("/sessions", async (_req, res) => {
      try {
        const sessions = await this.agent.listSessions()
        const current = this.agent.getCurrentSession()
        res.json({ sessions, current: current.label })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/sessions", async (req, res) => {
      try {
        const { label } = req.body
        if (!label) { res.status(400).json({ error: "label required" }); return }
        const session = await this.agent.createSession(label)
        const messages = this.agent.getMessages()
        this.broadcast({ type: "session_created", session, messages })
        res.json({ session, messages })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/sessions/:label/switch", async (req, res) => {
      try {
        const session = await this.agent.switchSession(req.params.label)
        const messages = this.agent.getMessages()
        this.broadcast({ type: "session_switched", session, messages })
        res.json({ session, messages })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.delete("/sessions/:label", async (req, res) => {
      try {
        await this.agent.deleteSession(req.params.label)
        const current = this.agent.getCurrentSession()
        const messages = this.agent.getMessages()
        this.broadcast({ type: "session_deleted", label: req.params.label, current, messages })
        res.json({ deleted: req.params.label, current, messages })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.get("/sessions/:label/messages", (req, res) => {
      const messages = this.agent.getMessages(req.params.label)
      res.json({ messages })
    })

    this.app.post("/chat", async (req, res) => {
      try {
        const { prompt } = req.body
        if (!prompt) { res.status(400).json({ error: "prompt required" }); return }

        let fullResult = ""
        const result = await this.agent.chat(prompt, {
          onToken: (t) => { fullResult += t },
        })
        res.json({ result })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })
  }

  private setupWebSocket() {
    this.wss.on("connection", (ws: WebSocket) => {
      const channelId = ++channelIdCounter
      this.channels.set(channelId, ws)
      console.error(`[gateway] channel ${channelId} connected (${this.channels.size} total)`)

      ws.send(JSON.stringify({
        type: "connected",
        channelId,
        session: this.agent.getCurrentSession(),
        messages: this.agent.getMessages(),
        sessions: Array.from((this.agent as any).sessions.keys()),
      }))

      ws.on("message", async (raw) => {
        try {
          const msg = JSON.parse(raw.toString())

          switch (msg.type) {
            case "chat": {
              const { prompt } = msg
              if (!prompt) {
                ws.send(JSON.stringify({ type: "error", message: "prompt required" }))
                return
              }
              this.broadcast({ type: "user_message", content: prompt, channelId })

              await this.agent.chat(prompt, {
                onToken: (t) => {
                  this.broadcast({ type: "token", text: t, channelId })
                },
              })

              this.broadcast({
                type: "done",
                session: this.agent.getCurrentSession(),
                messages: this.agent.getMessages(),
              })
              break
            }

            case "create_session": {
              const session = await this.agent.createSession(msg.label)
              const messages = this.agent.getMessages()
              this.broadcast({ type: "session_created", session, messages })
              break
            }

            case "switch_session": {
              const session = await this.agent.switchSession(msg.label)
              const messages = this.agent.getMessages()
              this.broadcast({ type: "session_switched", session, messages })
              break
            }

            case "delete_session": {
              await this.agent.deleteSession(msg.label)
              const current = this.agent.getCurrentSession()
              const messages = this.agent.getMessages()
              this.broadcast({ type: "session_deleted", label: msg.label, current, messages })
              break
            }

            default:
              ws.send(JSON.stringify({ type: "error", message: `Unknown message type: ${msg.type}` }))
          }
        } catch (err: any) {
          ws.send(JSON.stringify({ type: "error", message: err.message }))
        }
      })

      ws.on("close", () => {
        this.channels.delete(channelId)
        console.error(`[gateway] channel ${channelId} disconnected (${this.channels.size} total)`)
      })

      ws.on("error", () => {
        this.channels.delete(channelId)
      })
    })
  }

  private broadcast(data: object) {
    const payload = JSON.stringify(data)
    for (const [id, ws] of this.channels) {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(payload)
      } else {
        this.channels.delete(id)
      }
    }
  }

  async start(port = 3030, host = "0.0.0.0"): Promise<void> {
    return new Promise((resolve) => {
      this.server.listen(port, host, () => {
        resolve()
      })
    })
  }

  getHttpServer(): http.Server {
    return this.server
  }

  async stop(): Promise<void> {
    this.wss.close()
    for (const [_, ws] of this.channels) {
      ws.close()
    }
    this.channels.clear()
    await this.agent.dispose()
    return new Promise((resolve) => this.server.close(() => resolve()))
  }
}
