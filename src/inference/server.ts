import express from "express"
import * as http from "http"
import * as path from "path"
import { fileURLToPath } from "url"
import { InferenceBackend, type StreamCallbacks } from "./backend.ts"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const PROJECT_ROOT = path.resolve(__dirname, "..")

export class InferenceServer {
  private app: express.Express
  private server: http.Server
  private backend: InferenceBackend
  private slotsDir: string

  constructor(slotsDir: string, port = 3100) {
    this.slotsDir = slotsDir
    this.backend = new InferenceBackend(slotsDir)
    this.app = express()
    this.app.use(express.json({ limit: "100mb" }))
    this.server = http.createServer(this.app)
    this.routes()
  }

  private routes() {
    this.app.get("/health", (_req, res) => {
      res.json({ status: "ok" })
    })

    this.app.post("/v1/generate", async (req, res) => {
      try {
        const { modelPath, loraPaths, stateSlot, prompt, ...opts } = req.body
        if (!modelPath || !prompt) return res.status(400).json({ error: "modelPath and prompt required" })

        await this.backend.ensureModel(modelPath, loraPaths ?? [])

        if (stateSlot) {
          try {
            await this.backend.loadStateSlot(stateSlot, modelPath, loraPaths ?? [])
          } catch { /* slot doesn't exist yet, skip */ }
        }

        const text = await this.backend.generate(prompt, opts)
        res.json({ text })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/v1/evaluate", async (req, res) => {
      try {
        const { modelPath, loraPaths, text } = req.body
        if (!modelPath || !text) return res.status(400).json({ error: "modelPath and text required" })
        await this.backend.ensureModel(modelPath, loraPaths ?? [])
        const result = await this.backend.evaluate(text)
        res.json(result)
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/v1/state/save", async (req, res) => {
      try {
        const { modelPath, loraPaths, slotName } = req.body
        if (!modelPath || !slotName) return res.status(400).json({ error: "modelPath and slotName required" })
        const result = await this.backend.saveStateSlot(slotName, modelPath, loraPaths ?? [])
        res.json(result)
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/v1/state/load", async (req, res) => {
      try {
        const { modelPath, loraPaths, slotName } = req.body
        if (!modelPath || !slotName) return res.status(400).json({ error: "modelPath and slotName required" })
        await this.backend.loadStateSlot(slotName, modelPath, loraPaths ?? [])
        res.json({ loaded: slotName })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/v1/tokenize", (req, res) => {
      try {
        const { modelPath, text } = req.body
        if (!modelPath || !text) return res.status(400).json({ error: "modelPath and text required" })
        // Need model loaded first — load without state
        // Tokenize only needs model, not sequence, so we handle it differently
        res.status(501).json({ error: "tokenize requires model to be loaded first via generate/evaluate" })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.get("/v1/generate", (req, res) => {
      const prompt = req.query.prompt as string
      const modelPath = req.query.modelPath as string
      if (!prompt || !modelPath) { res.status(400).json({ error: "prompt and modelPath required" }); return }

      res.writeHead(200, {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      })

      const callbacks: StreamCallbacks = {
        onToken: (text: string) => {
          res.write(`data: ${JSON.stringify({ type: "token", text })}\n\n`)
        },
        onDone: (meta) => {
          res.write(`data: ${JSON.stringify({ type: "done", ...meta })}\n\n`)
          res.end()
        },
        onError: (err: string) => {
          res.write(`data: ${JSON.stringify({ type: "error", error: err })}\n\n`)
          res.end()
        },
      }

      this.backend.ensureModel(modelPath, [])
        .then(() => this.backend.stream(prompt, callbacks, {
          maxTokens: parseInt(req.query.maxTokens as string) || 1024,
          temperature: parseFloat(req.query.temperature as string) || 0.8,
          topP: parseFloat(req.query.topP as string) || 0.9,
        }))
        .catch((err) => callbacks.onError?.(err instanceof Error ? err.message : String(err)))
    })

    this.app.post("/v1/stream", async (req, res) => {
      try {
        const { modelPath, loraPaths, stateSlot, prompt, ...opts } = req.body
        if (!prompt) { res.status(400).json({ error: "prompt required" }); return }

        await this.backend.ensureModel(modelPath, loraPaths ?? [])
        if (stateSlot) {
          try { await this.backend.loadStateSlot(stateSlot, modelPath, loraPaths ?? []) } catch { /* */ }
        }

        res.writeHead(200, {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache",
          Connection: "keep-alive",
        })

        const callbacks: StreamCallbacks = {
          onToken: (text: string) => {
            res.write(`data: ${JSON.stringify({ type: "token", text })}\n\n`)
          },
          onDone: (meta) => {
            res.write(`data: ${JSON.stringify({ type: "done", ...meta })}\n\n`)
            res.end()
          },
          onError: (err: string) => {
            res.write(`data: ${JSON.stringify({ type: "error", error: err })}\n\n`)
            res.end()
          },
        }

        await this.backend.stream(prompt, callbacks, opts)
      } catch (err: any) {
        if (!res.writableEnded) {
          res.write(`data: ${JSON.stringify({ type: "error", error: err.message })}\n\n`)
          res.end()
        }
      }
    })
  }

  async start(port = 3100): Promise<void> {
    await new Promise<void>((resolve) => {
      this.server.listen(port, "0.0.0.0", () => resolve())
    })
    console.error(`Inference API listening on http://0.0.0.0:${port}`)
  }

  async stop(): Promise<void> {
    return new Promise((resolve) => {
      this.server.close(() => resolve())
    })
  }
}