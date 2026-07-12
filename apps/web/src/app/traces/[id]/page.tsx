"use client";

import { use, useEffect, useState } from "react";
import type { TraceT } from "@/lib/schemas";
import { TracePanel } from "@/components/trace/TracePanel";

export default function TraceDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const [trace, setTrace] = useState<TraceT | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`/api/traces/${id}`);
        if (!res.ok) throw new Error(await res.text());
        const data = (await res.json()) as TraceT;
        if (!cancelled) {
          setTrace(data);
          setLoading(false);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
          setLoading(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (loading) {
    return (
      <div className="p-6 text-slate-400 text-sm">Loading trace {id}…</div>
    );
  }
  if (error) {
    return (
      <div className="p-6 bg-red-900/50 border border-red-700 rounded m-6 text-xs text-red-300">
        {error}
      </div>
    );
  }
  return (
    <div className="h-screen flex flex-col bg-slate-950 text-slate-100">
      <header className="px-6 py-3 border-b border-slate-800 bg-slate-900 flex items-center gap-3">
        <a href="/traces" className="text-blue-400 hover:text-blue-300">← Traces</a>
        <span className="font-mono text-sm text-slate-400">{id}</span>
      </header>
      <div className="flex-1 min-h-0">
        <TracePanel trace={trace} />
      </div>
    </div>
  );
}
