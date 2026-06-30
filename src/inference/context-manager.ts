import * as fsp from "fs/promises"
import * as fs from "fs"
import * as path from "path"
import { getLlama, LlamaModel, LlamaContext, LlamaContextSequence, LlamaGrammar, LlamaGrammarEvaluationState } from "node-llama-cpp"
import { GenerateOpts, DEFAULT_GEN_OPTS } from "../core/types.ts"

// ─── types ──────────────────────────────────────────────────────────────────

export interface ContextHandle {
  id: string
  modelPath: string
  model: LlamaModel | null
  context: LlamaContext | null
  sequence: LlamaContextSequence | null
  loras: string[]
  stateSlots: Map<string, { path: string; size: number }>
  gpu: string
  contextSize: number
  createdAt: number
  lastUsed: number
}

export interface StreamCallbacks {
  onToken?: (text: string) => void
  onDone?: (meta: { tokens: number; text: string }) => void
  onError?: (err: string) => void
}

// ─── Context manager ─────────────────────────────────────────────────────────

export class ContextManager {
  private contexts: Map<string, ContextHandle> = new Map()
  private llama: Awaited<ReturnType<typeof getLlama>> | null = null
  private slotsDir: string
  private _globalModel: LlamaModel | null = null
  private _globalModelPath: string = ""

  constructor(slotsDir: string) {
    this.slotsDir = slotsDir
  }

  private async ensureLlama(): Promise<Awaited<ReturnType<typeof getLlama>>> {
    if (!this.llama) {
      this.llama = await getLlama({ gpu: "vulkan" })
    }
    return this.llama
  }

  private touch(id: string) {
    const c = this.contexts.get(id)
    if (c) c.lastUsed = Date.now()
  }

  private tmp(suffix: string): string {
    return path.join(this.slotsDir, `_tmp_${Date.now()}_${Math.random().toString(36).slice(2)}${suffix}`)
  }

  // ── lifecycle ────────────────────────────────────────────────────────────

  async createContext(opts: { gpu?: string; contextSize?: number } = {}): Promise<ContextHandle> {
    await this.ensureLlama()
    const id = `ctx_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`
    const handle: ContextHandle = {
      id,
      modelPath: "",
      model: null,
      context: null,
      sequence: null,
      loras: [],
      stateSlots: new Map(),
      gpu: opts.gpu ?? "vulkan",
      contextSize: opts.contextSize ?? 8192,
      createdAt: Date.now(),
      lastUsed: Date.now(),
    }
    this.contexts.set(id, handle)
    return handle
  }

  get(id: string): ContextHandle | undefined {
    this.touch(id)
    return this.contexts.get(id)
  }

  list() {
    return Array.from(this.contexts.values()).map((c) => ({
      id: c.id,
      modelPath: c.modelPath,
      loras: [...c.loras],
      slots: Array.from(c.stateSlots.keys()),
      lastUsed: c.lastUsed,
    }))
  }

  async destroy(id: string): Promise<void> {
    const c = this.contexts.get(id)
    if (!c) return
    await this.disposeContext(c)
    this.contexts.delete(id)
  }

  // ── model ────────────────────────────────────────────────────────────────

  async loadModel(id: string, modelPath: string, loraPaths?: string[]): Promise<void> {
    const c = this.get(id)
    if (!c) throw new Error(`Context not found: ${id}`)
    await this.disposeContext(c)
    let model: LlamaModel
    if (this._globalModel && this._globalModelPath === modelPath) {
      model = this._globalModel
    } else {
      if (this._globalModel) {
        try { await this._globalModel.dispose() } catch { /* */ }
        this._globalModel = null
        this._globalModelPath = ""
      }
      const llama = await this.ensureLlama()
      model = await llama.loadModel({ modelPath })
      this._globalModel = model
      this._globalModelPath = modelPath
    }
    const loras = loraPaths ?? []
    const loraOpts = loras.length > 0 ? { lora: loras.map((p) => ({ filePath: p, scale: 1.0 })) as any } : {}
    const ctx = await model.createContext({ contextSize: c.contextSize, ...loraOpts })
    const seq = ctx.getSequence()
    c.modelPath = modelPath
    c.model = model
    c.context = ctx
    c.sequence = seq
    c.loras = loras
    this.touch(id)
  }

