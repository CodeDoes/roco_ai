/**
 * oRPC API Route — handles all oRPC procedure calls at `/api/orpc`.
 *
 * This is the main entry point for the oRPC server. Any procedure
 * defined in `@/lib/orpc` is callable via POST to `/api/orpc?procedure=xxx`.
 */
import { handler } from '@/lib/orpc'

export const runtime = 'nodejs'

export async function GET(req: Request) {
  const { matched, response } = await handler.handle(req)
  if (!matched) {
    return new Response('Not Found', { status: 404 })
  }
  return response
}

export async function POST(req: Request) {
  const { matched, response } = await handler.handle(req)
  if (!matched) {
    return new Response('Not Found', { status: 404 })
  }
  return response
}
