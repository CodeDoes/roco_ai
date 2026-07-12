"use client";

import { useState } from "react";
import type { TraceT } from "@/lib/schemas";
import { StatCards } from "./StatCards";
import { EventLog } from "./EventLog";

type Tab = "events" | "summary";

export function TracePanel({ trace }: { trace: TraceT | null }) {
  if (!trace) {
    return (
      <div className="flex items-center justify-center h-full text-center p-8">
        <div>
          <div className="text-3xl mb-2">◻</div>
          <p className="text-slate-400 text-sm">
            Ask RoCo to run a task in the chat panel.
          </p>
        </div>
      </div>
    );
  }

  return <TraceBody trace={trace} />;
}

function TraceBody({ trace }: { trace: TraceT }) {
  const [tab, setTab] = useState<Tab>("events");
  return (
    <div className="flex flex-col h-full">
      <div className="px-4 py-3 border-b border-slate-800 bg-slate-900">
        <div className="text-xs uppercase tracking-wider text-slate-500 mb-1">
          Objective
        </div>
        <div className="text-sm text-slate-200">{trace.objective}</div>
      </div>

      <div className="px-4 pt-3">
        <StatCards summary={trace.summary} />
      </div>

      <div className="px-4 mt-3 flex gap-2 border-b border-slate-800">
        <TabButton active={tab === "events"} onClick={() => setTab("events")}>
          Events ({trace.events.length})
        </TabButton>
        <TabButton active={tab === "summary"} onClick={() => setTab("summary")}>
          Summary
        </TabButton>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {tab === "events" && <EventLog events={trace.events} />}
        {tab === "summary" && <SummaryView trace={trace} />}
      </div>
    </div>
  );
}

function TabButton({
  children,
  active,
  onClick,
}: {
  children: React.ReactNode;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={
        "px-3 py-1.5 text-sm -mb-px border-b-2 " +
        (active
          ? "border-blue-400 text-slate-100"
          : "border-transparent text-slate-400 hover:text-slate-200")
      }
    >
      {children}
    </button>
  );
}

function SummaryView({ trace }: { trace: TraceT }) {
  const phaseCounts: Record<string, number> = {};
  for (const e of trace.events) phaseCounts[e.phase] = (phaseCounts[e.phase] ?? 0) + 1;

  return (
    <div className="space-y-3">
      <div className="border border-slate-800 rounded-lg bg-slate-900/60 p-4">
        <h3 className="text-sm font-semibold text-slate-200 mb-2">
          Phase distribution
        </h3>
        <div className="space-y-1">
          {Object.entries(phaseCounts)
            .sort((a, b) => b[1] - a[1])
            .map(([phase, count]) => {
              const pct = (count / trace.events.length) * 100;
              return (
                <div key={phase} className="flex items-center gap-2">
                  <span className="font-mono text-xs text-slate-400 w-32">{phase}</span>
                  <div className="flex-1 h-3 bg-slate-800 rounded overflow-hidden">
                    <div
                      className="h-full bg-blue-500/60"
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                  <span className="font-mono text-xs text-slate-500 w-16 text-right">
                    {count} ({pct.toFixed(0)}%)
                  </span>
                </div>
              );
            })}
        </div>
      </div>
    </div>
  );
}
