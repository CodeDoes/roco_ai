/**
 * RoCo Web App — oRPC router (Phase 2 → Phase 3).
 *
 * Procedures can run via one of two backends:
 *   1. **Gateway** (recommended) — axum HTTP server at GATEWAY_URL
 *      (default http://localhost:3001). Faster, no cargo rebuild per call.
 *   2. **CLI exec** (fallback) — `cargo run -p roco-cli -- run-input <file>`
 *      or `roco run-input <file>` in production.
 *   3. **Direct napi** (future) — when roco_napi .node addon is compiled.
 *
 * Usage from a Next.js API route:
 *   import { handler } from '@/lib/orpc'
 *   export const POST = (req: Request) => handler.handle(req)
 */

import { z } from 'zod'
import { os } from '@orpc/server'
import { RPCHandler } from '@orpc/server/fetch'
import { execSync } from 'child_process'
import { writeFileSync, unlinkSync, mkdirSync } from 'fs'
import { join } from 'path'

// ---------------------------------------------------------------------------
// Backend resolution
// ---------------------------------------------------------------------------

/** Gateway base URL (set GATEWAY_URL env var to override) */
const GATEWAY_URL = process.env.GATEWAY_URL || 'http://localhost:3001'

/** Whether to prefer the gateway over CLI exec */
const PREFER_GATEWAY = process.env.PREFER_GATEWAY !== 'false'

/** Generate a default demo context for testing */
function defaultContext(): string {
  return Array.from({ length: 40 }, (_, i) =>
    `Fact ${i}: the orchestrator routes subtask ${i} through a verification gate. `
  ).join('')
}

// ---------------------------------------------------------------------------
// CLI exec bridge
// ---------------------------------------------------------------------------

function cargoRoot(): string {
  return join(process.cwd(), '..', '..')
}

function tmpDir(): string {
  const d = join(process.cwd(), '.roco', 'orpc-tmp')
  mkdirSync(d, { recursive: true })
  return d
}

function runRocoCli(args: string[], input?: unknown): string {
  const inputFile = input ? join(tmpDir(), `input-${Date.now()}.json`) : null

  try {
    if (inputFile && input) {
      writeFileSync(inputFile, JSON.stringify(input))
    }

    const argsStr = args.join(' ')
    const inputStr = inputFile ? ` "${inputFile}"` : ''

    const isDev = process.env.NODE_ENV === 'development'
    const cmd = isDev
      ? `cargo run -p roco-cli -- ${argsStr}${inputStr} 2>/dev/null`
      : `roco ${argsStr}${inputStr} 2>/dev/null`

    return execSync(cmd, {
      cwd: isDev ? cargoRoot() : undefined,
      timeout: 60_000,
      encoding: 'utf-8',
      maxBuffer: 10 * 1024 * 1024,
    })
  } finally {
    if (inputFile) {
      try { unlinkSync(inputFile) } catch { /* ignore */ }
    }
  }
}

// ---------------------------------------------------------------------------
// Gateway proxy
// ---------------------------------------------------------------------------

async function gatewayFetch(path: string, body?: unknown): Promise<Response> {
  const url = `${GATEWAY_URL}${path}`
  const res = await fetch(url, {
    method: body ? 'POST' : 'GET',
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(60_000),
  })
  return res
}

async function gatewayAvailable(): Promise<boolean> {
  if (!PREFER_GATEWAY) return false
  try {
    const res = await fetch(`${GATEWAY_URL}/health`, {
      signal: AbortSignal.timeout(2_000),
    })
    return res.ok
  } catch {
    return false
  }
}

// ---------------------------------------------------------------------------
// oRPC procedures
// ---------------------------------------------------------------------------

