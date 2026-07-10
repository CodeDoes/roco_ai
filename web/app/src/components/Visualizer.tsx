'use client'

import { useMemo, useState, useRef, useEffect } from 'react'
import type { TraceData, TraceEventData } from '@/lib/types'

interface VisualizerProps {
  trace: TraceData
}

type TabId = 'events' | 'messages' | 'summary'

export function Visualizer({ trace }: VisualizerProps) {
  const [activeTab, setActiveTab] = useState<TabId>('events')
  const [expandedEvent, setExpandedEvent] = useState<number | null>(null)

  const { events, messages, summary, objective } = trace
  const totalEvents = events.length

  // Phase color mapping
  const phaseColor = (phase: string): string => {
    const colors: Record<string, string> = {
      decompose: 'text-purple-400 border-purple-600',
      execute: 'text-blue-400 border-blue-600',
      budget_check: 'text-cyan-400 border-cyan-600',
      model_call: 'text-yellow-400 border-yellow-600',
      tool_parse: 'text-green-400 border-green-600',
      tool_exec: 'text-emerald-400 border-emerald-600',
      tool_result: 'text-teal-400 border-teal-600',
      verify: 'text-orange-400 border-orange-600',
      retry: 'text-red-400 border-red-600',
      aggregate: 'text-pink-400 border-pink-600',
      done: 'text-slate-400 border-slate-600',
    }
    return colors[phase] ?? 'text-slate-400 border-slate-600'
  }

  // Stats for the summary tab
  const stats = useMemo(() => {
    const phaseCounts: Record<string, number> = {}
    for (const e of events) {
      phaseCounts[e.phase] = (phaseCounts[e.phase] ?? 0) + 1
    }
    return {
      total: events.length,
      byPhase: Object.entries(phaseCounts).sort((a, b) => b[1] - a[1]),
      duration: summary.duration_ms,
      subtasks: summary.subtask_count,
      failed: summary.failed_subtasks,
      modelCalls: summary.model_calls,
      toolCalls: summary.tool_calls,
      retries: summary.retries,
    }
  }, [events, summary])

  // Format timestamp
  const fmtTs = (ms: number): string => {
    const base = events[0]?.ts_ms ?? ms
    const diff = ms - base
    return `+${(diff / 1000).toFixed(2)}s`
  }

  return (
    <div className="p-6 max-w-5xl mx-auto space-y-6">
      {/* Objective Header */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
        <div className="text-xs text-slate-500 mb-1">Objective</div>
        <div className="text-sm text-slate-200 font-medium">{objective}</div>
      </div>

      {/* Summary Stats Bar */}
      <div className="grid grid-cols-4 gap-3">
        <StatCard label="Subtasks" value={stats.subtasks} />
        <StatCard label="Failed" value={stats.failed} color={stats.failed > 0 ? 'text-red-400' : undefined} />
        <StatCard label="Model Calls" value={stats.modelCalls} />
        <StatCard label="Duration" value={`${(stats.duration / 1000).toFixed(1)}s`} />
        <StatCard label="Events" value={stats.total} />
        <StatCard label="Tool Calls" value={stats.toolCalls} />
        <StatCard label="Retries" value={stats.retries} color={stats.retries > 0 ? 'text-orange-400' : undefined} />
        <StatCard label="Tool Errors" value={summary.tool_errors} color={summary.tool_errors > 0 ? 'text-red-400' : undefined} />
      </div>

      {/* Tabs */}
      <div className="flex gap-1 border-b border-slate-800">
        <TabButton active={activeTab === 'events'} onClick={() => setActiveTab('events')}>
          Events ({totalEvents})
        </TabButton>
        <TabButton active={activeTab === 'messages'} onClick={() => setActiveTab('messages')}>
          Messages
        </TabButton>
        <TabButton active={activeTab === 'summary'} onClick={() => setActiveTab('summary')}>
          Summary
        </TabButton>
      </div>

      {/* Tab Content */}
      <div className="min-h-[400px]">
        {activeTab === 'events' && (
          <div className="space-y-1">
            {events.length === 0 && (
              <p className="text-slate-500 text-sm py-8 text-center">No events recorded.</p>
            )}
            {events.map((ev, i) => (
              <EventRow
                key={i}
                event={ev}
                index={i}
                fmtTs={fmtTs}
                phaseColor={phaseColor}
                expanded={expandedEvent === i}
                onToggle={() => setExpandedEvent(expandedEvent === i ? null : i)}
              />
            ))}
          </div>
        )}

        {activeTab === 'messages' && (
          <div className="space-y-4">
            {(!messages || (Array.isArray(messages) && messages.length === 0)) && (
              <p className="text-slate-500 text-sm py-8 text-center">No messages.</p>
            )}
            {Array.isArray(messages) && messages.map((msg: any, i: number) => (
              <div
                key={i}
                className={`p-3 rounded-lg max-w-[80%] text-sm ${
                  msg.role === 'user'
                    ? 'bg-blue-900/30 border border-blue-800 ml-auto'
                    : 'bg-slate-800 border border-slate-700'
                }`}
              >
                <div className="text-xs text-slate-500 mb-1 uppercase">{msg.role}</div>
                <div className="text-slate-200 whitespace-pre-wrap">{msg.content}</div>
              </div>
            ))}
          </div>
        )}

        {activeTab === 'summary' && (
          <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
            <h3 className="text-sm font-semibold text-slate-300 mb-3">Execution Summary</h3>
            <table className="w-full text-sm">
              <tbody>
                {stats.byPhase.map(([phase, count]) => (
                  <tr key={phase} className="border-b border-slate-800 last:border-0">
                    <td className="py-2 text-slate-400 capitalize">{phase.replace(/_/g, ' ')}</td>
                    <td className="py-2 text-right text-slate-200">{count}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  )
}

// --- Sub-components ---

function StatCard({ label, value, color }: { label: string; value: string | number; color?: string }) {
  return (
    <div className="bg-slate-900 border border-slate-800 rounded-lg p-3">
      <div className="text-xs text-slate-500 mb-0.5">{label}</div>
      <div className={`text-lg font-semibold ${color ?? 'text-slate-200'}`}>{value}</div>
    </div>
  )
}

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={`px-4 py-2 text-sm font-medium transition-colors border-b-2 -mb-[1px] ${
        active
          ? 'border-blue-500 text-blue-400'
          : 'border-transparent text-slate-500 hover:text-slate-300'
      }`}
    >
      {children}
    </button>
  )
}

function EventRow({
  event,
  index,
  fmtTs,
  phaseColor,
  expanded,
  onToggle,
}: {
  event: TraceEventData
  index: number
  fmtTs: (ms: number) => string
  phaseColor: (phase: string) => string
  expanded: boolean
  onToggle: () => void
}) {
  return (
    <div
      className={`border-l-2 pl-3 py-2 cursor-pointer hover:bg-slate-900/50 transition-colors ${
        phaseColor(event.phase).split(' ')[1] // border color
      }`}
      onClick={onToggle}
    >
      <div className="flex items-start gap-3">
        <span className="text-xs text-slate-600 font-mono w-16 shrink-0 pt-0.5">
          {fmtTs(event.ts_ms)}
        </span>
        <span className={`text-xs font-semibold uppercase shrink-0 w-24 pt-0.5 ${phaseColor(event.phase).split(' ')[0]}`}>
          {event.phase}
        </span>
        <span className="text-xs text-slate-500 font-mono w-28 shrink-0 pt-0.5">
          {event.actor}
        </span>
        <span className="text-sm text-slate-300 flex-1">
          {event.detail}
        </span>
      </div>
      {expanded && event.meta && Object.keys(event.meta).length > 0 && (
        <div className="mt-2 ml-[11.5rem] bg-slate-900 border border-slate-800 rounded p-3">
          <pre className="text-xs text-slate-400 overflow-x-auto">
            {JSON.stringify(event.meta, null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}
