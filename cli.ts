#!/usr/bin/env node
import { promises as fsp } from "fs"
import * as path from "path"
import { fileURLToPath } from "url"
import { RwkvEngine } from "./src/rwkv-engine.ts"
import { SessionManager } from "./src/session.ts"
import { StorytellerAgent } from "./src/storyteller.ts"
import { AgentLoop } from "./src/agent-loop.ts"
import { AgentEngine } from "./src/agent-engine.ts"
import { GatewayServer } from "./src/gateway/server.ts"
import { Tui } from "./tui/index.ts"
import { GenerateOpts, DEFAULT_GEN_OPTS } from "./src/types.ts"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const PROJECT_ROOT = path.resolve(__dirname)

const args = process.argv.slice(2)
const command = args[0]
const modelPath = args.find((a) => a.startsWith("--model="))?.split("=")[1]
  || path.join(PROJECT_ROOT, "models/rwkv7-g1g-2.9b-20260526-ctx8192-Q4_K_M.gguf")
const story = args.find((a) => a.startsWith("--story="))?.split("=")[1] || "default"
const gpuArg = (args.find((a) => a.startsWith("--gpu="))?.split("=")[1] || "vulkan") as "vulkan" | "cuda" | "auto"
const loraRaw = args.find((a) => a.startsWith("--lora="))?.split("=")[1]
const loraPaths = loraRaw ? loraRaw.split(",").map((p) => p.startsWith("/") ? p : path.join(PROJECT_ROOT, p)) : undefined
const fixParagraphs = args.includes("--fix-paragraphs") || args.includes("-p")
const agentDepth = parseInt(args.find((a) => a.startsWith("--depth="))?.split("=")[1] || "5", 10)
const grammarPath = args.find((a) => a.startsWith("--grammar="))?.split("=")[1]
const gatewayPort = parseInt(args.find((a) => a.startsWith("--port="))?.split("=")[1] || "3030", 10)
const input = args.slice(1).filter((a) => !a.startsWith("--")).join(" ")
const stateDir = path.join(PROJECT_ROOT, "s", story)

async function main() {
  switch (command) {
    case "gateway":
      return runGateway()
    case "tui":
      return runTui()
    default:
      return runCli()
  }
}

async function runGateway() {
  console.error(`RWKV Gateway | port: ${gatewayPort} | model: ${path.basename(modelPath)}`)

  const engine = new RwkvEngine(modelPath, path.join(PROJECT_ROOT, "s", "_gateway"))
  await engine.init(gpuArg, loraPaths)
  const agent = new AgentEngine(engine, path.join(PROJECT_ROOT, "s", "_gateway"))
  await agent.init()
  const server = new GatewayServer(agent, path.join(PROJECT_ROOT, "webapp"))

  await server.start(gatewayPort)
  console.error(`  API:  http://0.0.0.0:${gatewayPort}`)
  console.error(`  WS:   ws://0.0.0.0:${gatewayPort}`)
  console.error(`  Web:  http://0.0.0.0:${gatewayPort}`)
  console.error(`  Sessions: ${(await agent.listSessions()).length}`)

  const shutdown = async () => {
    console.error("\nShutting down...")
    await server.stop()
    process.exit(0)
  }
  process.on("SIGINT", shutdown)
  process.on("SIGTERM", shutdown)
}

async function runTui() {
  const mode = args.includes("--connect") ? "gateway_client" : "direct"
  const gatewayHost = args.find((a) => a.startsWith("--host="))?.split("=")[1]

  const tui = new Tui({
    modelPath,
    stateDir,
    story,
    gpu: gpuArg,
    loraPaths,
    fixParagraphs,
    agentDepth,
    grammar: grammarPath ? await fsp.readFile(grammarPath, "utf-8") : undefined,
    gatewayPort,
    mode: mode as any,
    gatewayHost,
  })

  await tui.start()
}

