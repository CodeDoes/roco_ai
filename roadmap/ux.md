# UX Plan — RoCo AI

> The human is the author. The AI is the tool. Every screen should make the
> human feel in control, never reviewed-to-death.

## GUI framework decision

- **gpui: REJECTED** (2026-07-19). Not published for common use; only via a
  ~1GB Zed git fetch; API unstable; stalled the build. Do not revisit.
- **egui: CHOSEN** (2026-07-19). Mature (29.7k★), on crates.io (normal
  `cargo add`), immediate mode — ideal for streaming text + controls.
  Use `egui_markdown` (iamseeley) for *rendered* markdown (wiki, chapter
  preview, comment display). The *editable* markdown editor is built custom
  (egui has no native rich editor — see Lockbook's approach).

## Core philosophy (non-negotiable)

- Human controls pace, not reviews output. Agent does one task → human sees it
  → human decides accept / modify / skip / stop. No mandatory review gates.
- AI output is a *suggestion* until accepted — visually distinct from accepted.

## Build principle: widgets standalone-first, then compose

**Every widget is built and tested in isolation BEFORE being wired into a
combined screen.** This is a hard rule, not a preference:

1. Build widget N as a self-contained unit with its own test(s) proving it
   works on its own (renders, handles input, emits the right event/state).
2. Only after it is green and tested, compose it into a panel / screen.
3. Composition bugs are then isolated to layout, not buried in widget logic.

This keeps the build green and makes each piece verifiable — per the
Definition of Done (surface + control + tested + reversible).

## Widget spec

### Markdown editor (the primary surface — prose is the product)
- **Per-range comments, MS-Word style**: margin annotations tied to specific
  text ranges (not whole-document). Per-range is REQUIRED, especially for
  prose diffs — a human reviews and annotates by sentence/paragraph span.
- **Inline generate / replace with AI**: select a range → generate or replace.
- **Diff view**: show AI change against original at range granularity.
- **Accept-section** and **accept-selection**: accept a whole section, or a
  specific selected range. Rejection discards the suggestion.
- Built custom on `egui::TextEdit` + cursor/range mapping + `Painter`
  overlays (Lockbook-style). egui has no native rich editor.
- Reuse `egui_markdown` for the *rendered/readonly* preview of accepted prose.

### Chat
Message parts (each its own styled widget):
- system message, user message, think part, text part, tool_call part,
  tool_result part, event message.
User input section:
- text area
- capabilities toggles
- send button
- attachments bar
- context info
- **agent pacing control**: planning / careful / rolling / auto-accept
  (maps to the tested `InteractionMode` in `crates/agent/src/interaction.rs`).

### Panels / browsers
- **File tree**
- **Wiki browser** (rendered markdown via `egui_markdown`)
- **Project link graph** (Obsidian-like) — use `egui_graphs` or custom
  `Painter`; build standalone first.
- **Session browser**
- **Project change timeline** (uses existing `VersionControl` /
  `story_persistence`)

## Layout principles
- One artifact in focus; side panels, not tabs fighting for focus.
- Streaming is a preview, not a commitment; stopping does not publish.
- Accept/skip/stop triad always visible, never behind a menu.

## What we are NOT doing
- No new engine features. The core (`crates/agent`, etc.) is frozen.
- No gpui. No webapp revival (`apps/` retired).
- No boiling the ocean: widgets standalone-first, composer later.
