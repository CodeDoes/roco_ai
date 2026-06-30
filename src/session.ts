import { promises as fsp } from "fs"
import * as path from "path"
import { RwkvSession, RwkvMessage } from "./types.ts"

export class SessionManager {
  private sessionDir: string
  private sessionFile: string
  private session: RwkvSession

  constructor(storyDir: string, story: string, model: string) {
    this.sessionDir = storyDir
    this.sessionFile = path.join(storyDir, "_session.json")
    this.session = {
      story,
      model,
      messages: [],
      stepCount: 0,
      status: "new",
      statePaths: {
        baseline: path.join(storyDir, "_system_baseline.state"),
        checkpoints: {},
        latest: null,
      },
    }
  }

  async load(): Promise<RwkvSession> {
    try {
      const raw = await fsp.readFile(this.sessionFile, "utf-8")
      this.session = JSON.parse(raw)
      return this.session
    } catch {
      return this.session
    }
  }

  async save(): Promise<void> {
    this.session.updatedAt = new Date().toISOString()
    this.session.stepCount = this.session.messages.filter((m) => m.role === "assistant").length
    await fsp.mkdir(this.sessionDir, { recursive: true })
    await fsp.writeFile(this.sessionFile, JSON.stringify(this.session, null, 2), "utf-8")
  }

  get(): RwkvSession {
    return this.session
  }

  addMessage(msg: RwkvMessage) {
    this.session.messages.push(msg)
  }

  buildPrompt(systemPrompt: string, useRoles = false): string {
    const msgs = this.session.messages
    let prompt = systemPrompt + "\n\n"
    for (const m of msgs) {
      switch (m.role) {
        case "user":
          prompt += `${useRoles ? "User: " : ""}${m.content}\n\n`
          break
        case "assistant":
          prompt += `${useRoles ? "Assistant: " : ""}${this.stripThinkBlock(m.content)}\n\n`
          break
        case "tool":
          prompt += `[Tool result: ${m.content.slice(0, 200)}]\n\n`
          break
      }
    }
    return prompt
  }

  private stripThinkBlock(text: string): string {
    return text
      .replace(/^Assistant:\s*/i, "")
      .replace(/<think>[\s\S]*?<\/think>\n*/g, "")
      .trim()
  }

  registerCheckpoint(name: string, filePath: string) {
    this.session.statePaths.checkpoints[name] = filePath
    this.session.statePaths.latest = filePath
  }

  getLatestCheckpoint(): string | null {
    return this.session.statePaths.latest
      ? path.resolve(this.session.statePaths.latest)
      : null
  }

  async ensureDir() {
    await fsp.mkdir(this.sessionDir, { recursive: true })
  }

  stateFilePath(name: string): string {
    return path.join(this.sessionDir, `_state_${name}.state`)
  }

  async saveLog(text: string) {
    await fsp.appendFile(path.join(this.sessionDir, "_agent.log"), text, "utf-8")
  }
}