  async unloadModel(id: string): Promise<void> {
    const c = this.get(id)
    if (!c) throw new Error(`Context not found: ${id}`)
    await this.disposeContext(c)
    c.modelPath = ""
    c.model = null
    c.context = null
    c.sequence = null
    c.loras = []
  }

  // ── state ────────────────────────────────────────────────────────────────

  async loadState(id: string, stateBase64: string): Promise<void> {
    const c = this.get(id)
    if (!c?.sequence) throw new Error("Model not loaded on context")
    const bytes = Buffer.from(stateBase64, "base64")
    const tmp = this.tmp(".state")
    await fsp.writeFile(tmp, bytes)
    try {
      await c.sequence.loadStateFromFile(tmp, { acceptRisk: true })
    } finally {
      await fsp.unlink(tmp).catch(() => {})
    }
  }

  async saveStateSlot(id: string, slotId: string): Promise<{ path: string; size: number }> {
    const c = this.get(id)
    if (!c?.sequence) throw new Error("Model not loaded on context")
    const slotPath = path.join(this.slotsDir, `${id}__${slotId}.state`)
    const { fileSize } = await c.sequence.saveStateToFile(slotPath)
    c.stateSlots.set(slotId, { path: slotPath, size: fileSize })
    return { path: slotPath, size: fileSize }
  }

  async loadStateSlot(id: string, slotId: string): Promise<void> {
    const c = this.get(id)
    if (!c?.sequence) throw new Error("Model not loaded on context")
    const slot = c.stateSlots.get(slotId)
    if (!slot) throw new Error(`State slot not found: ${slotId}`)
    await c.sequence.loadStateFromFile(slot.path, { acceptRisk: true })
  }

  async downloadStateSlot(id: string, slotId: string): Promise<{ state: string; size: number }> {
    const c = this.get(id)
    const slot = c?.stateSlots.get(slotId)
    if (!slot) throw new Error(`State slot not found: ${slotId}`)
    if (!fs.existsSync(slot.path)) throw new Error(`State file missing: ${slot.path}`)
    const bytes = await fsp.readFile(slot.path)
    return { state: Buffer.from(bytes).toString("base64"), size: bytes.length }
  }

  async uploadStateSlot(id: string, slotId: string, stateBase64: string): Promise<{ size: number }> {
    const c = this.get(id)
    if (!c) throw new Error(`Context not found: ${id}`)
    const bytes = Buffer.from(stateBase64, "base64")
    const slotPath = path.join(this.slotsDir, `${id}__${slotId}.state`)
    await fsp.writeFile(slotPath, bytes)
    c.stateSlots.set(slotId, { path: slotPath, size: bytes.length })
    return { size: bytes.length }
  }

  // ── LoRA ─────────────────────────────────────────────────────────────────

  async loadLora(id: string, loraPath: string): Promise<void> {
    const c = this.get(id)
    if (!c?.model) throw new Error("Model not loaded on context")
    if (c.loras.includes(loraPath)) return
    const saved = c.sequence ? await this.snapState(c.sequence) : null
    const newLoras = [...c.loras, loraPath]
    await this.loadModel(id, c.modelPath, newLoras)
    if (c.sequence && saved) {
      await c.sequence.loadStateFromFile(saved, { acceptRisk: true }).catch(() => {})
      await fsp.unlink(saved).catch(() => {})
    }
  }

  async unloadLora(id: string, loraPath?: string): Promise<void> {
    const c = this.get(id)
    if (!c?.model) throw new Error("Model not loaded on context")
    const remaining = loraPath ? c.loras.filter((p) => p !== loraPath) : []
    if (remaining.length === c.loras.length) return
    const saved = c.sequence ? await this.snapState(c.sequence) : null
    await this.loadModel(id, c.modelPath, remaining.length > 0 ? remaining : undefined)
    if (c.sequence && saved) {
      await c.sequence.loadStateFromFile(saved, { acceptRisk: true }).catch(() => {})
      await fsp.unlink(saved).catch(() => {})
    }
  }

  // ── MoSE ─────────────────────────────────────────────────────────────────