export const router = os.router({
  runTask: os
    .input(
      z.object({
        objective: z.string(),
        context: z.string().optional().default(''),
        outputSchema: z.string().optional().default(''),
        allowAbstain: z.boolean().optional().default(true),
      })
    )
    .handler(async ({ input }) => {
      // Try gateway first, fall back to CLI
      if (await gatewayAvailable()) {
        const res = await gatewayFetch('/rpc', {
          objective: input.objective,
          context: input.context || defaultContext(),
          output_schema: input.outputSchema || '{"result": "<string>"}',
          allow_abstain: input.allowAbstain,
        })
        if (res.ok) return res.json()
        console.warn(`gateway /rpc failed (${res.status}), falling back to CLI`)
      }

      // CLI fallback
      const stdout = runRocoCli(['run-input'], {
        objective: input.objective,
        context: input.context || defaultContext(),
        output_schema: input.outputSchema || '{"result": "<string>"}',
        allow_abstain: input.allowAbstain,
      })
      return JSON.parse(stdout)
    }),

  listTraces: os
    .input(z.void())
    .handler(async () => {
      if (await gatewayAvailable()) {
        const res = await gatewayFetch('/traces')
        if (res.ok) return res.json()
      }

      // Fallback: read traces directory directly
      const { readdirSync, readFileSync, existsSync } = await import('fs')
      const { join } = await import('path')
      const tracesDir = join(process.cwd(), '.roco', 'traces')

      if (!existsSync(tracesDir)) return []

      const entries: Array<{
        id: string
        objective: string
        events: number
        subtasks: number
        failed: number
      }> = []
      for (const file of readdirSync(tracesDir)) {
        if (!file.endsWith('.json')) continue
        try {
          const content = JSON.parse(readFileSync(join(tracesDir, file), 'utf-8'))
          entries.push({
            id: content.id ?? file.replace('.json', ''),
            objective: content.objective ?? '',
            events: content.events?.length ?? 0,
            subtasks: content.summary?.subtask_count ?? 0,
            failed: content.summary?.failed_subtasks ?? 0,
          })
        } catch {
          /* skip corrupt files */
        }
      }
      entries.sort((a, b) => b.id.localeCompare(a.id))
      return entries
    }),

  loadTrace: os
    .input(z.object({ id: z.string() }))
    .handler(async ({ input }) => {
      if (await gatewayAvailable()) {
        const res = await gatewayFetch(`/trace/${encodeURIComponent(input.id)}`)
        if (res.ok) return res.json()
      }

      // Fallback: read trace file directly
      const { readFileSync, existsSync } = await import('fs')
      const { join } = await import('path')
      const traceFile = join(process.cwd(), '.roco', 'traces', `${input.id}.json`)

      if (!existsSync(traceFile)) {
        throw new Error(`trace '${input.id}' not found`)
      }
      return JSON.parse(readFileSync(traceFile, 'utf-8'))
    }),

  diffTraces: os
    .input(z.object({ id1: z.string(), id2: z.string() }))
    .handler(async ({ input }) => {
      if (await gatewayAvailable()) {
        const t1Res = await gatewayFetch(`/trace/${encodeURIComponent(input.id1)}`)
        const t2Res = await gatewayFetch(`/trace/${encodeURIComponent(input.id2)}`)
        if (t1Res.ok && t2Res.ok) {
          const [t1, t2] = await Promise.all([t1Res.json(), t2Res.json()])
          return {
            id1: input.id1,
            id2: input.id2,
            events_added: Math.max(0, t2.events.length - t1.events.length),
            events_removed: Math.max(0, t1.events.length - t2.events.length),
            subtask_delta: (t2.summary?.subtask_count ?? 0) - (t1.summary?.subtask_count ?? 0),
            failed_delta: (t2.summary?.failed_subtasks ?? 0) - (t1.summary?.failed_subtasks ?? 0),
            retries_delta: (t2.summary?.retries ?? 0) - (t1.summary?.retries ?? 0),
          }
        }
      }

      // CLI fallback
      const stdout = runRocoCli(['trace', 'diff', input.id1, input.id2])
      const lines = stdout.split('\n').filter(Boolean)
      return {
        id1: input.id1,
        id2: input.id2,
        events_added: 0,
        events_removed: 0,
        subtask_delta: 0,
        failed_delta: 0,
        retries_delta: 0,
        _raw: stdout,
        _summary: lines,
      }
    }),
})

export type Router = typeof router

// Create the RPCHandler - wraps the router for fetch-based adapters
export const handler = new RPCHandler(router)
