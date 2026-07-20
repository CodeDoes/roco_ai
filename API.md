# API Reference — `roco-server`

> The server provides HTTP endpoints for web apps (`apps/chat/`, `apps/studio/`) and editor plugins.

## Start the Server

```bash
RWKV_MODEL=... cargo run --release -p roco-server
```

Default port: `8080`

## Endpoints

### `GET /health`

Returns server status and model info.

```json
{
  "status": "ok",
  "model": "rwkv7-2.9b.st",
  "adapter": "Vulkan"
}
```

### `POST /generate`

Generate text with optional grammar constraints.

**Request body:**
```json
{
  "prompt": "Write a dark fantasy about a fallen knight",
  "grammar": "optional_gbnf_string",
  "max_tokens": 256
}
```

**Response:**
```json
{
  "text": "The knight's armor...",
  "tokens": 142
}
```

### `POST /story/outline`

Generate or update a story outline.

**Request:**
```json
{
  "premise": "A lone cultivator levels up alone",
  "direction": { "tone": "dark", "style": "literary" }
}
```

**Response:**
```json
{
  "outline": [
    { "number": 1, "title": "The Awakening", "summary": "..." }
  ]
}
```

### `POST /story/chapter`

Generate a chapter based on outline entry.

### `GET /workspace/:story_id`

List files in a workspace directory.

### `GET /workspace/:story_id/file/:filename`

Read a workspace file (`01-OUTLINE.md`, `06-STORY.md`, etc.).

### `POST /workspace/:story_id/file/:filename`

Write or update a workspace file.

## Authentication

Currently no authentication — the server is intended for local use only. Do not expose to the public internet without adding authentication.

## Error Responses

| Status | Meaning |
|---|---|
| `400` | Invalid request (missing prompt, invalid JSON) |
| `500` | Server error (check `RWKV_MODEL`, model file, GPU status) |
| `503` | Model not loaded (check server startup logs) |

## Example (curl)

```bash
curl -X POST http://localhost:8080/generate \
  -H "Content-Type: application/json" \
  -d '{"prompt":"Write a short poem","max_tokens":50}'
```
