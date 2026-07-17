"use client";

import { useEffect, useState } from "react";
import { ProseMirrorEditor } from "@/components/prose-mirror-editor";

interface EditorPanelProps {
  file: string | null;
}

export function EditorPanel({ file }: EditorPanelProps) {
  const [content, setContent] = useState<string>("");
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    if (file) {
      loadFile(file);
    }
  }, [file]);

  async function loadFile(path: string) {
    setIsLoading(true);
    try {
      const response = await fetch(`/api/files?path=${encodeURIComponent(path)}`);
      const data = await response.json();
      setContent(data.content || "");
    } catch (error) {
      console.error("Failed to load file:", error);
    } finally {
      setIsLoading(false);
    }
  }

  async function saveFile() {
    if (!file) return;
    try {
      await fetch("/api/files", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: file, content }),
      });
    } catch (error) {
      console.error("Failed to save file:", error);
    }
  }

  return (
    <div className="h-full flex flex-col">
      <div className="border-b px-4 py-2 flex items-center justify-between">
        <div>
          <h2 className="font-semibold">Editor</h2>
          <p className="text-xs text-muted-foreground">
            {file || "No file selected"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={saveFile}
            disabled={!file}
            className="px-3 py-1 rounded text-sm bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            Save
          </button>
          <button
            onClick={() => loadFile(file || "")}
            disabled={!file}
            className="px-3 py-1 rounded text-sm hover:bg-muted disabled:opacity-50"
          >
            Reload
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-auto p-4">
        {isLoading ? (
          <div className="flex items-center justify-center h-full">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
          </div>
        ) : file ? (
          <ProseMirrorEditor
            content={content}
            onChange={setContent}
          />
        ) : (
          <div className="flex items-center justify-center h-full text-muted-foreground">
            Select a file to edit
          </div>
        )}
      </div>
    </div>
  );
}
