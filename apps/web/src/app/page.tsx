import { ChatPanel } from "@/components/chat/ChatPanel";

export default function Home() {
  return (
    <div className="flex flex-col h-screen bg-slate-950 text-slate-100">
      <header className="px-6 py-3 border-b border-slate-800 bg-slate-900 flex items-center justify-between shrink-0">
        <div className="flex items-center gap-3">
          <span className="text-xl font-bold text-blue-400">RoCo AI</span>
          <span className="text-xs text-slate-500 bg-slate-800 px-2 py-0.5 rounded">
            Chat · oRPC · zod · assistant-ui
          </span>
        </div>
      </header>
      <main className="flex-1 min-h-0">
        <ChatPanel />
      </main>
    </div>
  );
}
