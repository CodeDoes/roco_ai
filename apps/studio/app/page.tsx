"use client";

import { useState } from "react";
import { ChatPanel } from "@/components/chat-panel";
import { EditorPanel } from "@/components/editor-panel";
import { FileBrowser } from "@/components/file-browser";
import { AgentsManager } from "@/components/agents-manager";
import { PanelGroup, Panel, PanelResizeHandle } from "@/components/ui/resizable";

export default function Studio() {
  const [activePanel, setActivePanel] = useState<string>("chat");
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);

  return (
    <div className="h-screen flex flex-col bg-background">
      {/* Header */}
      <header className="border-b px-4 py-2 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h1 className="text-xl font-bold">RoCo Studio</h1>
          <span className="text-sm text-muted-foreground">AI Story Writing</span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setActivePanel("chat")}
            className={`px-3 py-1 rounded text-sm ${
              activePanel === "chat"
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted"
            }`}
          >
            Chat
          </button>
          <button
            onClick={() => setActivePanel("editor")}
            className={`px-3 py-1 rounded text-sm ${
              activePanel === "editor"
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted"
            }`}
          >
            Editor
          </button>
          <button
            onClick={() => setActivePanel("files")}
            className={`px-3 py-1 rounded text-sm ${
              activePanel === "files"
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted"
            }`}
          >
            Files
          </button>
          <button
            onClick={() => setActivePanel("agents")}
            className={`px-3 py-1 rounded text-sm ${
              activePanel === "agents"
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted"
            }`}
          >
            Agents
          </button>
        </div>
      </header>

      {/* Main Content */}
      <div className="flex-1 overflow-hidden">
        <PanelGroup direction="horizontal">
          {/* Sidebar */}
          <Panel defaultSize={20} minSize={15}>
            <div className="h-full border-r">
              {activePanel === "files" ? (
                <FileBrowser
                  onFileSelect={setSelectedFile}
                  selectedFile={selectedFile}
                />
              ) : activePanel === "agents" ? (
                <AgentsManager
                  onAgentSelect={setSelectedAgent}
                  selectedAgent={selectedAgent}
                />
              ) : (
                <FileBrowser
                  onFileSelect={setSelectedFile}
                  selectedFile={selectedFile}
                />
              )}
            </div>
          </Panel>

          <PanelResizeHandle />

          {/* Main Panel */}
          <Panel defaultSize={50} minSize={30}>
            <div className="h-full">
              {activePanel === "chat" ? (
                <ChatPanel />
              ) : activePanel === "editor" ? (
                <EditorPanel file={selectedFile} />
              ) : activePanel === "files" ? (
                <EditorPanel file={selectedFile} />
              ) : activePanel === "agents" ? (
                <EditorPanel file={selectedFile} />
              ) : (
                <ChatPanel />
              )}
            </div>
          </Panel>

          <PanelResizeHandle />

          {/* Right Panel */}
          <Panel defaultSize={30} minSize={20}>
            <div className="h-full border-l">
              <ChatPanel />
            </div>
          </Panel>
        </PanelGroup>
      </div>

      {/* Status Bar */}
      <footer className="border-t px-4 py-1 flex items-center justify-between text-xs text-muted-foreground">
        <div className="flex items-center gap-4">
          <span>Agent: {selectedAgent || "None"}</span>
          <span>File: {selectedFile || "None"}</span>
        </div>
        <div className="flex items-center gap-4">
          <span>Words: 0</span>
          <span>Chapters: 0</span>
          <span>Quality: --</span>
        </div>
      </footer>
    </div>
  );
}
