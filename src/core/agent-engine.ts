import * as path from "path"
import { promises as fsp } from "fs"
import { RwkvEngine } from "../engine/rwkv-engine.ts"
import { AgentLoop } from "./agent-loop.ts"
import { SessionManager } from "./session.ts"
import { GenerateOpts, DEFAULT_GEN_OPTS, SessionInfo, ChatMessage } from "./types.ts"
import { toolDefs, toolsToXml } from "./tool-registry.ts"

const SYSTEM_PREAMBLE = `You are a helpful AI assistant with file system access. You can read, write, edit files, list directories, and search file contents.`

export class AgentEngine {
  private engine: RwkvEngine
  private stateDir: string
  private currentLabel: string = "default"
  private sessions: Map<string, { label: string; messages: ChatMessage[] }> = new Map()
  private sessionManager: SessionManager

  constructor(engine: RwkvEngine, stateDir: string) {
    this.engine = engine
    this.stateDir = stateDir
    this.sessionManager = new SessionManager(stateDir, "_agent", "unknown")

    this.sessions.set("default", { label: "default", messages: [] })
  }

  async init() {
    await fsp.mkdir(this.stateDir, { recursive: true })
    await this.engine.bakeSystemPrompt(SYSTEM_PREAMBLE)
    await this.loadSessionIndex()
  }

  private sessionIndexPath(): string {
    return path.join(this.stateDir, "_sessions.json")
  }

  private async loadSessionIndex() {
    try {
      const raw = await fsp.readFile(this.sessionIndexPath(), "utf-8")
      const data = JSON.parse(raw)
      for (const s of data.sessions || []) {
        if (!this.sessions.has(s.label)) {
          this.sessions.set(s.label, { label: s.label, messages: s.messages || [] })
        }
      }
      if (data.currentLabel) {
        this.currentLabel = data.currentLabel
      }
    } catch {
    }
  }

  private async saveSessionIndex() {
    const data = {
      currentLabel: this.currentLabel,
      sessions: Array.from(this.sessions.values()).map((s) => ({
        label: s.label,
        messages: s.messages,
      })),
    }
    await fsp.writeFile(this.sessionIndexPath(), JSON.stringify(data, null, 2), "utf-8")
  }

  getCurrentSession(): SessionInfo {
    const s = this.sessions.get(this.currentLabel)!
    const statePath = this.engine.statePath(`session_${this.currentLabel}`)
    return {
      label: this.currentLabel,
      createdAt: "",
      updatedAt: "",
      statePath,
      messageCount: s.messages.length,
    }
  }

  async listSessions(): Promise<SessionInfo[]> {
    const result: SessionInfo[] = []
    for (const [label, s] of this.sessions) {
      result.push({
        label,
        createdAt: "",
        updatedAt: "",
        statePath: this.engine.statePath(`session_${label}`),
        messageCount: s.messages.length,
      })
    }
    return result
  }

  async createSession(label: string): Promise<SessionInfo> {
    const existing = this.sessions.get(this.currentLabel)
    if (existing && existing.messages.length > 0) {
      const cpName = `session_${this.currentLabel}`
      await this.engine.saveCheckpoint(cpName)
    }

    this.currentLabel = label
    if (!this.sessions.has(label)) {
      this.sessions.set(label, { label, messages: [] })
    }

    const statePath = this.engine.statePath(`session_${label}`)
    try {
      await this.engine.loadCheckpoint(`session_${label}`)
    } catch {
      await this.engine.loadBaseline()
    }

    await this.saveSessionIndex()

    return {
      label,
      createdAt: "",
      updatedAt: "",
      statePath,
      messageCount: this.sessions.get(label)!.messages.length,
    }
  }

  async switchSession(label: string): Promise<SessionInfo> {
    if (!this.sessions.has(label)) {
      throw new Error(`Session "${label}" not found`)
    }

    const current = this.sessions.get(this.currentLabel)
    if (current && current.messages.length > 0) {
      const cpName = `session_${this.currentLabel}`
      await this.engine.saveCheckpoint(cpName)
    }

    this.currentLabel = label
    try {
      await this.engine.loadCheckpoint(`session_${label}`)
    } catch {
      await this.engine.loadBaseline()
    }

    await this.saveSessionIndex()

    return {
      label,
      createdAt: "",
      updatedAt: "",
      statePath: this.engine.statePath(`session_${label}`),
      messageCount: this.sessions.get(label)!.messages.length,
    }
  }

  async deleteSession(label: string) {
    if (label === "default") throw new Error("Cannot delete default session")
    this.sessions.delete(label)

    const statePath = this.engine.statePath(`session_${label}`)
    try {
      await fsp.unlink(statePath)
    } catch {
    }

    if (this.currentLabel === label) {
      this.currentLabel = "default"
      await this.engine.loadBaseline()
    }

    await this.saveSessionIndex()
  }

  getMessages(label?: string): ChatMessage[] {
    const s = this.sessions.get(label || this.currentLabel)
    return s?.messages || []
  }

  async chat(
    prompt: string,
    callbacks?: { onToken?: (text: string) => void },
    opts: Partial<GenerateOpts> = {},
  ): Promise<string> {
    const s = this.sessions.get(this.currentLabel)!
    const sess = this.sessionManager.get()
    sess.status = "active"

    const systemPrompt = SYSTEM_PREAMBLE + "\n\n" + toolDefsToPrompt()
    let history = systemPrompt + "\n\n"
    for (const m of s.messages) {
      if (m.role === "user") history += `User: ${m.content}\n\n`
      else if (m.role === "assistant") history += `Assistant: ${m.content}\n\n`
    }
    const fullPrompt = history + `User: ${prompt}\n\nAssistant: `

    s.messages.push({ role: "user", content: prompt, timestamp: new Date().toISOString() })

    let finalText = ""
    const agentLoop = new AgentLoop(this.engine, this.sessionManager, 5)
    try {
      const result = await agentLoop.run(prompt, {
        onText: (t) => {
          finalText += t
          callbacks?.onToken?.(t)
        },
      }, opts)

      s.messages.push({ role: "assistant", content: result, timestamp: new Date().toISOString() })
      await this.saveSessionIndex()
      return result
    } finally {
      await agentLoop.dispose()
    }
  }

  async dispose() {
    const s = this.sessions.get(this.currentLabel)
    if (s && s.messages.length > 0) {
      const cpName = `session_${this.currentLabel}`
      try {
        await this.engine.saveCheckpoint(cpName)
      } catch {
      }
    }
    await this.saveSessionIndex()
  }
}

function toolDefsToPrompt(): string {
  return toolDefs.map((t) =>
    `- ${t.name}: ${t.description}`
  ).join("\n")
}
