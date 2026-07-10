/**
 * API Route: POST /api/run-task
 *
 * Bridges to the RoCo Rust CLI via `roco run-input <tmpfile>`.
 * Once the napi-rs addon is compiled, swap to direct import.
 */
import { NextRequest, NextResponse } from 'next/server'
import { execSync } from 'child_process'
import { writeFileSync, unlinkSync, mkdirSync } from 'fs'
import { join } from 'path'

export const runtime = 'nodejs'

interface RunTaskBody {
  objective: string
  context?: string
  outputSchema?: string
  allowAbstain?: boolean
}

export async function POST(req: NextRequest) {
  try {
    const body: RunTaskBody = await req.json()

    if (!body.objective?.trim()) {
      return NextResponse.json(
        { error: 'objective is required' },
        { status: 400 }
      )
    }

    // Write input to a temp JSON file
    const tmpDir = join(process.cwd(), '.roco', 'api-tmp')
    mkdirSync(tmpDir, { recursive: true })

    const input = {
      objective: body.objective,
      context: body.context ?? '',
      output_schema: body.outputSchema ?? '',
      allow_abstain: body.allowAbstain ?? true,
    }

    const tmpFile = join(tmpDir, `input-${Date.now()}.json`)
    writeFileSync(tmpFile, JSON.stringify(input))

    // Determine the roco binary path
    // In development, use `cargo run -- run-input`. In production, expect
    // the binary on PATH or at a known location.
    const isDev = process.env.NODE_ENV === 'development'
    let stdout: string

    try {
      if (isDev) {
        // Use cargo run (slower but always up-to-date)
        stdout = execSync(
          `cargo run -p roco-cli -- run-input "${tmpFile}" 2>/dev/null`,
          {
            cwd: join(process.cwd(), '..', '..'), // roco_ai root
            timeout: 60_000,
            encoding: 'utf-8',
            maxBuffer: 10 * 1024 * 1024,
          }
        )
      } else {
        // Use pre-built binary
        stdout = execSync(`roco run-input "${tmpFile}" 2>/dev/null`, {
          timeout: 60_000,
          encoding: 'utf-8',
          maxBuffer: 10 * 1024 * 1024,
        })
      }
    } finally {
      // Clean up temp file
      try { unlinkSync(tmpFile) } catch { /* ignore */ }
    }

    // Parse the trace JSON from stdout
    const trace = JSON.parse(stdout)
    return NextResponse.json(trace)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    console.error('run-task error:', msg)
    return NextResponse.json({ error: msg }, { status: 500 })
  }
}
