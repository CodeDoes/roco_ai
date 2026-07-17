# RoCo AI Chat

A modern chat interface for RoCo AI built with:
- **assistant-ui** - React component library for AI chat
- **ai-sdk** - Vercel's AI SDK
- **Next.js** - React framework
- **Tailwind CSS** - Styling

## Features

- Modern chat UI with assistant-ui
- Streaming responses from RoCo API
- Markdown support
- Code highlighting
- Responsive design

## Setup

### 1. Install dependencies
```bash
cd apps/chat
npm install
```

### 2. Start the RoCo API server
```bash
cargo run --release -p roco-server
```

### 3. Start the chat app
```bash
npm run dev
```

### 4. Open in browser
Open http://localhost:3000

## Usage

### Chat with RoCo AI
1. Type a message in the input field
2. Press Enter or click Send
3. RoCo AI will respond with streaming text

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

The chat app connects to the RoCo API at `http://localhost:3000`.

### Endpoints
- `POST /api/chat` - Send a chat message
- `POST /api/suggestions` - Get suggestions
- `POST /api/feedback` - Send feedback
- `GET /api/plot-state` - Get plot state

## Development

### Project Structure
```
apps/chat/
├── app/
│   ├── api/
│   │   └── chat/
│   │       └── route.ts
│   ├── page.tsx
│   └── layout.tsx
├── components/
│   ├── assistant-ui/
│   │   └── thread.tsx
│   ├── ui/
│   │   ├── button.tsx
│   │   ├── avatar.tsx
│   │   └── scroll-area.tsx
│   └── chat.tsx
├── lib/
│   └── utils.ts
├── package.json
├── tailwind.config.ts
└── tsconfig.json
```

### Key Files
- `app/page.tsx` - Main page with chat component
- `app/api/chat/route.ts` - API route connecting to RoCo
- `components/chat.tsx` - Chat component using assistant-ui
- `components/assistant-ui/thread.tsx` - Thread component

### Adding Features
1. Add new UI components in `components/ui/`
2. Add new API routes in `app/api/`
3. Update the chat component to use new features

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

## Next Steps

1. Add more chat features (history, search, etc.)
2. Add voice input/output
3. Add file upload/download
4. Add collaborative editing
5. Add mobile support
