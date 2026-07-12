"use client";

import type { TraceSummaryT } from "@/lib/schemas";

type Card = { label: string; value: string | number; tone?: "good" | "bad" | "neutral" };

export function StatCards({ summary }: { summary: TraceSummaryT }) {
  const cards: Card[] = [
    { label: "Subtasks", value: summary.subtask_count },
    {
      label: "Failed",
      value: summary.failed_subtasks,
      tone: summary.failed_subtasks > 0 ? "bad" : "good",
    },
    { label: "Model Calls", value: summary.model_calls },
    { label: "Tool Calls", value: summary.tool_calls },
    {
      label: "Retries",
      value: summary.retries,
      tone: summary.retries > 0 ? "bad" : "good",
    },
    { label: "Duration", value: `${summary.duration_ms}ms` },
  ];

  const toneClass = (t?: Card["tone"]) =>
    t === "bad"
      ? "text-red-400 border-red-700/60"
      : t === "good"
        ? "text-emerald-400 border-emerald-700/60"
        : "text-blue-400 border-slate-700";

  return (
    <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2">
      {cards.map((c) => (
        <div
          key={c.label}
          className={`bg-slate-900/60 border rounded-lg px-3 py-2 ${toneClass(c.tone)}`}
        >
          <div className="text-[10px] uppercase tracking-wider text-slate-500">
            {c.label}
          </div>
          <div className="font-mono text-lg">{c.value}</div>
        </div>
      ))}
    </div>
  );
}
