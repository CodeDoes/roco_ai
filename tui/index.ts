import * as readline from "readline"
import { RwkvEngine } from "../src/rwkv-engine.ts"
import { SessionManager } from "../src/session.ts"
import { AgentLoop } from "../src/agent-loop.ts"
import { StorytellerAgent } from "../src/storyteller.ts"
import * as path from "path"
import { DEFAULT_GEN_OPTS } from "../src/types.ts"

const PROJECT_ROOT = path.resolve(import.meta.dirname!, "..")

interface TuiConfig {
  modelPath: string
  stateDir: string
  story: string
  gpu: "vulkan" | "cuda" | "auto"
  loraPaths?: string[]
  fixParagraphs?: boolean
  agentDepth?: number
  grammar?: string
}

export class TuiChannel {
  private engine: RwkvEngine
  private session: SessionManager
  private loop: AgentLoop
  private storyAgent: StorytellerAgent
  private rl: readline.Interface
  private config: TuiConfig
  private mode: "agent" | "story" = "agent"

  constructor(config: TuiConfig) {
    this.config = config
    this.engine = new RwkvEngine(config.modelPath, config.stateDir)
    this.session = new SessionManager(config.stateDir, config.story, config.modelPath)
    this.loop = new AgentLoop(this.engine, this.session, config.agentDepth || 5)
    this.storyAgent = new StorytellerAgent(this.engine, this.session, {
      fixParagraphBreak: config.fixParagraphs,
    })

    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
      prompt: "",
    })
  }

  async init() {
    await this.engine.init(this.config.gpu, this.config.loraPaths)
    await this.session.load()
    await this.session.ensureDir()
    if (this.session.get().status === "new") {
      await this.engine.bakeSystemPrompt("You are a helpful AI assistant with file system access.")
      await this.session.save()
    } else {
      await this.engine.loadBaseline()
    }
  }

  async start() {
    console.error("\x1b[36mRWKV TUI\x1b[0m | story: \x1b[33m" + this.config.story + "\x1b[0m | mode: " + this.mode)
    console.error("Commands: /mode agent|story, /save, /load <name>, /clear, /exit")
    console.error("---")

    this.rl.on("line", async (line) => {
      const input = line.trim()
      if (!input) { this.prompt(); return }

      if (input.startsWith("/")) {
        await this.handleCommand(input)
        this.prompt()
        return
      }

      process.stdout.write("\n")

      try {
        if (this.mode === "agent") {
          const result = await this.loop.run(input, {
            onText: (t) => process.stdout.write(t),
          })
          process.stdout.write("\n\n")
        } else {
          const result = await this.storyAgent.continueStoryStream(input, (t) => process.stdout.write(t))
          process.stdout.write("\n\n")
        }
      } catch (err: any) {
        console.error("\x1b[31mError:\x1b[0m", err.message)
      }

      this.prompt()
    })

    this.prompt()
  }

  private async handleCommand(cmd: string) {
    const parts = cmd.slice(1).split(/\s+/)
    const verb = parts[0]

    switch (verb) {
      case "mode": {
        const m = parts[1]
        if (m === "agent" || m === "story") {
          this.mode = m
          console.error("\x1b[32mMode:\x1b[0m", this.mode)
        } else {
          console.error("Usage: /mode agent|story")
        }
        break
      }
      case "save": {
        const name = parts[1] || `tui_${Date.now()}`
        const info = await this.engine.saveCheckpoint(name)
        this.session.registerCheckpoint(name, info.filePath)
        await this.session.save()
        console.error("\x1b[32mSaved:\x1b[0m", name, `(${(info.fileSize / 1024).toFixed(1)} KB)`)
        break
      }
      case "load": {
        const name = parts[1]
        if (!name) { console.error("Usage: /load <name>"); break }
        try {
          await this.engine.loadCheckpoint(name)
          console.error("\x1b[32mLoaded:\x1b[0m", name)
        } catch (e: any) {
          console.error("\x1b[31mError:\x1b[0m", e.message)
        }
        break
      }
      case "clear": {
        console.clear()
        break
      }
      case "exit":
      case "quit": {
        await this.dispose()
        process.exit(0)
      }
      default:
        console.error("Unknown command:", verb)
    }
  }

  private prompt() {
    const prefix = this.mode === "agent" ? "\x1b[36magent\x1b[0m" : "\x1b[33mstory\x1b[0m"
    this.rl.setPrompt(`[${prefix}]> `)
    this.rl.prompt()
  }

  async dispose() {
    await this.loop.dispose()
    await this.storyAgent.dispose()
    this.rl.close()
  }
}
