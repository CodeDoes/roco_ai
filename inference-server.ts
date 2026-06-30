#!/usr/bin/env node
import * as path from "path"
import { fileURLToPath } from "url"
import { InferenceServer } from "./src/inference/server.ts"
import { promises as fsp } from "fs"

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const PROJECT_ROOT = path.resolve(__dirname)

const args = process.argv.slice(2)
const port = parseInt(args.find((a) => a.startsWith("--port="))?.split("=")[1] || "3210", 10)
const slotsDir = args.find((a) => a.startsWith("--slots-dir="))?.split("=")[1] || path.join(PROJECT_ROOT, "inference-slots", String(port))

async function main() {
  await fsp.mkdir(slotsDir, { recursive: true })
  const server = new InferenceServer(slotsDir, port)
  process.on("SIGINT", async () => {
    console.error("\nShutting down inference API...")
    await server.stop()
    process.exit(0)
  })
  process.on("SIGTERM", async () => {
    await server.stop()
    process.exit(0)
  })
  await server.start(port)
}

main().catch((err) => {
  console.error(`Fatal: ${err.message}`)
  process.exit(1)
})