import { z } from "zod";

/** Client-side zod schemas (mirror the Rust gateway's request/response). */

export const RunTaskInput = z.object({
  objective: z.string().min(1),
  context: z.string().optional().default(""),
  outputSchema: z.string().optional().default(""),
  allowAbstain: z.boolean().optional().default(true),
});
export type RunTaskInputT = z.infer<typeof RunTaskInput>;

export const TraceEventSchema = z.object({
  ts_ms: z.number(),
  phase: z.string(),
  actor: z.string(),
  detail: z.string(),
  meta: z.record(z.unknown()).optional(),
});
export type TraceEventT = z.infer<typeof TraceEventSchema>;

export const TraceSummarySchema = z.object({
  subtask_count: z.number(),
  failed_subtasks: z.number(),
  model_calls: z.number(),
  tool_calls: z.number(),
  tool_errors: z.number(),
  retries: z.number(),
  duration_ms: z.number(),
});
export type TraceSummaryT = z.infer<typeof TraceSummarySchema>;

export const TraceSchema = z.object({
  id: z.string(),
  objective: z.string(),
  events: z.array(TraceEventSchema),
  messages: z.array(z.object({ role: z.string(), content: z.string() }).or(z.unknown())),
  memory: z.unknown().nullable(),
  summary: TraceSummarySchema,
});
export type TraceT = z.infer<typeof TraceSchema>;
