"use client";

import {
  ThreadPrimitive,
  MessagePrimitive,
  ComposerPrimitive,
} from "@assistant-ui/react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar";
import { Send, Sparkles, User } from "lucide-react";

export function Thread() {
  return (
    <ThreadPrimitive.Root className="flex h-full flex-col">
      <ThreadPrimitive.Viewport className="flex-1 overflow-y-auto">
        <ScrollArea className="h-full">
          <div className="p-4 space-y-4">
            <ThreadPrimitive.Messages
              components={{
                UserMessage: UserMessage,
                AssistantMessage: AssistantMessage,
              }}
            />
          </div>
        </ScrollArea>
      </ThreadPrimitive.Viewport>

      <div className="border-t p-4">
        <Composer />
      </div>
    </ThreadPrimitive.Root>
  );
}

function UserMessage() {
  return (
    <MessagePrimitive.Root className="flex gap-3">
      <Avatar className="h-8 w-8">
        <AvatarFallback>
          <User className="h-4 w-4" />
        </AvatarFallback>
      </Avatar>
      <div className="flex-1">
        <div className="font-semibold text-sm">You</div>
        <MessagePrimitive.Content className="mt-1 text-sm" />
      </div>
    </MessagePrimitive.Root>
  );
}

function AssistantMessage() {
  return (
    <MessagePrimitive.Root className="flex gap-3">
      <Avatar className="h-8 w-8">
        <AvatarFallback>
          <Sparkles className="h-4 w-4" />
        </AvatarFallback>
      </Avatar>
      <div className="flex-1">
        <div className="font-semibold text-sm">RoCo AI</div>
        <MessagePrimitive.Content className="mt-1 text-sm" />
      </div>
    </MessagePrimitive.Root>
  );
}

function Composer() {
  return (
    <ComposerPrimitive.Root className="flex gap-2">
      <ComposerPrimitive.Input
        className="flex-1 rounded-md border bg-background px-3 py-2 text-sm"
        placeholder="Type a message..."
      />
      <ComposerPrimitive.Send asChild>
        <Button size="icon">
          <Send className="h-4 w-4" />
        </Button>
      </ComposerPrimitive.Send>
    </ComposerPrimitive.Root>
  );
}
