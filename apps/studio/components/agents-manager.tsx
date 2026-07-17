"use client";

import { useEffect, useState } from "react";
import {
  Bot,
  Plus,
  Settings,
  Trash,
  Play,
  Pause,
  RefreshCw,
} from "lucide-react";

interface AgentsManagerProps {
  onAgentSelect: (agentId: string) => void;
  selectedAgent: string | null;
}

interface Agent {
  id: string;
  name: string;
  type: string;
  status: "idle" | "running" | "paused" | "error";
  description: string;
  tasks: number;
  lastActive: string;
}

export function AgentsManager({ onAgentSelect, selectedAgent }: AgentsManagerProps) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    loadAgents();
  }, []);

  async function loadAgents() {
    setIsLoading(true);
    try {
      const response = await fetch("/api/agents");
      const data = await response.json();
      setAgents(data.agents || []);
    } catch (error) {
      console.error("Failed to load agents:", error);
    } finally {
      setIsLoading(false);
    }
  }

  async function createAgent() {
    try {
      const response = await fetch("/api/agents", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: "New Agent",
          type: "storyteller",
          description: "A new story writing agent",
        }),
      });
      const data = await response.json();
      setAgents((prev) => [...prev, data.agent]);
    } catch (error) {
      console.error("Failed to create agent:", error);
    }
  }

  async function deleteAgent(agentId: string) {
    try {
      await fetch(`/api/agents?id=${agentId}`, { method: "DELETE" });
      setAgents((prev) => prev.filter((a) => a.id !== agentId));
    } catch (error) {
      console.error("Failed to delete agent:", error);
    }
  }

  async function toggleAgent(agentId: string) {
    const agent = agents.find((a) => a.id === agentId);
    if (!agent) return;

    try {
      const newStatus = agent.status === "running" ? "paused" : "running";
      await fetch(`/api/agents?id=${agentId}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ status: newStatus }),
      });
      setAgents((prev) =>
        prev.map((a) =>
          a.id === agentId ? { ...a, status: newStatus } : a
        )
      );
    } catch (error) {
      console.error("Failed to toggle agent:", error);
    }
  }

  function getStatusColor(status: string) {
    switch (status) {
      case "running":
        return "text-green-500";
      case "paused":
        return "text-yellow-500";
      case "error":
        return "text-red-500";
      default:
        return "text-muted-foreground";
    }
  }

  function getStatusIcon(status: string) {
    switch (status) {
      case "running":
        return <Play className="h-4 w-4" />;
      case "paused":
        return <Pause className="h-4 w-4" />;
      case "error":
        return <RefreshCw className="h-4 w-4" />;
      default:
        return <Bot className="h-4 w-4" />;
    }
  }

  return (
    <div className="h-full flex flex-col">
      <div className="border-b px-4 py-2 flex items-center justify-between">
        <div>
          <h2 className="font-semibold">Agents</h2>
          <p className="text-xs text-muted-foreground">
            Manage your AI agents
          </p>
        </div>
        <button
          onClick={createAgent}
          className="px-2 py-1 rounded text-sm bg-primary text-primary-foreground hover:bg-primary/90"
        >
          <Plus className="h-4 w-4" />
        </button>
      </div>

      <div className="flex-1 overflow-auto">
        {isLoading ? (
          <div className="flex items-center justify-center h-full">
            <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary"></div>
          </div>
        ) : agents.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-muted-foreground p-4">
            <Bot className="h-12 w-12 mb-2" />
            <p className="text-sm">No agents yet</p>
            <p className="text-xs">Click + to create one</p>
          </div>
        ) : (
          <div className="py-2">
            {agents.map((agent) => (
              <div
                key={agent.id}
                className={`flex items-center gap-3 px-4 py-3 cursor-pointer hover:bg-muted ${
                  selectedAgent === agent.id ? "bg-muted" : ""
                }`}
                onClick={() => onAgentSelect(agent.id)}
              >
                <div className={getStatusColor(agent.status)}>
                  {getStatusIcon(agent.status)}
                </div>

                <div className="flex-1 min-w-0">
                  <div className="font-medium text-sm truncate">
                    {agent.name}
                  </div>
                  <div className="text-xs text-muted-foreground truncate">
                    {agent.description}
                  </div>
                </div>

                <div className="flex items-center gap-1">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleAgent(agent.id);
                    }}
                    className="p-1 rounded hover:bg-background"
                  >
                    {agent.status === "running" ? (
                      <Pause className="h-4 w-4" />
                    ) : (
                      <Play className="h-4 w-4" />
                    )}
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      deleteAgent(agent.id);
                    }}
                    className="p-1 rounded hover:bg-background"
                  >
                    <Trash className="h-4 w-4" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="border-t p-2">
        <button
          onClick={loadAgents}
          className="w-full px-3 py-1 rounded text-sm hover:bg-muted"
        >
          Refresh
        </button>
      </div>
    </div>
  );
}
