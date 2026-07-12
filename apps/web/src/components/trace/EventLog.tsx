"use client";

import type { TraceEventT } from "@/lib/schemas";

const PHASE_COLOR: Record<string, string> = {
  decompose: "text-purple-400 border-purple-600",
  execute: "text-blue-400 border-blue-600",
  budget_check: "text-cyan-400 border-cyan-600",
  model_call: "text-yellow-400 border-yellow-600",
  tool_parse: "text-green-400 border-green-600",
  tool_exec: "text-emerald-400 border-emerald-600",
  tool_result: "text-teal-400 border-teal-600",
  verify: "text-orange-400 border-orange-600",
  retry: "text-red-400 border-red-600",
  aggregate: "text-pink-400 border-pink-600",
  done: "text-slate-400 border-slate-600",
};

const PHASE_RING: Record<string, string> = {
  ...Object.fromEntries(
    Object.entries(PHASE_COLOR).map(([k, v]) => [k, v.split(" ")[0]]),
  ),
};

export function EventLog({ events }: { events: TraceEventT[] }) {
  return (
    <div className="border border-slate-800 rounded-lg overflow-hidden">
      <div className="bg-slate-900 px-4 py-2 border-b border-slate-800 text-xs uppercase tracking-wider text-slate-400 flex items-center justify-between">
        <span>Events</span>
        <span className="text-slate-500 font-mono">{events.length} total</span>
      </div>
      <div className="max-h-[calc(100vh-16rem)] overflow-y-auto">
        {events.map((e, i) => {
          const color = PHASE_COLOR[e.phase] ?? "text-slate-400 border-slate-600";
          const ring = PHASE_RING[e.phase] ?? "text-slate-400";
          return (
            <div
              key={i}
              className={`flex items-start gap-3 px-4 py-2 border-l-4 ${color} bg-slate-950/40 border-b border-slate-900 hover:bg-slate-900/60`}
            >
              <span className={`font-mono text-xs ${ring} shrink-0 w-20 truncate`}>
                {e.phase}
              </span>
              <span className="text-xs text-slate-400 shrink-0 w-32 font-mono truncate">
                {e.actor}
              </span>
              <span className="text-xs text-slate-200 flex-1 break-words">
                {e.detail}
              </span>
              <span className="text-[10px] text-slate-600 shrink-0 font-mono">
                +{e.ts_ms}ms
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
