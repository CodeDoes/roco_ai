export interface RwkvSession {
  story: string
  model: string
  messages: RwkvMessage[]
  stepCount: number
  status: "new" | "active" | "complete" | "error"
  updatedAt?: string
  error?: string
  statePaths: {
    baseline: string
    checkpoints: Record<string, string>
    latest: string | null
  }
}

export interface RwkvMessage {
  role: "system" | "user" | "assistant" | "tool"
  content: string
}

export interface StoryState {
  title: string
  synopsis: string
  tags: string[]
  currentChapter: number
  chapters: ChapterInfo[]
  planPath: string
}

export interface ChapterInfo {
  num: number
  slug: string
  title: string
  status: "draft" | "complete"
  wordCount: number
  stateCheckpoint: string | null
}

export interface GenerateOpts {
  maxTokens: number
  temperature: number
  topP: number
  repeatPenalty: number
  frequencyPenalty: number
  presencePenalty: number
  grammar?: string
}

export interface GenerateCallbacks {
  onText?: (text: string) => void
  onDone?: () => void
}

export const DEFAULT_GEN_OPTS: GenerateOpts = {
  maxTokens: 1024,
  temperature: 0.8,
  topP: 0.9,
  repeatPenalty: 1.1,
  frequencyPenalty: 0.1,
  presencePenalty: 0,
}

export interface ToolDef {
  name: string
  description: string
  parameters: ToolParam[]
}

export interface ToolParam {
  name: string
  type: "string" | "number" | "boolean"
  description: string
  required: boolean
  enum?: string[]
}

export interface ToolCall {
  name: string
  args: Record<string, unknown>
}

export interface ToolResult {
  name: string
  success: boolean
  data: unknown
  error?: string
}

export type ToolHandler = (args: Record<string, unknown>) => unknown | Promise<unknown>

export interface SessionInfo {
  label: string
  createdAt: string
  updatedAt: string
  statePath: string
  messageCount: number
}

export interface ChatMessage {
  role: "user" | "assistant" | "tool" | "system"
  content: string
  timestamp: string
}
