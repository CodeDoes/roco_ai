# API Reference — `roco-server`

> The server provides HTTP endpoints for web apps (`apps/chat/`, `apps/studio/`) and editor plugins. Start with `--story` to also enable story pipeline endpoints.

## Start the Server

**Basic:**
```bash
RWKV_MODEL=... roco server
# Default port: 8080
```

**With story pipeline:**
```bash
RWKV_MODEL=... roco server --story
```

**As a detach'd daemon:**
```bash
roco server --story --detach
```

## Base Endpoints (always available)

### `GET /health`

Returns server status and model info.

```json
{
  "status": "ok",
  "model": "rwkv7-2.9b.st",
  "adapter": "Vulkan"
}
```

### `GET /vocab`

Returns model vocabulary metadata.

### `POST /complete`

Generate text with optional grammar constraints.

**Request body:**
```json
{
  "prompt": "Write a dark fantasy about a fallen knight",
  "system": "You are a creative writing assistant.",
  "grammar": "optional_gbnf_string",
  "max_tokens": 256,
  "temperature": 0.8
}
```

**Response:**
```json
{
  "text": "The knight's armor...",
  "tokens": 142
}
```

### `POST /v1/completions`

OpenAI-compatible completion endpoint (used by Zed plugin).

**Request:**
```json
{
  "prompt": "a lone cultivator seeking immortality",
  "max_tokens": 256,
  "temperature": 0.4,
  "system": "You are a creative writing assistant."
}
```

**Response:**
```json
{
  "choices": [
    { "text": "The cultivator climbed...", "index": 0, "finish_reason": "length" }
  ]
}
```

## Story Endpoints (require `--story` flag)

### Outline

#### `GET /outline`

Returns the current story outline.

#### `PUT /outline`

Update the story outline.

**Request:**
```json
{
  "chapters": [
    { "number": 1, "title": "The Awakening", "summary": "..." }
  ]
}
```

### Chapters

#### `GET /chapters/:num`

Read a chapter's content.

#### `PUT /chapters/:num`

Save updated chapter content.

**Request:**
```json
{ "content": "The full chapter text..." }
```

#### `POST /chapters/:num/generate`

Generate a new chapter based on current plot state.

**Request:**
```json
{ "direction": "Optional author direction" }
```

**Response:**
```json
{
  "number": 3,
  "title": "The Confrontation",
  "content": "Chapter text..."
}
```

#### `POST /chapters/:num/revise`

Revise a chapter based on natural-language feedback.

**Request:**
```json
{ "feedback": "Make it darker, add more dialogue" }
```

**Response:**
```json
{
  "number": 3,
  "content": "Revised chapter text..."
}
```

#### `GET /chapters/:num/quality`

Run quality evaluation on a chapter.

**Response:**
```json
{
  "overall": 7.0,
  "pacing": 8.0,
  "show_dont_tell": 5.0,
  "character_voice": 7.0,
  "tense_consistency": 9.0,
  "plot_coherence": 7.0,
  "engagement": 8.0,
  "prose_quality": 5.5,
  "issues": [
    { "category": "show_dont_tell", "severity": "medium", "description": "Several emotional states are stated rather than demonstrated" }
  ],
  "strengths": ["Strong pacing keeps the reader engaged"],
  "suggestions": ["Add more sensory detail to setting descriptions"]
}
```

### Suggestions

#### `POST /suggestions`

Get AI suggestions for the current text.

**Request:**
```json
{ "text": "Current document content" }
```

**Response:**
```json
{
  "suggestions": [
    { "type": "Rewrite", "text": "Suggested revision text" }
  ]
}
```

#### `POST /suggestions/apply`

Apply a specific suggestion.

### Writing Assistance

#### `POST /continue`

Continue writing from the current cursor position.

**Request:**
```json
{ "text": "Current line text" }
```

**Response:**
```json
{ "text": "Continuation text..." }
```

### `POST /feedback`

Send natural-language feedback to guide story direction.

**Request:**
```json
{ "feedback": "Make it darker and add more moral ambiguity" }
```

**Response:**
```json
{
  "intent": "style_change",
  "response": "Adjusting tone toward darker, morally ambiguous..."
}
```

### Plot State

#### `GET /plot-state`

Returns the current tracked plot state (characters, locations, conflicts).

### `POST /publish`

Assemble all chapters into the final story document.

### `GET /status`

Returns a summary of the current writing session.

```json
{
  "chapters": 3,
  "words": 4521,
  "quality": 7.0
}
```

## Authentication

Currently no authentication — the server is intended for local use only. Do not expose to the public internet without adding authentication.

## Error Responses

| Status | Meaning |
|---|---|
| `400` | Invalid request (missing prompt, invalid JSON) |
| `404` | Endpoint not found (did you add `--story`?) |
| `500` | Server error (check `RWKV_MODEL`, model file, GPU status) |
| `503` | Model not loaded (check server startup logs) |

## Example (curl)

```bash
# Health check
curl http://localhost:8080/health

# Generate text
curl -X POST http://localhost:8080/complete \
  -H "Content-Type: application/json" \
  -d '{"prompt":"Write a short poem","max_tokens":50}'

# Story: get quality for chapter 1 (requires --story)
curl http://localhost:8080/chapters/1/quality
```
