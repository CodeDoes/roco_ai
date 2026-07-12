'use client'

import { useState, useCallback } from 'react'
import type { TraceData, TraceDiff } from '@/lib/types'

export default function DiffPage() {
  const [id1, setId1] = useState('')
  const [id2, setId2] = useState('')
  const [diff, setDiff] = useState<TraceDiff | null>(null)
  const [t1, setT1] = useState<TraceData | null>(null)
  const [t2, setT2] = useState<TraceData | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadTrace = useCallback(async (id: string): Promise<TraceData> => {
    const res = await fetch(`/api/traces/${encodeURIComponent(id)}`)
    if (!res.ok) throw new Error(await res.text())
    return res.json()
  }, [])

  const compare = useCallback(async () => {
    if (!id1.trim() || !id2.trim()) return
    setLoading(true)
    setError(null)
    setDiff(null)
    setT1(null)
    setT2(null)

    try {
      const [trace1, trace2] = await Promise.all([
        loadTrace(id1.trim()),
        loadTrace(id2.trim()),
      ])
      setT1(trace1)
      setT2(trace2)

      const d: TraceDiff = {
        id1: id1.trim(),
        id2: id2.trim(),
        events_added: Math.max(0, trace2.events.length - trace1.events.length),
        events_removed: Math.max(0, trace1.events.length - trace2.events.length),
        subtask_delta: (trace2.summary?.subtask_count ?? 0) - (trace1.summary?.subtask_count ?? 0),
        failed_delta: (trace2.summary?.failed_subtasks ?? 0) - (trace1.summary?.failed_subtasks ?? 0),
        retries_delta: (trace2.summary?.retries ?? 0) - (trace1.summary?.retries ?? 0),
      }
      setDiff(d)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [id1, id2, loadTrace])

  const Delta = ({ value, label }: { value: number; label: string }) => (
    <div className="bg-slate-900 border border-slate-800 rounded-lg p-3">
      <div className="text-xs text-slate-500 mb-0.5">{label}</div>
      <div className={`text-lg font-semibold ${value === 0 ? 'text-slate-400' : value > 0 ? 'text-orange-400' : 'text-green-400'}`}>
        {value > 0 ? '+' : ''}{value}
      </div>
    </div>
  )

  return (
    <div className="flex flex-col h-screen">
      <header className="px-6 py-3 border-b border-slate-800 bg-slate-900 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <a href="/" className="text-xl font-bold text-blue-400 hover:text-blue-300">RoCo AI</a>
          <span className="text-xs text-slate-500 bg-slate-800 px-2 py-0.5 rounded">
            Trace Diff
          </span>
        </div>
        <nav className="flex items-center gap-4 text-sm">
          <a href="/" className="text-slate-400 hover:text-slate-200">Run</a>
          <a href="/traces" className="text-slate-400 hover:text-slate-200">Traces</a>
          <a href="/diff" className="text-blue-400 hover:text-blue-300 font-medium">Diff</a>
        </nav>
      </header>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-2xl mx-auto">
          <h1 className="text-lg font-semibold text-slate-200 mb-4">Compare Traces</h1>

          <div className="flex gap-3 mb-4">
            <div className="flex-1">
              <label className="text-xs text-slate-400 mb-1 block">Trace ID 1 (older)</label>
              <input
                className="w-full bg-slate-800 border border-slate-700 rounded px-3 py-2 text-sm text-slate-100 font-mono focus:outline-none focus:border-blue-500"
                value={id1}
                onChange={(e) => setId1(e.target.value)}
                placeholder="viz-12345..."
              />
            </div>
            <div className="flex-1">
              <label className="text-xs text-slate-400 mb-1 block">Trace ID 2 (newer)</label>
              <input
                className="w-full bg-slate-800 border border-slate-700 rounded px-3 py-2 text-sm text-slate-100 font-mono focus:outline-none focus:border-blue-500"
                value={id2}
                onChange={(e) => setId2(e.target.value)}
                placeholder="viz-67890..."
              />
            </div>
          </div>

          <button
            onClick={compare}
            disabled={loading || !id1.trim() || !id2.trim()}
            className="bg-blue-600 hover:bg-blue-500 disabled:bg-slate-700 disabled:text-slate-500 text-white rounded px-4 py-2 text-sm font-medium transition-colors mb-6"
          >
            {loading ? 'Loading...' : 'Compare'}
          </button>

          {error && (
            <div className="bg-red-900/50 border border-red-700 rounded p-3 text-sm text-red-300 mb-4">
              {error}
            </div>
          )}

          {diff && (
            <>
              <div className="grid grid-cols-2 gap-3 mb-6">
                <Delta value={diff.events_added} label="Events Added" />
                <Delta value={diff.events_removed} label="Events Removed" />
                <Delta value={diff.subtask_delta} label="Subtask Δ" />
                <Delta value={diff.failed_delta} label="Failed Δ" />
                <Delta value={diff.retries_delta} label="Retries Δ" />
              </div>

              <div className="grid grid-cols-2 gap-4">
                {t1 && (
                  <div>
                    <h3 className="text-sm font-semibold text-slate-300 mb-2">
                      {t1.id} <span className="text-xs text-slate-500">({t1.events.length} events)</span>
                    </h3>
                    <pre className="text-xs text-slate-400 bg-slate-900 border border-slate-800 rounded p-3 max-h-96 overflow-y-auto">
                      {JSON.stringify(t1.summary, null, 2)}
                    </pre>
                  </div>
                )}
                {t2 && (
                  <div>
                    <h3 className="text-sm font-semibold text-slate-300 mb-2">
                      {t2.id} <span className="text-xs text-slate-500">({t2.events.length} events)</span>
                    </h3>
                    <pre className="text-xs text-slate-400 bg-slate-900 border border-slate-800 rounded p-3 max-h-96 overflow-y-auto">
                      {JSON.stringify(t2.summary, null, 2)}
                    </pre>
                  </div>
                )}
              </div>
            </>
          )}

          {!loading && !diff && !error && (
            <p className="text-sm text-slate-500">
              Enter two trace IDs to compare them. You can find trace IDs on the{` `}
              <a href="/traces" className="text-blue-400 hover:underline">traces page</a>.
            </p>
          )}
        </div>
      </div>
    </div>
  )
}
