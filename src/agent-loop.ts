import { RwkvEngine } from "./rwkv-engine.ts"
import { SessionManager } from "./session.ts"
import { GenerateOpts, DEFAULT_GEN_OPTS, GenerateCallbacks, ToolCall, ToolResult } from "./types.ts"
import { toolDefs, toolHandlers, toolsToXml } from "./tool-registry.ts"

const SYSTEM_PREAMBLE = `You can use tools to read and write files. When you need to use a tool, output:

<tool_call>
{"name": "tool_name", "args": { ... }}
</tool_call>

Then I'll run the tool and give you the result.`

const TOOL_CALL_RE = /<tool_call>\s*(\{[\s\S]*?\})\s*<\/tool_call>/g

export class AgentLoop {
  private engine: RwkvEngine
  private session: SessionManager
  private maxDepth: number

  constructor(engine: RwkvEngine, session: SessionManager, maxDepth = 5) {
    this.engine = engine
    this.session = session
    this.maxDepth = maxDepth
  }

  async run(
    userInput: string,
    callbacks?: GenerateCallbacks,
    opts: Partial<GenerateOpts> = {},
  ): Promise<string> {
    const sess = this.session.get()
    sess.status = "active"

    const history = this.session.buildPrompt(this.buildSystemPrompt(), true)
    let fullPrompt = history + "User: " + userInput + "\n\nAssistant: "
    let finalText = ""
    let depth = 0

    while (depth < this.maxDepth) {
      const raw = await this.engine.generate(fullPrompt, {
        ...DEFAULT_GEN_OPTS,
        temperature: 0.7,
        ...opts,
      })

      const { text, toolCalls } = this.parseToolCalls(raw)
      callbacks?.onText?.(text)
      finalText += text

      const cpName = `agent_turn_${String(depth).padStart(2, "0")}`
      const cpInfo = await this.engine.saveCheckpoint(cpName)
      this.session.registerCheckpoint(cpName, cpInfo.filePath)
      await this.session.save()

      if (toolCalls.length === 0) break

      for (const call of toolCalls) {
        const preCpName = `agent_pretool_${String(depth).padStart(2, "0")}_${call.name}`
        const preCpInfo = await this.engine.saveCheckpoint(preCpName)
        this.session.registerCheckpoint(preCpName, preCpInfo.filePath)

        const result = await this.executeTool(call)
        const resultBlock = this.formatToolResult(result)
        fullPrompt += raw + "\n\nUser: " + resultBlock + "\n\nAssistant: "
      }
      await this.session.save()
      depth++
    }

    callbacks?.onDone?.()

    const cleaned = this.cleanOutput(finalText)
    this.session.addMessage({ role: "user", content: userInput })
    this.session.addMessage({ role: "assistant", content: cleaned })
    await this.session.save()

    return cleaned
  }

  private buildSystemPrompt(): string {
    return SYSTEM_PREAMBLE + "\n\nTools:\n" + toolsToXml() + "\n\nExamples:\n\nUser: list files in /tmp\n\nAssistant: <tool_call>\n{\"name\": \"ls\", \"args\": {\"path\": \"/tmp\"}}\n</tool_call>\n\nUser: <tool_result name=\"ls\" success=\"true\">\n[\"file1.txt\", \"file2.txt\"]\n</tool_result>\n\nAssistant: Here are the files in /tmp: file1.txt, file2.txt.\n\nUser: read file.txt\n\nAssistant: <tool_call>\n{\"name\": \"read\", \"args\": {\"path\": \"file.txt\"}}\n</tool_call>\n\nUser: <tool_result name=\"read\" success=\"true\">\n\"file contents here\"\n</tool_result>\n\nAssistant: The file contains: file contents here."
  }

  private parseToolCalls(text: string): {
    text: string
    toolCalls: ToolCall[]
    beforeFirst: string
  } {
    const toolCalls: ToolCall[] = []
    const segments: string[] = []
    let lastIndex = 0
    let match: RegExpExecArray | null

    const re = new RegExp(TOOL_CALL_RE.source, "g")
    while ((match = re.exec(text)) !== null) {
      segments.push(text.slice(lastIndex, match.index))
      lastIndex = re.lastIndex
      try {
        const parsed = JSON.parse(match[1])
        toolCalls.push({ name: parsed.name, args: parsed.args ?? {} })
      } catch {
        segments.push(match[0])
      }
    }
    segments.push(text.slice(lastIndex))

    const beforeFirst = segments[0] ?? ""
    const cleaned = segments.join("").trim()

    return { text: cleaned, toolCalls, beforeFirst }
  }

  private async executeTool(call: ToolCall): Promise<ToolResult> {
    const handler = toolHandlers[call.name]
    if (!handler) {
      return { name: call.name, success: false, data: null, error: `Unknown tool: ${call.name}` }
    }
    try {
      const data = await handler(call.args)
      return { name: call.name, success: true, data, error: undefined }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      return { name: call.name, success: false, data: null, error: msg }
    }
  }

  private formatToolResult(result: ToolResult): string {
    const body = JSON.stringify(result.data ?? null)
    const label = `<tool_result name="${result.name}" success="${result.success}">`
    if (result.error) {
      return `${label}\nerror: ${result.error}\n</tool_result>`
    }
    const truncated = body.length > 2000 ? body.slice(0, 2000) + "..." : body
    return `${label}\n${truncated}\n</tool_result>`
  }

  private cleanOutput(text: string): string {
    return text
      .replace(/^Assistant:\s*/i, "")
      .replace(/<think>[\s\S]*?<\/think>\n*/g, "")
      .trim()
  }

  async dispose() {
    await this.session.save()
  }
}
