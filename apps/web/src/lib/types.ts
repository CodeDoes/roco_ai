/** Mirrors Rust's TraceEvent struct */
export interface TraceEventData {
  ts_ms: number
  phase: string
  actor: string
  detail: string
  meta?: Record<string, unknown>
}

/** Mirrors Rust's TraceSummary struct */
export interface TraceSummary {
  subtask_count: number
  failed_subtasks: number
  model_calls: number
  tool_calls: number
  tool_errors: number
  retries: number
  duration_ms: number
}

/** Mirrors Rust's Trace struct */
export interface TraceData {
  id: string
  objective: string
  events: TraceEventData[]
  messages: Array<{ role: string; content: string }> | unknown[]
  memory: unknown[] | Record<string, unknown> | null
  summary: TraceSummary
}

/** Diff result shape */
export interface TraceDiff {
  id1: string
  id2: string
  events_added: number
  events_removed: number
  subtask_delta: number
  failed_delta: number
  retries_delta: number
}

/** Trace list entry */
export interface TraceEntry {
  id: string
  objective: string
  events: number
  subtasks: number
  failed: number
}
