# Tool Calling

Intent: Emit tool/function calls **only** through BNF-constrained channels —
never extract tool calls from free-form text. The model produces valid
tool-call objects because the grammar forbids anything else at every sample.

## Architecture: constrained tool call pipeline

```
ToolRegistry → schema() for each tool
    ↓
jsonschema_to_gbnf(schema) → GBNF fragment per tool arguments
    ↓
MessageFormatOptions{tools: true, tool_schemas: [...]}
    ↓
assistant_response_gbnf(options, schemas) → root ::= asm with embedded <tool_call> productions
    ↓
BnfConstraint transform → vocabulary trie enforces every token is tool-call-valid
    ↓
Model outputs: <tool_call>{"name":"read","arguments":{"path":"main.rs"}}
    ↓
parse_assistant_response() extracts call; serde deserialization always succeeds
```

## Key rule: no unstructured tool calls

```
❌ Model emits: "I'll read the file now: read(path='main.rs')"
   → parse_assistant_response can't extract it reliably

✅ Model emits under GBNF constraint:
   <tool_call>{"name":"read","arguments":{"path":"main.rs"}}
   → exact tag delimiters, valid JSON keys/types enforced by grammar
   → serde_json::from_str() is guaranteed to succeed
```

## Sub-goals

- **Per-tool GBNF compilation**: Each registered tool's argument schema compiles
  to a GBNF fragment (`{"name":enum([...]),"arguments":tool-schema-gbnf}`)
- **Tool-scope grammar embedding**: `assistant_response_gbnf()` embeds tool
  productions only for tools active in the current context (gradual disclosure)
- **Result envelope**: Tool results use `<tool_result>text</tool_result>` with
  strict tag boundaries so the harness can reliably separate result from prose
- **Error handling as structured output**: Tool failures produce a consistent
  `{"error":"..."}` shape via grammar, never ad-hoc text
