'use client'

import { useState, useEffect } from 'react'
import type { TraceEntry } from '@/lib/types'

export default function TracesPage() {
  const [traces, setTraces] = useState<TraceEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetchTraces()
  }, [])

  async function fetchTraces() {
    setLoading(true)
    setError(null)
    try {
      const res = await fetch('/api/traces')
      if (!res.ok) throw new Error(await res.text())
      const data: TraceEntry[] = await res.json()
      setTraces(data)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="flex flex-col h-screen">
      <header className="px-6 py-3 border-b border-slate-800 bg-slate-900 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <a href="/" className="text-xl font-bold text-blue-400 hover:text-blue-300">RoCo AI</a>
          <span className="text-xs text-slate-500 bg-slate-800 px-2 py-0.5 rounded">
            Saved Traces
          </span>
        </div>
        <nav className="flex items-center gap-4 text-sm">
          <a href="/" className="text-slate-400 hover:text-slate-200">Run</a>
          <a href="/traces" className="text-blue-400 hover:text-blue-300 font-medium">Traces</a>
          <button onClick={fetchTraces} className="text-xs text-slate-500 hover:text-slate-300 border border-slate-700 rounded px-2 py-1">
            Refresh
          </button>
        </nav>
      </header>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-4xl mx-auto">
          <h1 className="text-lg font-semibold text-slate-200 mb-4">Saved Traces</h1>

          {loading && (
            <div className="flex items-center justify-center py-12">
              <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-blue-400 mr-3" />
              <span className="text-slate-400 text-sm">Loading...</span>
            </div>
          )}

          {error && (
            <div className="bg-red-900/50 border border-red-700 rounded p-4 text-sm text-red-300 mb-4">
              {error}
            </div>
          )}

          {!loading && !error && traces.length === 0 && (
            <div className="text-center py-12">
              <div className="text-3xl mb-3">📭</div>
              <p className="text-slate-500 text-sm mb-2">No traces saved yet.</p>
              <a href="/" className="text-blue-400 hover:underline text-sm">Run a task to create one</a>
            </div>
          )}

          {traces.length > 0 && (
            <div className="space-y-2">
              {traces.map((t) => (
                <a
                  key={t.id}
                  href={`/traces/${encodeURIComponent(t.id)}`}
                  className="block bg-slate-900 border border-slate-800 rounded-lg p-4 hover:border-slate-600 transition-colors"
                >
                  <div className="flex items-start justify-between">
                    <div className="flex-1 min-w-0">
                      <div className="text-sm text-slate-200 font-medium truncate">
                        {t.objective || '(no objective)'}
                      </div>
                      <div className="text-xs text-slate-500 font-mono mt-1">
                        {t.id}
                      </div>
                    </div>
                    <div className="flex gap-3 text-xs text-slate-400 ml-4 shrink-0">
                      <span>{t.events} events</span>
                      <span>{t.subtasks} subtasks</span>
                      <span className={t.failed > 0 ? 'text-red-400' : 'text-green-400'}>
                        {t.failed} failed
                      </span>
                    </div>
                  </div>
                </a>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