async function runCli() {
  const engine = new RwkvEngine(modelPath, stateDir)
  const session = new SessionManager(stateDir, story, modelPath)
  const agent = new StorytellerAgent(engine, session, { fixParagraphBreak: fixParagraphs })

  let cleanupAgent: () => Promise<void> = () => agent.dispose()
  let shutdown = false

  async function cleanup(signal: string) {
    if (shutdown) return
    shutdown = true
    console.error(`\n${signal} - saving state...`)
    await cleanupAgent()
    process.exit(0)
  }

  process.on("SIGINT", () => cleanup("SIGINT"))
  process.on("SIGTERM", () => cleanup("SIGTERM"))

  console.error(`RWKV CLI | model: ${path.basename(modelPath)} | gpu: ${gpuArg} | story: ${story}`)
  if (loraPaths) console.error(`LoRA: ${loraPaths.join(", ")}`)
  if (fixParagraphs) console.error("Fix-paragraph-break enabled")
  console.error(`State: ${stateDir}`)
  console.error("---")

  await engine.init(gpuArg, loraPaths)
  await agent.init()

  let grammar: string | undefined
  if (grammarPath) {
    grammar = await fsp.readFile(grammarPath, "utf-8")
  }

  const genOpts: Partial<GenerateOpts> = { grammar }

  switch (command) {
    case "tell": {
      const prompt = input || "Continue the story."
      console.error(`\nPrompt: ${prompt}\n`)
      const result = await agent.continueStoryStream(prompt, (t) => process.stdout.write(t), genOpts)
      console.error(`\n---\nGenerated ${result.length} chars`)
      break
    }

    case "agent": {
      const prompt = input || "What would you like to do?"
      console.error(`\nAgent mode | max depth: ${agentDepth}\n`)
      const agentLoop = new AgentLoop(engine, session, agentDepth)
      cleanupAgent = () => agentLoop.dispose()
      const result = await agentLoop.run(prompt, {
        onText: (t) => process.stdout.write(t),
      }, genOpts)
      console.error(`\n---\nGenerated ${result.length} chars`)
      break
    }

    case "chapter": {
      const chapterNum = parseInt(args.find((a) => a.startsWith("--num="))?.split("=")[1] || "1", 10)
      const slug = args.find((a) => a.startsWith("--slug="))?.split("=")[1] || `chapter_${String(chapterNum).padStart(3, "0")}`
      const prompt = input || `Write chapter ${chapterNum}.`
      console.error(`Chapter ${chapterNum} | slug: ${slug}\n`)
      const result = await agent.continueStoryStream(prompt, (t) => process.stdout.write(t), genOpts)
      await agent.saveChapterCheckpoint(chapterNum, slug)
      console.error(`\n---\nSaved checkpoint for chapter ${chapterNum}`)
      break
    }

    case "checkpoint": {
      const sub = args[1]
      if (sub === "save") {
        const name = args[2] || `manual_${Date.now()}`
        const info = await engine.saveCheckpoint(name)
        session.registerCheckpoint(name, engine.statePath(name))
        await session.save()
        console.error(`Saved checkpoint "${name}" (${info.fileSize} bytes)`)
      } else if (sub === "load") {
        const name = args[2]
        if (!name) { console.error("Usage: checkpoint load <name>"); break }
        await engine.loadCheckpoint(name)
        console.error(`Loaded checkpoint "${name}"`)
      } else if (sub === "ls") {
        const sess = session.get()
        const cps = Object.entries(sess.statePaths.checkpoints)
        if (cps.length === 0) { console.error("No checkpoints"); break }
        for (const [name, fp] of cps) {
          const stat = await fsp.stat(fp).catch(() => null)
          const size = stat ? `(${(stat.size / 1024).toFixed(1)} KB)` : "(missing)"
          console.error(`  ${name} ${size}`)
        }
      } else {
        console.error("Usage: checkpoint save|load|ls [name]")
      }
      break
    }

    case "plan": {
      const prompt = input || "Create a story plan with chapters, characters, and worldbuilding."
      const planPrompt = `${prompt}\n\nWrite a detailed story plan as a structured outline:`
      console.error(`\nGenerating plan...\n`)
      const result = await engine.generate(planPrompt, { ...DEFAULT_GEN_OPTS, maxTokens: 2048, temperature: 0.9, ...genOpts })
      console.log(result)
      const planPath = path.join(stateDir, "_plan.md")
      await fsp.mkdir(stateDir, { recursive: true })
      await fsp.writeFile(planPath, result, "utf-8")
      console.error(`\nPlan saved to ${planPath}`)
      break
    }

    case "interactive": {
      console.error("\nInteractive mode. Type 'exit' to quit, 'save' to checkpoint.\n")
      while (!shutdown) {
        const prompt = await new Promise<string>((resolve) => {
          process.stdout.write("\n> ")
          let buf = ""
          const stdin = process.stdin
          stdin.resume()
          const onData = (chunk: Buffer) => {
            const text = chunk.toString()
            if (text.includes("\n")) {
              buf += text.slice(0, text.indexOf("\n"))
              stdin.pause()
              stdin.removeListener("data", onData)
              resolve(buf.trim())
            } else {
              buf += text
            }
          }
          stdin.on("data", onData)
        })

        const inp = prompt
        if (!inp || inp === "exit") break
        if (inp === "save") {
          const name = `interactive_${Date.now()}`
          const info = await engine.saveCheckpoint(name)
          session.registerCheckpoint(name, engine.statePath(name))
          await session.save()
          console.error(`Checkpoint saved (${info.fileSize} bytes)`)
          continue
        }

        process.stdout.write("\n")
        const result = await agent.continueStoryStream(inp, (t) => process.stdout.write(t), genOpts)
        process.stdout.write("\n")
      }
      break
    }

    case "continue": {
      const sess = session.get()
      const cpNames = Object.keys(sess.statePaths.checkpoints)
      if (cpNames.length > 0) {
        const last = cpNames[cpNames.length - 1]
        await engine.loadCheckpoint(last)
        console.error(`Loaded checkpoint: ${last}`)
      } else {
        console.error("No checkpoint found, starting from baseline")
        await agent.resumeFromBaseline()
      }
      const prompt = input || "Continue the story from here."
      console.error(`\nPrompt: ${prompt}\n`)
      const result = await agent.continueStoryStream(prompt, (t) => process.stdout.write(t), genOpts)
      console.error(`\n---\nGenerated ${result.length} chars`)
      break
    }

    case "state-info": {
      try {
        const sess = session.get()
        const stateSize = engine.getStateSize()
        console.error(`State size: ${stateSize} bytes (${(stateSize / 1024 / 1024).toFixed(2)} MB)`)
        console.error(`Messages: ${sess.messages.length}`)
        console.error(`Steps: ${sess.stepCount}`)
        console.error(`Status: ${sess.status}`)
        console.error(`Checkpoints: ${Object.keys(sess.statePaths.checkpoints).length}`)
      } catch (e) {
        console.error(`Error: ${e}`)
      }
      break
    }

    default:
      console.error(`
Usage: pnpm tsx cli.ts <command> [options]

Commands:
  gateway              Start gateway (engine + API + WS broadcast)
  tui                  Terminal UI (--connect to connect to running gateway)
  tell [prompt]        Generate story text
  agent [prompt]       Agent mode with tool use
  chapter --num=N      Write a chapter, save checkpoint
  checkpoint save|load|ls
  plan [prompt]        Generate story plan
  interactive          Interactive story mode
  continue [prompt]    Continue from latest checkpoint
  state-info           Show engine/session state info

Options:
  --model=PATH         Model path
  --story=NAME         Story slug
  --gpu=TYPE           GPU backend: vulkan | cuda | auto
  --lora=PATH          LoRA adapter(s)
  --depth=N            Max agent loop depth (default: 5)
  --grammar=PATH       GBNF grammar file
  --port=N             Gateway port (default: 3030)
  --host=URL           Gateway URL for --connect mode
  --connect            TUI connects to running gateway
  --fix-paragraphs, -p Continue past \\n\\n EOS boundary
`)
      process.exit(1)
  }

  await agent.dispose()
}

main().catch((err) => {
  console.error(`Fatal: ${err.message}`)
  process.exit(1)
})
