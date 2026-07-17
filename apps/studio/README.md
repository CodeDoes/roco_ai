# RoCo Studio

A unified interface for AI story writing with:
- **Chat** - Talk to RoCo AI about your story
- **Editor** - Rich text editing with ProseMirror
- **File Browser** - Navigate and manage files
- **Agents Manager** - Manage multiple AI agents

## Features

### Chat
- Modern chat UI with assistant-ui
- Streaming responses from RoCo AI
- Markdown support
- Code highlighting

### Editor
- Rich text editing with ProseMirror
- Markdown support
- Auto-save
- Syntax highlighting

### File Browser
- Tree view of story files
- Navigate directories
- Select files to edit
- Refresh file list

### Agents Manager
- Create/delete agents
- Start/stop agents
- View agent status
- Configure agent settings

## Setup

### 1. Install dependencies
```bash
cd apps/studio
npm install
```

### 2. Start the RoCo API server
```bash
cargo run --release -p roco-server
```

### 3. Start the studio
```bash
npm run dev
```

### 4. Open in browser
Open http://localhost:3000

## Usage

### Chat with RoCo AI
1. Click the "Chat" tab in the header
2. Type a message in the input field
3. Press Enter or click Send
4. RoCo AI will respond with streaming text

### Edit Files
1. Click the "Files" tab in the sidebar
2. Navigate to a file
3. Click the file to open it in the editor
4. Edit the content
5. Click "Save" to save changes

### Manage Agents
1. Click the "Agents" tab in the sidebar
2. Click "+" to create a new agent
3. Click the play button to start an agent
4. Click the pause button to pause an agent
5. Click the trash button to delete an agent

### Give Feedback
You can give feedback in natural language:
- "make it darker"
- "add more dialogue"
- "the pacing is too slow"

### Get Suggestions
Ask for suggestions:
- "suggest a continuation"
- "what should happen next?"
- "how can I improve this?"

### Edit the Outline
Ask to edit the outline:
- "add a chapter about the knight's past"
- "remove chapter 3"
- "move chapter 1 to chapter 3"

## API

The studio connects to the RoCo API at `http://localhost:3000`.

### Endpoints
- `GET /files` - List files
- `GET /files?path=...` - Get file content
- `PUT /files` - Save file
- `GET /agents` - List agents
- `POST /agents` - Create agent
- `PATCH /agents?id=...` - Update agent
- `DELETE /agents?id=...` - Delete agent

## Development

### Project Structure
```
apps/studio/
├── app/
│   ├── api/
│   │   ├── chat/route.ts
│   │   ├── files/route.ts
│   │   └── agents/route.ts
│   ├── page.tsx
│   └── layout.tsx
├── components/
│   ├── assistant-ui/
│   │   └── thread.tsx
│   ├── ui/
│   │   ├── button.tsx
│   │   ├── avatar.tsx
│   │   ├── scroll-area.tsx
│   │   └── resizable.tsx
│   ├── chat-panel.tsx
│   ├── editor-panel.tsx
│   ├── file-browser.tsx
│   ├── agents-manager.tsx
│   └── prose-mirror-editor.tsx
├── lib/
│   └── utils.ts
├── package.json
├── tailwind.config.ts
└── tsconfig.json
```

### Key Files
- `app/page.tsx` - Main page with layout
- `components/chat-panel.tsx` - Chat panel
- `components/editor-panel.tsx` - Editor panel
- `components/file-browser.tsx` - File browser
- `components/agents-manager.tsx` - Agents manager
- `components/prose-mirror-editor.tsx` - ProseMirror editor

### Adding Features
1. Add new UI components in `components/ui/`
2. Add new API routes in `app/api/`
3. Update the panels to use new features

## Troubleshooting

### "Connection refused"
Make sure the RoCo API server is running:
```bash
cargo run --release -p roco-server
```

### "Slow responses"
The API server might be loading the model. Wait a few seconds and try again.

### "No response"
Check the API server logs for errors.

### "Files not loading"
Make sure the RoCo API server is running and the workspace exists.

## Next Steps

1. Add more chat features (history, search, etc.)
2. Add voice input/output
3. Add file upload/download
4. Add collaborative editing
5. Add mobile support
6. Add more editor features (syntax highlighting, etc.)
7. Add more agent features (configuration, etc.)
