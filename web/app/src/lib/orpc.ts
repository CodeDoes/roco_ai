// RoCo Web App — oRPC router (Phase 2 scaffold).
//
// Procedures call the RoCo CLI via exec as a bridge until napi-rs is compiled.
// Once the .node addon is built, swap the exec calls for direct addon invocations.

import { z } from 'zod'
import { initServer, type inferRouterInput, type inferRouterOutput } from '@orpc/server'
import { RPCHandler } from '@orpc/server/fetch'

export const router = initServer().router({
  runTask: {
    input: z.object({
      objective: z.string(),
      context: z.string().optional().default(''),
      outputSchema: z.string().optional().default(''),
      allowAbstain: z.boolean().optional().default(true),
    }),
    handler: async ({ input }) => {
      // TODO: swap for direct napi call once roco_napi .node is built
      // const roco = await import('roco_napi')
      // return JSON.parse(await roco.runTask({ ... }))
      return { id: 'demo', objective: input.objective, events: [], messages: [], memory: null, summary: { subtask_count: 0, failed_subtasks: 0, model_calls: 0, tool_calls: 0, tool_errors: 0, retries: 0, duration_ms: 0 } }
    },
  },
  listTraces: {
    handler: async () => {
      return [] as Array<{ id: string; objective: string; events: number; subtasks: number; failed: number }>
    },
  },
  loadTrace: {
    input: z.object({ id: z.string() }),
    handler: async ({ input }) => {
      return { id: input.id, objective: '', events: [], messages: [], memory: null, summary: { subtask_count: 0, failed_subtasks: 0, model_calls: 0, tool_calls: 0, tool_errors: 0, retries: 0, duration_ms: 0 } }
    },
  },
  diffTraces: {
    input: z.object({ id1: z.string(), id2: z.string() }),
    handler: async ({ input }) => {
      return { id1: input.id1, id2: input.id2, events_added: 0, events_removed: 0, subtask_delta: 0, failed_delta: 0, retries_delta: 0 }
    },
  },
})

export type Router = typeof router
export type RouterInput = inferRouterInput<Router>
export type RouterOutput = inferRouterOutput<Router>

// API Route handler (Next.js App Router)
export const handler = new RPCHandler(router)
