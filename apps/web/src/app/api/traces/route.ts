/**
 * API Route: GET /api/traces
 *
 * Lists all saved traces from the Rust TraceStore on disk.
 */
import { NextResponse } from 'next/server'
import { readdirSync, readFileSync, existsSync } from 'fs'
import { join } from 'path'

export const runtime = 'nodejs'

export async function GET() {
  try {
    const tracesDir = join(process.cwd(), '.roco', 'traces')

    if (!existsSync(tracesDir)) {
      return NextResponse.json([])
    }

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
    return NextResponse.json(entries)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    console.error('list-traces error:', msg)
    return NextResponse.json({ error: msg }, { status: 500 })
  }
}
