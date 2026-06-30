import express from "express"
import * as http from "http"
import * as path from "path"
import { WebSocketServer, WebSocket } from "ws"
import { fileURLToPath } from "url"
import { RwkvEngine } from "../rwkv-engine.ts"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const PROJECT_ROOT = path.resolve(__dirname, "../..")

export class GatewayServer {
  private engine: RwkvEngine
  private app: express.Express
  private server: http.Server
  private wss: WebSocketServer

  constructor(engine: RwkvEngine) {
    this.engine = engine

    this.app = express()
    this.app.use(express.json())

    this.server = http.createServer(this.app)
    this.wss = new WebSocketServer({ server: this.server })

    this.setupRoutes()
    this.setupWebSocket()
  }

  private setupRoutes() {
    this.app.post("/generate", async (req, res) => {
      try {
        const { prompt, maxTokens, temperature, grammar } = req.body
        if (!prompt) { res.status(400).json({ error: "prompt required" }); return }

        const result = await this.engine.generate(prompt, { maxTokens, temperature, grammar } as any)
        res.json({ result })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/tokenize", (req, res) => {
      try {
        const { text } = req.body
        if (!text) { res.status(400).json({ error: "text required" }); return }
        const tokens = this.engine.tokenize(text)
        res.json({ tokens, count: tokens.length })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.get("/state/size", (_req, res) => {
      try {
        const size = this.engine.getStateSize()
        res.json({ size })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.get("/state", (_req, res) => {
      res.json({
        modelPath: (this.engine as any).modelPath,
        stateDir: (this.engine as any).stateDir,
      })
    })

    this.app.get("/health", (_req, res) => {
      res.json({ status: "ok" })
    })
  }

  private setupWebSocket() {
    this.wss.on("connection", (ws: WebSocket) => {
      ws.on("message", async (raw) => {
        try {
          const msg = JSON.parse(raw.toString())
          if (msg.type !== "generate") return

          const { prompt, maxTokens, temperature, grammar } = msg
          if (!prompt) {
            ws.send(JSON.stringify({ type: "error", message: "prompt required" }))
            return
          }

          await this.engine.generateStream(prompt, {
            onText: (t) => {
              if (ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: "token", text: t }))
              }
            },
          }, { maxTokens, temperature, grammar } as any)

          if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: "done" }))
          }
        } catch (err: any) {
          if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: "error", message: err.message }))
          }
        }
      })

      ws.send(JSON.stringify({ type: "connected" }))
    })
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
    return new Promise((resolve) => this.server.close(() => resolve()))
  }
}
