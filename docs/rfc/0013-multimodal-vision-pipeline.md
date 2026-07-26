# RFC 0013: Multimodal Vision Pipeline
Status: Design

## Vision Pipeline Specs
- **Supported Formats:** `.png`, `.jpg`, `.webp`.
- **Sandbox Rules:** `Sandbox` allows image file path reading under asset directories.
- **Context Metadata:** Image paths and dimensions passed via `Context.attachments`.
- **Backend Fallback:** Structured text description generator when vision model offline.
