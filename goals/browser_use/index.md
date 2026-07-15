# Goals: browser_use

## Grammar-First Principle

Browser automation requires structured extraction and action planning. Page understanding and tool derivation are grammar-constrained by BNF grammars, ensuring reliable structured output from web content (see `goals/infer/gbnf.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **agent_controlled_browser** — the agent opens, navigates, and controls a browser
2. **page_understanding** — extracting structure and content from web pages
3. **browser_derived_tools** — tools auto-generated from page content (forms, buttons, links)
4. **stealth** — anti-detection: fingerprint rotation, Camoufox, undetected-chromedriver
