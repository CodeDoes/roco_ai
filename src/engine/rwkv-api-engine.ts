import { promises as fsp } from "node:fs"
import { DEFAULT_GEN_OPTS, GenerateCallbacks } from "../core/types.ts"
import type { MoseBlendWeights, Engine, GenerateOpts } from "../core/types.ts"

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type GenOptsWithExtras = Partial<GenerateOpts> & Record<string, any>

interface StateInfo {
  filePath: string
  fileSize: number
}

interface SystemPromptState {
  baselinePath: string
  fileSize: number
}

export class RwkvApiEngine implements Engine {
  apiBase: string
  private stateDir: string
  private systemState: SystemPromptState | null = null
  mose: ApiMoSEEngine
  loraMgr = { list: () => [] as never[], getActive: () => [] as string[], add: () => {}, remove: () => false, activate: async () => {}, deactivateAll: async () => {} }

  constructor(modelPath: string, stateDir: string, apiBase: string = "http://localhost:3100") {
    this.apiBase = apiBase.replace(/\/+$/, "")
    this.stateDir = stateDir
    this.mose = new ApiMoSEEngine(this, this.stateDir)
  }

  async init(_gpu?: string, _loraPaths?: unknown) {
    const res = await fetch(`${this.apiBase}/health`)
    if (!res.ok) throw new Error(`API server unreachable: ${res.status}`)
    await res.json()
  }

  /** @internal */
  async apiGet<T>(path: string): Promise<T> {
    const res = await fetch(`${this.apiBase}${path}`)
    if (!res.ok) throw new Error(`API GET ${path}: ${res.status}`)
    return res.json()
  }

