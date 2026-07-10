'use client'

import { useState, useCallback } from 'react'
import type { TraceData } from '@/lib/types'
import { Visualizer } from '@/components/Visualizer'

export default function Home() {
  const [objective, setObjective] = useState('Review the provided facts and summarize.')
  const [context, setContext] = useState('')
  const [trace, setTrace] = useState<TraceData | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [traceId, setTraceId] = useState<string | null>(null)

  const runTask = useCallback(async () => {
    setLoading(true)
    setError(null)
    setTrace(null)
    setTraceId(null)

    try {
      const res = await fetch('/api/run-task', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          objective,
          context: context || generateDefaultContext(),
          outputSchema: '',
          allowAbstain: true,
        }),
      })

      if (!res.ok) {
        const err = await res.text()
        throw new Error(err || `HTTP ${res.status}`)
      }

      const data: TraceData = await res.json()
      setTrace(data)
      setTraceId(data.id ?? null)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [objective, context])

  const generateDemoContext = useCallback(() => {
    const ctx = Array.from({ length: 80 }, (_, i) =>
      `Fact ${i}: the orchestrator routes subtask ${i} through a verification gate. `
    ).join('')
    setContext(ctx)
  }, [])

  return (
    <div className="flex flex-col h-screen">
      {/* Header */}
      <header className="px-6 py-3 border-b border-slate-800 bg-slate-900 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <a href="/" className="text-xl font-bold text-blue-400 hover:text-blue-300">RoCo AI</a>
          <span className="text-xs text-slate-500 bg-slate-800 px-2 py-0.5 rounded">
            Trace Visualizer
          </span>
        </div>
        <nav className="flex items-center gap-4 text-sm">
          <a href="/" className="text-blue-400 hover:text-blue-300 font-medium">Run</a>
          <a href="/traces" className="text-slate-400 hover:text-slate-200">Traces</a>
          {traceId && (
            <span className="text-xs text-slate-500 font-mono ml-2">
              {traceId}
            </span>
          )}
        </nav>
      </header>

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar — Task Input */}
        <aside className="w-80 border-r border-slate-800 bg-slate-900 p-4 flex flex-col gap-4 overflow-y-auto shrink-0">
          <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-400">
            Run Task
          </h2>

          <div className="flex flex-col gap-2">
            <label className="text-xs text-slate-400">Objective</label>
            <textarea
              className="bg-slate-800 border border-slate-700 rounded px-3 py-2 text-sm text-slate-100 resize-none h-20 focus:outline-none focus:border-blue-500"
              value={objective}
              onChange={(e) => setObjective(e.target.value)}
              placeholder="Task objective..."
            />
          </div>

          <div className="flex flex-col gap-2">
            <label className="text-xs text-slate-400">Context</label>
            <textarea
              className="bg-slate-800 border border-slate-700 rounded px-3 py-2 text-sm text-slate-100 resize-none flex-1 min-h-[120px] font-mono text-xs focus:outline-none focus:border-blue-500"
              value={context}
              onChange={(e) => setContext(e.target.value)}
              placeholder="Task context (large text is chunked automatically)..."
            />
          </div>

          <div className="flex gap-2">
            <button
              onClick={generateDemoContext}
              className="text-xs text-slate-400 hover:text-slate-200 border border-slate-700 rounded px-2 py-1"
            >
              Demo Context
            </button>
          </div>

          <button
            onClick={runTask}
            disabled={loading || !objective.trim()}
            className="bg-blue-600 hover:bg-blue-500 disabled:bg-slate-700 disabled:text-slate-500 text-white rounded px-4 py-2 text-sm font-medium transition-colors mt-2"
          >
            {loading ? 'Running...' : 'Run Task'}
          </button>

          {error && (
            <div className="bg-red-900/50 border border-red-700 rounded p-3 text-xs text-red-300">
              {error}
            </div>
          )}

          {!loading && !trace && (
            <div className="text-xs text-slate-500 mt-4 space-y-2">
              <p>Enter a task objective and context, then click <strong>Run Task</strong>.</p>
              <p>The orchestrator decomposes the task into 4K-budget subtasks, executes them via workers, verifies outputs, and returns a structured trace.</p>
              <p className="pt-2">Or browse <a href="/traces" className="text-blue-400 hover:underline">saved traces</a>.</p>
            </div>
          )}
        </aside>

        {/* Main — Visualizer */}
        <main className="flex-1 overflow-y-auto bg-slate-950">
          {loading ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-center">
                <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-400 mx-auto mb-4" />
                <p className="text-slate-400 text-sm">Running orchestration...</p>
              </div>
            </div>
          ) : trace ? (
            <Visualizer trace={trace} />
          ) : (
            <div className="flex items-center justify-center h-full">
              <div className="text-center max-w-md">
                <div className="text-4xl mb-4">🔍</div>
                <h2 className="text-lg font-semibold text-slate-300 mb-2">No Trace Yet</h2>
                <p className="text-slate-500 text-sm">
                  Enter a task in the sidebar and click <strong>Run Task</strong> to see the execution trace.
                </p>
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  )
}

function generateDefaultContext(): string {
  return Array.from({ length: 40 }, (_, i) =>
    `Fact ${i}: the orchestrator routes subtask ${i} through a verification gate. `
  ).join('')
}
