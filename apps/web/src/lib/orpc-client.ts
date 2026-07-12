import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import { router } from "./orpc";

/**
 * Browser-side oRPC client. Type-safe against the server router
 * (defined in `./orpc`). The server's `RPCHandler` runs at `/api/orpc`,
 * so this client just points there.
 *
 * Usage:
 *   const trace = await orpc.runTask({ objective: "..." });
 */
export const orpc = createORPCClient<any>(new RPCLink({ url: "/api/orpc" }));
