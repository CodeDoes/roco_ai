# Phase 2: Agent Loop

## Goal
Wire existing tools into a proper agent loop with tool-call parsing, execution, and result feedback.

## Components

### Tool-Call Parsing
Port from `rwkv-ide-interop/lib/parse.ts`:
- Model outputs `<tool_call>{"name":"read","args":{...}}</tool_call>`
- Parser extracts tool name + JSON args
- Router dispatches to tool module
- Result formatted as `<tool_result>...</tool_result>`

### Agent Loop
```typescript
async function agentLoop(input: string, maxDepth = 5) {
  let depth = 0
  while (depth < maxDepth) {
    const response = await generate(prompt)
    const toolCalls = parseToolCalls(response)
    if (toolCalls.length === 0) return cleanText(response)
    for (const call of toolCalls) {
      const result = await executeTool(call)
      prompt += formatToolResult(call, result)
    }
    depth++
  }
}
```

### System Prompt Template
Include tool definitions in XML format:
```xml
<tools>
<tool name="read" description="Read file contents">
<parameter name="path" type="string" required="true"/>
</tool>
...
</tools>
```

### Status
[ ] Port tool-call parser from rwkv-ide-interop
[ ] Port prompt builder with XML tool definitions
[ ] Wire `tools/` modules into executor
[ ] Add tool randomization (description diversity)
[ ] Test with behavioral evals
