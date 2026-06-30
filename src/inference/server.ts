import express from "express"
import * as http from "http"
import * as path from "path"
import { fileURLToPath } from "url"
import { ContextManager, type ContextHandle, type StreamCallbacks } from "./context-manager.ts"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const PROJECT_ROOT = path.resolve(__dirname, "..")

// ─── helpers ────────────────────────────────────────────────────────────────

function ctxOrFail(c: ContextHandle | undefined): ContextHandle {
  if (!c) throw new Error("Context not found")
  return c
}

function modelRequired(c: ContextHandle): asserts c is ContextHandle & { sequence: NonNullable<ContextHandle["sequence"]>; model: NonNullable<ContextHandle["model"]> } {
  if (!c.model || !c.sequence) throw new Error("Model not loaded — POST /context/:id/model first")
}

// ─── server ─────────────────────────────────────────────────────────────────

export class InferenceServer {
  private app: express.Express
  private server: http.Server
  private mgr: ContextManager
  private slotsDir: string

  constructor(slotsDir: string, port = 3100) {
    this.slotsDir = slotsDir
    this.mgr = new ContextManager(slotsDir)
    this.app = express()
    this.app.use(express.json({ limit: "100mb" }))
    this.server = http.createServer(this.app)
    this.routes()
  }

  private routes() {
    // health
    this.app.get("/health", (_req, res) => {
      const ctxs = this.mgr.list()
      res.json({ status: "ok", contexts: ctxs.length })
    })

    // ── context lifecycle ──────────────────────────────────────────────────

    this.app.post("/v1/context", async (_req, res) => {
      try {
        const ctx = await this.mgr.createContext()
        res.json({ id: ctx.id, createdAt: ctx.createdAt })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.get("/v1/contexts", (_req, res) => {
      res.json({ contexts: this.mgr.list() })
    })

    this.app.get("/v1/context/:id", (req, res) => {
      const c = this.mgr.get(req.params.id)
      if (!c) return res.status(404).json({ error: "Context not found" })
      res.json({
        id: c.id,
        modelPath: c.modelPath,
        loras: [...c.loras],
        slots: Array.from(c.stateSlots.keys()),
        lastUsed: c.lastUsed,
      })
    })

    this.app.delete("/v1/context/:id", async (req, res) => {
      try {
        await this.mgr.destroy(req.params.id)
        res.json({ deleted: req.params.id })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── model ops ──────────────────────────────────────────────────────────

    this.app.post("/v1/context/:id/model", async (req, res) => {
      try {
        const { modelPath, loraPaths } = req.body
        if (!modelPath) return res.status(400).json({ error: "modelPath required" })
        await this.mgr.loadModel(req.params.id, modelPath, loraPaths)
        res.json({ loaded: modelPath, loras: loraPaths ?? [] })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.delete("/v1/context/:id/model", async (req, res) => {
      try {
        await this.mgr.unloadModel(req.params.id)
        res.json({ unloaded: true })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── LoRA ───────────────────────────────────────────────────────────────

    this.app.post("/v1/context/:id/lora", async (req, res) => {
      try {
        const { path: loraPath } = req.body
        if (!loraPath) return res.status(400).json({ error: "path required" })
        await this.mgr.loadLora(req.params.id, loraPath)
        res.json({ loaded: loraPath })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.delete("/v1/context/:id/lora", async (req, res) => {
      try {
        const { path: loraPath } = req.body
        await this.mgr.unloadLora(req.params.id, loraPath)
        res.json({ removed: loraPath ?? "all" })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── state ──────────────────────────────────────────────────────────────

    this.app.post("/v1/context/:id/state", async (req, res) => {
      try {
        const { state } = req.body
        if (!state) return res.status(400).json({ error: "state required (base64)" })
        await this.mgr.loadState(req.params.id, state)
        res.json({ loaded: true })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── state slots ────────────────────────────────────────────────────────

    this.app.post("/v1/context/:id/state/slot", async (req, res) => {
      try {
        const { slotId } = req.body
        if (!slotId) return res.status(400).json({ error: "slotId required" })
        const result = await this.mgr.saveStateSlot(req.params.id, slotId)
        res.json({ slotId, path: result.path, size: result.size })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/v1/context/:id/state/slot/load", async (req, res) => {
      try {
        const { slotId } = req.body
        if (!slotId) return res.status(400).json({ error: "slotId required" })
        await this.mgr.loadStateSlot(req.params.id, slotId)
        res.json({ loaded: slotId })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.get("/v1/context/:id/state/slot/:slotId", async (req, res) => {
      try {
        const { state, size } = await this.mgr.downloadStateSlot(req.params.id, req.params.slotId)
        res.json({ state, size })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.put("/v1/context/:id/state/slot/:slotId", async (req, res) => {
      try {
        const { state } = req.body
        if (!state) return res.status(400).json({ error: "state required (base64)" })
        const result = await this.mgr.uploadStateSlot(req.params.id, req.params.slotId, state)
        res.json({ uploaded: true, size: result.size })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── MoSE ───────────────────────────────────────────────────────────────

    this.app.post("/v1/context/:id/mose", async (req, res) => {
      try {
        const { weights } = req.body
        if (!weights || typeof weights !== "object") return res.status(400).json({ error: "weights required: { slotId: weight }" })
        await this.mgr.useMoSE(req.params.id, weights as Record<string, number>)
        res.json({ blended: weights })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    this.app.post("/v1/context/:id/lose", async (req, res) => {
      try {
        const { weights } = req.body
        await this.mgr.useLose(req.params.id, weights as Record<string, number>)
        res.json({ applied: weights })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── evaluate (processContext) ──────────────────────────────────────────

    this.app.post("/v1/context/:id/evaluate", async (req, res) => {
      try {
        const { text } = req.body
        if (!text) return res.status(400).json({ error: "text required" })
        modelRequired(ctxOrFail(this.mgr.get(req.params.id)))
        const result = await this.mgr.processContext(req.params.id, text)
        res.json(result)
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── generate ──────────────────────────────────────────────────────────

    this.app.post("/v1/context/:id/generate", async (req, res) => {
      try {
        const { prompt, ...opts } = req.body
        if (!prompt) return res.status(400).json({ error: "prompt required" })
        const result = await this.mgr.generate(req.params.id, prompt, opts)
        res.json({ text: result })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })

    // ── stream (SSE) ───────────────────────────────────────────────────────

    this.app.get("/v1/context/:id/stream", (req, res) => {
      const prompt = req.query.prompt as string
      if (!prompt) {
        res.status(400).json({ error: "prompt required" })
        return
      }
      const opts: Record<string, unknown> = {}
      if (req.query.maxTokens) opts.maxTokens = parseInt(req.query.maxTokens as string)
      if (req.query.temperature) opts.temperature = parseFloat(req.query.temperature as string)
      if (req.query.topP) opts.topP = parseFloat(req.query.topP as string)

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

      const id = req.params.id
      this.mgr
        .stream(id, prompt, callbacks, opts)
        .catch((err) => {
          callbacks.onError?.(err instanceof Error ? err.message : String(err))
        })
    })

    // ── SSE POST (for non-GET clients) ─────────────────────────────────────

    this.app.post("/v1/context/:id/stream", async (req, res) => {
      try {
        const { prompt, ...opts } = req.body
        if (!prompt) return res.status(400).json({ error: "prompt required" })

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

        await this.mgr.stream(req.params.id, prompt, callbacks, opts)
      } catch (err: any) {
        if (!res.writableEnded) {
          res.write(`data: ${JSON.stringify({ type: "error", error: err.message })}\n\n`)
          res.end()
        }
      }
    })

    // ── tokenize ───────────────────────────────────────────────────────────

    this.app.post("/v1/context/:id/tokenize", (req, res) => {
      try {
        const { text } = req.body
        if (!text) return res.status(400).json({ error: "text required" })
        const tokens = this.mgr.tokenize(req.params.id, text)
        res.json({ tokens })
      } catch (err: any) {
        res.status(500).json({ error: err.message })
      }
    })
  }

  async start(port = 3100): Promise<void> {
    const addr = `0.0.0.0:${port}`
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
