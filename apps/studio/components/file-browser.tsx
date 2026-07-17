"use client";

import { useEffect, useState } from "react";
import {
  ChevronRight,
  ChevronDown,
  File,
  Folder,
  FolderOpen,
} from "lucide-react";

interface FileBrowserProps {
  onFileSelect: (path: string) => void;
  selectedFile: string | null;
}

interface FileItem {
  name: string;
  path: string;
  type: "file" | "directory";
  children?: FileItem[];
}

export function FileBrowser({ onFileSelect, selectedFile }: FileBrowserProps) {
  const [files, setFiles] = useState<FileItem[]>([]);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    loadFiles();
  }, []);

  async function loadFiles() {
    setIsLoading(true);
    try {
      const response = await fetch("/api/files");
      const data = await response.json();
      setFiles(data.files || []);
    } catch (error) {
      console.error("Failed to load files:", error);
    } finally {
      setIsLoading(false);
    }
  }

  function toggleExpand(path: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  function renderItem(item: FileItem, depth: number = 0) {
    const isExpanded = expanded.has(item.path);
    const isSelected = selectedFile === item.path;
    const isDirectory = item.type === "directory";

    return (
      <div key={item.path}>
        <div
          className={`flex items-center gap-1 px-2 py-1 cursor-pointer hover:bg-muted ${
            isSelected ? "bg-muted" : ""
          }`}
          style={{ paddingLeft: `${depth * 16 + 8}px` }}
          onClick={() => {
            if (isDirectory) {
              toggleExpand(item.path);
            } else {
              onFileSelect(item.path);
            }
          }}
        >
          {isDirectory ? (
            <>
              {isExpanded ? (
                <ChevronDown className="h-4 w-4 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-4 w-4 text-muted-foreground" />
              )}
              {isExpanded ? (
                <FolderOpen className="h-4 w-4 text-muted-foreground" />
              ) : (
                <Folder className="h-4 w-4 text-muted-foreground" />
              )}
            </>
          ) : (
            <>
              <div className="w-4" />
              <File className="h-4 w-4 text-muted-foreground" />
            </>
          )}
          <span className="text-sm truncate">{item.name}</span>
        </div>

        {isDirectory && isExpanded && item.children && (
          <div>
            {item.children.map((child) => renderItem(child, depth + 1))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      <div className="border-b px-4 py-2">
        <h2 className="font-semibold">Files</h2>
        <p className="text-xs text-muted-foreground">
          Browse your story files
        </p>
      </div>

      <div className="flex-1 overflow-auto">
        {isLoading ? (
          <div className="flex items-center justify-center h-full">
            <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary"></div>
          </div>
        ) : (
          <div className="py-2">
            {files.map((file) => renderItem(file))}
          </div>
        )}
      </div>

      <div className="border-t p-2">
        <button
          onClick={loadFiles}
          className="w-full px-3 py-1 rounded text-sm hover:bg-muted"
        >
          Refresh
        </button>
      </div>
    </div>
  );
}
