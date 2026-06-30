import * as fs from "fs"
import * as path from "path"
import { fileURLToPath } from "url"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const TRACES_DIR = path.resolve(__dirname, "..", "eval", ".traces")

export class TraceWriter {
  private filePath: string
  private fd: number | null = null

  constructor(mode: string) {
    fs.mkdirSync(TRACES_DIR, { recursive: true })
    const ts = new Date().toISOString().replace(/[:.]/g, "-")
    this.filePath = path.join(TRACES_DIR, `${mode}-${ts}.txt`)
  }

  open() {
    this.fd = fs.openSync(this.filePath, "a")
    this.emit(`=== ${path.basename(this.filePath, ".txt")} ===`)
    this.emit(`start: ${new Date().toISOString()}`)
    this.emit("")
    return this
  }

  private emit(line: string) {
    if (this.fd !== null) {
      fs.writeSync(this.fd, line + "\n")
    }
  }

  write(line: string) {
    this.emit(line)
  }

  section(label: string) {
    this.emit("")
    this.emit(`--- ${label} ---`)
  }

  depth(n: number, tag = "") {
    const suffix = tag ? ` (${tag})` : ""
    this.emit("")
    this.emit(`-- depth ${n}${suffix} --`)
  }

  stream(text: string) {
    if (this.fd !== null) {
      fs.writeSync(this.fd, text)
    }
  }

  generate(text: string, truncated = true) {
    const content = truncated && text.length > 2000
      ? text.slice(0, 2000) + `\n... [truncated, total ${text.length} chars]`
      : text
    this.emit(`[generate]\n${content}`)
  }

  toolCall(name: string, args: Record<string, unknown>) {
    const argsStr = JSON.stringify(args, null, 2)
    this.emit(`[tool_call] ${name} ${argsStr}`)
  }

  toolResult(name: string, success: boolean, data: unknown, error?: string) {
    const dataStr = JSON.stringify(data ?? null).slice(0, 500)
    if (error) {
      this.emit(`[tool_result] ${name} success=${success} error=${error.slice(0, 300)}`)
    } else {
      this.emit(`[tool_result] ${name} success=${success} data=${dataStr}`)
    }
  }

  userInput(text: string) {
    this.emit(`[user] ${text}`)
  }

  verification(checks: { name: string; pass: boolean }[]) {
    this.emit("")
    this.emit(`── Verification ──`)
    for (const c of checks) {
      this.emit(`  [${c.pass ? "PASS" : "FAIL"}] ${c.name}`)
    }
    const passed = checks.filter((c) => c.pass).length
    const total = checks.length
    const status = passed === total ? "PASS" : "FAIL"
    this.emit(`${passed}/${total} ${status}`)
  }

  close() {
    if (this.fd !== null) {
      this.emit("")
      this.emit(`end: ${new Date().toISOString()}`)
      fs.closeSync(this.fd)
      this.fd = null
    }
  }

  get path(): string {
    return this.filePath
  }
}