  async useMoSE(id: string, weights: Record<string, number>): Promise<void> {
    const c = this.get(id)
    if (!c?.sequence) throw new Error("Model not loaded on context")
    const entries = Object.entries(weights)
    if (entries.length === 0) return
    const files = entries.map(([sid]) => {
      const slot = c.stateSlots.get(sid)
      if (!slot) throw new Error(`State slot not found: ${sid}`)
      return slot.path
    })
    const raw = await Promise.all(files.map((f) => fsp.readFile(f)))
    const n = raw[0].length / 4
    const result = new Float32Array(n)
    let wsum = 0
    for (let i = 0; i < entries.length; i++) {
      const floats = new Float32Array(raw[i].buffer, raw[i].byteOffset, n)
      const w = entries[i][1]
      wsum += w
      for (let j = 0; j < n; j++) result[j] += floats[j] * w
    }
    if (wsum > 0 && Math.abs(wsum - 1.0) > 1e-6) {
      for (let j = 0; j < n; j++) result[j] /= wsum
    }
    const blended = this.tmp(".mose.state")
    await fsp.writeFile(blended, Buffer.from(result.buffer))
    try {
      await c.sequence.loadStateFromFile(blended, { acceptRisk: true })
    } finally {
      await fsp.unlink(blended).catch(() => {})
    }
  }

  async useLose(_id: string, _weights: Record<string, number>): Promise<void> {
    throw new Error("LoSE not yet implemented — use loadLora/unloadLora")
  }

  // ── tokenize ─────────────────────────────────────────────────────────────

  tokenize(id: string, text: string): number[] {
    const c = this.get(id)
    if (!c?.model) throw new Error("Model not loaded on context")
    return c.model.tokenize(text)
  }

  // ── evaluate (processContext) ────────────────────────────────────────────

  async processContext(id: string, text: string): Promise<{ tokens: number }> {
    const c = this.get(id)
    if (!c?.sequence || !c?.model) throw new Error("Model not loaded on context")
    const tokens = c.model.tokenize(text)
    await c.sequence.evaluateWithoutGeneratingNewTokens(tokens)
    return { tokens: tokens.length }
  }

  // ── generate / stream ────────────────────────────────────────────────────

  async generate(id: string, prompt: string, opts: Partial<GenerateOpts> = {}): Promise<string> {
    let out = ""
    await this.stream(id, prompt, { onToken: (t) => { out += t } }, opts)
    return out
  }

  async stream(id: string, prompt: string, cbs: StreamCallbacks, opts: Partial<GenerateOpts> = {}): Promise<void> {
    const c = this.get(id)
    if (!c?.sequence || !c?.model) throw new Error("Model not loaded on context")
    this.touch(id)
    const genOpts = { ...DEFAULT_GEN_OPTS, ...opts }
    const tokens = c.model.tokenize(prompt)
    const ctx = c.context as any
    let grammarEvalState: LlamaGrammarEvaluationState | undefined
    if (genOpts.grammar) {
      const grammar = await ctx.createGrammar({ grammar: genOpts.grammar })
      grammarEvalState = new LlamaGrammarEvaluationState({ model: c.model, grammar })
    }
    const stopSeqs: string[] = (opts as any).stopSequences ?? []
    const gen = c.sequence.evaluate(tokens, {
      maxTokens: genOpts.maxTokens,
      temperature: genOpts.temperature,
      topP: genOpts.topP,
      repeatPenalty: {
        punishTokens: [] as number[],
        penalty: genOpts.repeatPenalty,
        frequencyPenalty: genOpts.frequencyPenalty,
        presencePenalty: genOpts.presencePenalty,
      },
      grammarEvaluationState: grammarEvalState as any,
      yieldEogToken: true,
    } as any)
    let result = ""
    let count = 0
    try {
      for await (const token of gen) {
        if (c.model.isEogToken(token)) break
        const text = c.model.detokenize([token])
        result += text
        count++
        cbs.onToken?.(text)
        for (const seq of stopSeqs) {
          if (result.includes(seq)) break
        }
        if (stopSeqs.some((seq) => result.includes(seq))) break
      }
      cbs.onDone?.({ tokens: count, text: result })
    } catch (err: any) {
      cbs.onError?.(err.message ?? String(err))
    }
  }

  // ── internal ────────────────────────────────────────────────────────────

  private async snapState(seq: LlamaContextSequence): Promise<string> {
    const tmp = this.tmp(".state")
    await seq.saveStateToFile(tmp)
    return tmp
  }

  private async disposeContext(c: ContextHandle): Promise<void> {
    try { c.sequence?.dispose() } catch { /* already disposed */ }
    try { c.context?.dispose() } catch { /* already disposed */ }
    c.sequence = null
    c.context = null
  }
}
