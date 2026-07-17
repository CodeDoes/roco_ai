"use client";

import { AssistantRuntimeProvider } from "@assistant-ui/react";
import { useChatRuntime } from "@assistant-ui/react-ai-sdk";
import { Thread } from "@/components/assistant-ui/thread";

export function ChatPanel() {
  const runtime = useChatRuntime({
    api: "/api/chat",
  });

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <div className="h-full flex flex-col">
        <div className="border-b px-4 py-2">
          <h2 className="font-semibold">Chat</h2>
          <p className="text-xs text-muted-foreground">
            Talk to RoCo AI about your story
          </p>
        </div>
        <div className="flex-1 overflow-hidden">
          <Thread />
        </div>
      </div>
    </AssistantRuntimeProvider>
  );
}
