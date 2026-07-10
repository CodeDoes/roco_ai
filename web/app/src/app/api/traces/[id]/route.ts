/**
 * API Route: GET /api/traces/[id]
 *
 * Loads a single saved trace by ID from the Rust TraceStore.
 */
import { NextRequest, NextResponse } from 'next/server'
import { readFileSync, existsSync } from 'fs'
import { join } from 'path'

export const runtime = 'nodejs'

export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  try {
    const { id } = await params
    const traceFile = join(process.cwd(), '.roco', 'traces', `${id}.json`)

    if (!existsSync(traceFile)) {
      return NextResponse.json(
        { error: `trace '${id}' not found` },
        { status: 404 }
      )
    }

    const content = JSON.parse(readFileSync(traceFile, 'utf-8'))
    return NextResponse.json(content)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    console.error('load-trace error:', msg)
    return NextResponse.json({ error: msg }, { status: 500 })
  }
}
