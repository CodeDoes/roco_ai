"use client";

import { useRef, useState } from "react";
import {
  type ChatModelAdapter,
  useLocalRuntime,
  AssistantRuntimeProvider,
  ThreadPrimitive,
  ComposerPrimitive,
  MessagePrimitive,
} from "@assistant-ui/react";
import { orpc } from "@/lib/orpc-client";
import type { TraceT } from "@/lib/schemas";
import { TracePanel } from "@/components/trace/TracePanel";

/** Pull the latest user objective out of the assistant-ui message stream. */
function extractText(
  messages: { content?: { type: string; text?: string }[] }[],
): string {
  const last = messages[messages.length - 1];
  if (!last?.content) return "";
  return (last.content as { type: string; text?: string }[])
    .filter((p) => p?.type === "text")
    .map((p) => p?.text ?? "")
    .join("");
}

function summarize(summary: TraceT["summary"]): string {
  return (
    `Done. ${summary.subtask_count} subtasks (${summary.failed_subtasks} failed), ` +
    `${summary.model_calls} model calls, ${summary.tool_calls} tool calls, ` +
    `${summary.retries} retries, in ${summary.duration_ms}ms.`
  );
}

function UserMessage() {
  return (
    <MessagePrimitive.Root className="flex justify-end">
      <div className="bg-blue-600/20 border border-blue-700/40 rounded-lg px-3 py-2 max-w-[80%] text-sm text-slate-100">
        <MessagePrimitive.Parts />
      </div>
    </MessagePrimitive.Root>
  );
}
function AssistantMessage() {
  return (
    <MessagePrimitive.Root className="flex justify-start">
      <div className="bg-slate-800/60 border border-slate-700 rounded-lg px-3 py-2 max-w-[80%] text-sm text-slate-100 whitespace-pre-wrap">
        <MessagePrimitive.Parts />
      </div>
    </MessagePrimitive.Root>
  );
}

export function ChatPanel() {
  const [trace, setTrace] = useState<TraceT | null>(null);
  const setTraceRef = useRef(setTrace);
  setTraceRef.current = setTrace;

  // Stable adapter (useRef so the runtime isn't recreated each render).
  const adapterRef = useRef<ChatModelAdapter>({
    async *run({ messages }) {
      const objective = extractText(messages as unknown as Parameters<typeof extractText>[0]);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const result = await (orpc as any).runTask({
        objective,
        context: "",
        outputSchema: "",
        allowAbstain: true,
      });
      setTraceRef.current(result as TraceT);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      yield { type: "text", text: summarize(result.summary) } as any;
    },
  }).current;

  const runtime = useLocalRuntime(adapterRef);

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <div className="flex flex-col h-full">
        <ThreadPrimitive.Root className="flex flex-col flex-1 min-h-0">
          <ThreadPrimitive.Viewport className="flex-1 overflow-y-auto px-4 py-3 space-y-3">
            <ThreadPrimitive.Empty>
              Ask RoCo to run a task (e.g.{" "}
              <em>"Summarize the provided facts"</em>).
            </ThreadPrimitive.Empty>
            <ThreadPrimitive.Messages
              components={{ UserMessage, AssistantMessage }}
            />
          </ThreadPrimitive.Viewport>

          <ComposerPrimitive.Root className="border-t border-slate-800 p-3 flex gap-2">
            <ComposerPrimitive.Input
              className="flex-1 bg-slate-800 border border-slate-700 rounded px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-blue-500"
              placeholder="Enter an objective..."
            />
            <ComposerPrimitive.Send>Send</ComposerPrimitive.Send>
          </ComposerPrimitive.Root>
        </ThreadPrimitive.Root>

        <div className="border-t border-slate-800 bg-slate-950" style={{ height: "55%" }}>
          <TracePanel trace={trace} />
        </div>
      </div>
    </AssistantRuntimeProvider>
  );
}
