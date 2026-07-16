# Goals: browser_use

## Grammar-First Principle

Browser automation requires structured extraction and action planning. Page understanding and tool derivation are grammar-constrained by BNF grammars, ensuring reliable structured output from web content (see `goals/infer/gbnf.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **agent_controlled_browser** — the agent opens, navigates, and controls a browser
2. **page_understanding** — extracting structure and content from web pages
3. **browser_derived_tools** — tools auto-generated from page content (forms, buttons, links)
4. **stealth** — anti-detection: fingerprint rotation, Camoufox, undetected-chromedriver


## Status & Self-Directed Actions

drive a headless browser through workspace-scoped tools.

Prerequisite order (mirrors the product layer):

1. **agent_controlled_browser** ⬜ *self-directed:* a `WorkspaceBashTool`
   already scopes shell execution; extend it with a `browser` tool (e.g.
   Playwright/headless-chrome) that operates inside the workspace dir.
2. **page_understanding** ⬜ *self-directed:* screenshot + readability
   extraction fed back as a tool result the model can reason over.
3. **browser_derived_tools** ⬜ *self-directed:* click/type/scrape actions
   exposed as tools, all confined to the workspace boundary.
4. **stealth** ⬜ *self-directed:* later; not a near-term concern.

**Next self-directed action:** skip for now. Revisit only after `agent`
orchestration and `session_search` land, since browser use depends on a
reliable agent loop.