  /** @internal */
  async apiPost<T>(path: string, body: unknown): Promise<T> {
    const res = await fetch(`${this.apiBase}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    })
    if (!res.ok) {
      const text = await res.text().catch(() => "")
      throw new Error(`API POST ${path}: ${res.status}${text ? ` ${text}` : ""}`)
    }
    return res.json()
  }

  statePath(name: string): string {
    return `${this.stateDir}/_state_${name}.state`
  }

  tokenize(text: string): number[] {
    // Sync for now — the API call is fast. Convert to async if needed later.
    return [0]
  }

  async tokenizeAsync(text: string): Promise<number[]> {
    const resp = await this.apiPost<{ tokens: number[] }>("/v1/tokenize", { text })
    return resp.tokens
  }

  detokenize(tokens: number[]): string {
    return ""
  }

  async detokenizeAsync(tokens: number[]): Promise<string> {
    const resp = await this.apiPost<{ text: string }>("/v1/detokenize", { tokens })
    return resp.text
  }

  private baselinePath(): string {
    return `${this.stateDir}/_system_baseline.state`
  }

  async bakeSystemPrompt(systemPrompt: string): Promise<SystemPromptState> {
    const tokens = await this.tokenizeAsync(systemPrompt)
    await this.apiPost("/v1/eval", { tokens })
    const stateResp = await this.apiGet<{ state: string; size: number }>("/v1/state")
    const buf = Buffer.from(stateResp.state, "base64")
    await fsp.mkdir(this.stateDir, { recursive: true })
    await fsp.writeFile(this.baselinePath(), buf)
    this.systemState = { baselinePath: this.baselinePath(), fileSize: buf.length }
    await this.apiPost("/v1/state/clear", {})
    return this.systemState
  }

  async loadBaseline() {
    await this.loadCheckpoint("system_baseline")
  }

  async saveCheckpoint(name: string): Promise<StateInfo> {
    const stateResp = await this.apiGet<{ state: string; size: number }>("/v1/state")
    const buf = Buffer.from(stateResp.state, "base64")
    const filePath = this.statePath(name)
    await fsp.mkdir(this.stateDir, { recursive: true })
    await fsp.writeFile(filePath, buf)
    return { filePath, fileSize: buf.length }
  }

  async loadCheckpoint(name: string) {
    const filePath = this.statePath(name)
    const buf = await fsp.readFile(filePath)
    const b64 = buf.toString("base64")
    await this.apiPost("/v1/state", { state: b64 })
  }

  async evaluate(text: string) {
    const tokens = await this.tokenizeAsync(text)
    await this.apiPost("/v1/eval", { tokens })
  }

  async generate(
    prompt: string,
    opts: GenOptsWithExtras = {}
  ): Promise<string> {
    let result = ""
    await this.generateStream(prompt, { onText: (t) => { result += t } }, opts)
    return result
  }

  async generateStream(
    prompt: string,
    callbacks: GenerateCallbacks = {},
    opts: GenOptsWithExtras = {}
  ): Promise<string> {
    const o = { ...DEFAULT_GEN_OPTS, ...opts }
    const resp = await this.apiPost<{ text: string; tokens_generated: number }>("/v1/generate", {
      prompt,
      max_tokens: o.maxTokens,
      temperature: o.temperature,
      top_p: o.topP,
    })
    const text = resp.text
    callbacks.onText?.(text)
    callbacks.onDone?.()
    return text
  }

  async generateWithBlend(
    prompt: string,
    blend?: MoseBlendWeights,
    opts: GenOptsWithExtras = {}
  ): Promise<string> {
    const o = { ...DEFAULT_GEN_OPTS, ...opts }
    const weights = blend
    const resp = await this.apiPost<{ text: string; tokens_generated: number }>("/v1/mose/generate", {
      prompt,
      max_tokens: o.maxTokens,
      temperature: o.temperature,
      top_p: o.topP,
      blend: weights,
    })
    return resp.text
  }

  async generateWithSegments(
    segments: { text: string; blend: MoseBlendWeights }[],
    opts: GenOptsWithExtras = {}
  ): Promise<string> {
    const last = segments.pop()!
    for (const seg of segments) {
      // Capture state, blend, evaluate
      const stateResp = await this.apiGet<{ state: string }>("/v1/state")
      const state = stateResp.state
      // Temporarily store current state
      // Blend and evaluate via MoSE
    }
    return this.generateWithBlend(last.text, last.blend, opts)
  }

  getStateSize(): number {
    return 21626880
  }

  async dispose() {
    // no-op: server manages lifecycle
  }
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
class ApiMoSEEngine {
  [key: string]: any
  engine: RwkvApiEngine
  private stateDir: string
  private experts: Map<string, { name: string; weight: number }> = new Map()

  constructor(engine: RwkvApiEngine, stateDir: string) {
    this.engine = engine
    this.stateDir = stateDir
  }

  list(): { name: string; weight: number; stateFile: string }[] {
    return Array.from(this.experts.values()).map((e) => ({
      name: e.name,
      weight: e.weight,
      stateFile: "",
    }))
  }

  get(name: string): { name: string; weight: number } | undefined {
    return this.experts.get(name)
  }

  async createExpert(name: string, text: string, weight: number = 1.0): Promise<{ name: string; stateFile: string }> {
    // Evaluate text to build state, then get state and register via API
    const tokens = await this.engine.tokenizeAsync(text)
    await this.engine.apiPost("/v1/eval", { tokens })
    const stateResp = await this.engine.apiGet<{ state: string; size: number }>("/v1/state")
    await this.engine.apiPost("/v1/mose/expert", { name, state: stateResp.state, weight })
    this.experts.set(name, { name, weight })
    const statPath = `${this.stateDir}/_expert_${name}.api`
    await fsp.mkdir(this.stateDir, { recursive: true })
    await fsp.writeFile(statPath, JSON.stringify({ name, weight }))
    return { name, stateFile: statPath }
  }

  async loadExpert(name: string, stateFilePath: string, weight: number = 1.0): Promise<{ name: string; stateFile: string }> {
    const buf = await fsp.readFile(stateFilePath)
    const b64 = buf.toString("base64")
    await this.engine.apiPost("/v1/mose/expert", { name, state: b64, weight })
    this.experts.set(name, { name, weight })
    return { name, stateFile: stateFilePath }
  }

  async removeExpert(name: string): Promise<boolean> {
    await this.engine.apiGet(`/v1/mose/expert/${name}`)
    this.experts.delete(name)
    return true
  }

  setWeight(name: string, weight: number): boolean {
    const expert = this.experts.get(name)
    if (!expert) return false
    expert.weight = weight
    return true
  }

  setWeights(weights: MoseBlendWeights): void {
    for (const [name, weight] of Object.entries(weights)) {
      this.setWeight(name, weight)
    }
  }

  async blend(weights?: MoseBlendWeights): Promise<void> {
    if (weights) this.setWeights(weights)
    const w: Record<string, number> = {}
    for (const [name, e] of this.experts) {
      w[name] = e.weight
    }
    await this.engine.apiPost("/v1/mose/blend", { weights: w })
  }

  async apply(weights?: MoseBlendWeights): Promise<void> {
    await this.blend(weights)
  }

  async segmentRoute(segments: { text: string; blend: MoseBlendWeights }[]): Promise<void> {
    for (const seg of segments) {
      await this.apply(seg.blend)
      const tokens = await this.engine.tokenizeAsync(seg.text)
      await this.engine.apiPost("/v1/eval", { tokens })
    }
  }

  async dispose(): Promise<void> {
    this.experts.clear()
  }
}
