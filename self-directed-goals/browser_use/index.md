# Self-Directed Goals: browser_use

Reflection of [`goals/browser_use/index.md`](../../goals/browser_use/index.md).
Not started. My self-directed view: defer until the agent loop is robust, then
drive a headless browser through workspace-scoped tools.

Prerequisite order (mirrors the product layer):

1. **agent_controlled_browser** — ⬜ *self-directed:* a `WorkspaceBashTool`
   already scopes shell execution; extend it with a `browser` tool (e.g.
   Playwright/headless-chrome) that operates inside the workspace dir.
2. **page_understanding** — ⬜ *self-directed:* screenshot + readability
   extraction fed back as a tool result the model can reason over.
3. **browser_derived_tools** — ⬜ *self-directed:* click/type/scrape actions
   exposed as tools, all confined to the workspace boundary.
4. **stealth** — ⬜ *self-directed:* later; not a near-term concern.

**Next self-directed action:** skip for now. Revisit only after `agent`
orchestration and `session_search` land, since browser use depends on a
reliable agent loop.
