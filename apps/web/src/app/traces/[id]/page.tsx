'use client'

import { useState, useEffect } from 'react'
import { useParams } from 'next/navigation'
import type { TraceData } from '@/lib/types'
import { Visualizer } from '@/components/Visualizer'

export default function TraceDetailPage() {
  const params = useParams()
  const id = params.id as string
  const [trace, setTrace] = useState<TraceData | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!id) return
    setLoading(true)
    setError(null)

    fetch(`/api/traces/${encodeURIComponent(id)}`)
      .then(async (res) => {
        if (!res.ok) throw new Error(await res.text())
        return res.json() as Promise<TraceData>
      })
      .then(setTrace)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false))
  }, [id])

  return (
    <div className="flex flex-col h-screen">
      <header className="px-6 py-3 border-b border-slate-800 bg-slate-900 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <a href="/" className="text-xl font-bold text-blue-400 hover:text-blue-300">RoCo AI</a>
          <span className="text-xs text-slate-500 bg-slate-800 px-2 py-0.5 rounded">
            Trace
          </span>
        </div>
        <nav className="flex items-center gap-4 text-sm">
          <a href="/" className="text-slate-400 hover:text-slate-200">Run</a>
          <a href="/traces" className="text-blue-400 hover:text-blue-300 font-medium">Traces</a>
          {id && <span className="text-xs text-slate-500 font-mono">{id}</span>}
        </nav>
      </header>

      <main className="flex-1 overflow-y-auto bg-slate-950">
        {loading && (
          <div className="flex items-center justify-center h-full">
            <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-blue-400 mr-3" />
            <span className="text-slate-400 text-sm">Loading trace...</span>
          </div>
        )}

        {error && (
          <div className="flex items-center justify-center h-full">
            <div className="text-center max-w-md">
              <div className="text-3xl mb-3">⚠️</div>
              <p className="text-red-400 text-sm mb-2">{error}</p>
              <a href="/traces" className="text-blue-400 hover:underline text-sm">Back to traces</a>
            </div>
          </div>
        )}

        {!loading && !error && trace && <Visualizer trace={trace} />}
      </main>
    </div>
  )
}
