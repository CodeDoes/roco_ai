# TASK 05 — CHAT MIGRATION (Web Chat → Desktop `ChatWidget`)

> **Reference:** `AGENTS.md` Sections A, C, D, E.2, G; `EDIT_GUIDE.md`; `STRATEGIC_PLAN.md` Phase 4.2; `TASK_02_DESKTOP_INTERACTION.md` Phase 3.2 completed (interactive mode wired).
> **Status:** Migration target (`apps/chat/` features → `crates/ui/src/chat.rs`).
> **Rule:** Same as `TASK_04_EDITOR_MIGRATION.md`: migrate existing capabilities, don't invent new ones. `crates/ui/src/chat.rs` is `Always` edit zone.

---

## PROBLEM (Migration Scope)

`apps/chat/` provides streaming assistant messages, markdown formatting, message history (`ChatMessage` roles), clear/undo/retry actions. Desktop `ChatWidget` (`chat.rs`) exists but may lack streaming display, markdown formatting, or full message action set. This task verifies and extends desktop chat to match web capabilities using direct `AppContext` backend (no server HTTP required for core chat).

---

## PREREQUISITE CHECKLIST

```bash
# Interactive mode works in desktop
run_desktop.sh
# Confirm: click in chat area; type message; press Enter or click Send; see response appear (even dummy response is acceptable for migration milestone — the stream mechanism is the target).
```
**If desktop chat doesn't respond:** See `TASK_02_DESKTOP_INTERACTION.md` Phase 3.2 troubleshooting (`PacingAction` mapping, `AppContext` wire). Fix first.

---

## WHAT TO READ

- `AGENTS.md` Section D (`Architecture`: `AppContext::generate_stream()` capability — confirm it exists in `crates/app/src/lib.rs`).
- `crates/ui/src/chat.rs` (full file — check `ChatMessage` roles, `ChatWidgetState`, `ChatAction`, streaming mechanism).
- `crates/app/src/lib.rs` (capability list: `generate_stream`, `generate_poll_finish`).
- `apps/chat/components/chat.tsx` (streaming mechanism, markdown rendering, message actions).
- `TASK_02_DESKTOP_INTERACTION.md` Milestone 3.2 (interactive mode must work).

---

## MIGRATION TARGETS (Web → Desktop)

| Web Feature (`apps/chat/`) | Desktop Target (`chat.rs`) | Verification Command / Check |
|---|---|---|
| Message roles (`user`, `assistant`, `system`) | `MessageRole` enum (`chat.rs` line with `enum MessageRole`) | `grep -n "MessageRole" crates/ui/src/chat.rs` |
| Streaming assistant messages | `ChatAction::SendMessage` → `AppContext::generate_stream()` → stream tokens to `ChatMessage::assistant()` | Check `handle_chat_action()` in `desktop_app.rs` for stream handling |
| Markdown formatting (`markdown` rendering) | `ChatMessage` content rendering (`RichText` or `egui_markdown`) | Visual: assistant message shows formatted text |
| Message history (`messages` array persistence) | `ChatWidgetState::messages` (check persistence through session save/load) | `grep -n "messages" crates/ui/src/chat.rs` |
| Clear / Undo / Retry actions | `ChatAction::Clear`, `Undo`, `Retry` (check if all exist) | `grep -n "ChatAction::" crates/ui/src/chat.rs` |
| Copy message (`ChatAction::CopyMessage`) | Confirm handler in `desktop_app.rs` (`handle_chat_action()`) | `grep -n "CopyMessage" crates/ui/src/chat.rs` |

---

## STEP-BY-STEP MIGRATION

### Step 1 — Confirm Streaming Mechanism Exists

```bash
grep -n "ChatAction::SendMessage\|generate_stream\|block_on" crates/ui/src/desktop_app.rs | head -n 15
```
Look for: `handle_chat_action()` handles `SendMessage` by calling backend (`Arc::new(RemoteBackend)` or direct `RwkvBackend`). Confirm `futures::executor::block_on()` is used (not `.await` in `update()`). Confirm `CompletionRequest` is created with `system`, `prompt`, `temperature`, `max_tokens`.

**If `handle_chat_action()` doesn't call backend or uses `.await`:** Read `desktop_app.rs` lines 200-350. Confirm `block_on()` usage. If missing, add using pattern from existing code (don't invent new async patterns). If backend is `None` (no model loaded), desktop still creates message but shows error — this is acceptable for migration milestone (stream mechanism verified, not full generation).

---

### Step 2 — Confirm Message Roles Match Web

```bash
grep -n "MessageRole" crates/ui/src/chat.rs
```
Expected: `System`, `User`, `Assistant`, `Event`. Compare to `apps/chat/components/chat.tsx` message roles. Confirm desktop has same or compatible roles.

**If roles don't match (`MessageRole` missing `Assistant` or `System`):** Check `chat.rs` enum definition. Don't rename roles without confirming all usages in `desktop_app.rs` (`handle_chat_action()` uses `MessageRole::Assistant`, `MessageRole::User`, etc.). Adjust `MessageRole` only if all references updated — or document gap (ask user if role naming change needed).

---

### Step 3 — Confirm Markdown Rendering

```bash
grep -n "RichText\|markdown\|format" crates/ui/src/chat.rs | head -n 10
```
Look for: `RichText::new()` or `egui_markdown` usage in message rendering.

**If no markdown rendering:** Confirm basic text display works (`ChatMessage` content shown as `RichText`). For migration milestone, basic display is sufficient. Document `egui_markdown` as future enhancement (see `TASK_10_CONTINUOUS_IMPROVEMENT.md` for feature tracking).

**Don't invent markdown rendering** without user confirmation. Migration milestone: messages display correctly (text visible, roles distinguishable by color/icon if present).

---

### Step 4 — Confirm Message Persistence (Session Save)

In `desktop_app.rs`, check `auto_save()` (line ~850):
```bash
sed -n '840,870p' crates/ui/src/desktop_app.rs
```
Look for: `serde_json::to_string_pretty()` and `std::fs::write()` to `session_path`. Confirm `session_path` is updated when `new_session()` is called. Confirm `load_session()` reads messages back into `ChatWidgetState`.

**If session persistence missing or broken:** Check `ChatWidgetState::new()` and message clearing. Confirm `auto_save()` writes `ConversationState` with `messages` array. If session persistence is only for chat state but not workspace story state (`01-OUTLINE.md`, etc.), document the distinction: session persistence = conversation; workspace persistence = story files (`TASK_03_DESKTOP_E2E.md` covers workspace output verification). Don't mix the two unless user confirms unified persistence needed.

---

### Step 5 — Migration Verification (Manual Check)

Run desktop (`run_desktop.sh`). Perform these actions manually (or document that automation isn't needed — the milestone is capability verification):
1. Click "New Session" (left panel button or `File` menu).
2. Type in chat: `"Write a short fantasy about a knight"`.
3. Confirm message appears as `User` role.
4. Confirm `SendMessage` action triggers generation attempt (even if `MockBackend` or `None` — observe `status_message` update or `ChatMessage::system()` error).
5. Confirm `ChatMessage::assistant()` appears (even empty or error text — mechanism verified).
6. Confirm message history persists through session save/load (`File → Save` then `Open Session` — verify messages remain).

**Milestone 5.1:** Chat mechanism verified manually; streaming or response mechanism confirmed; persistence confirmed; migration complete for chat capabilities.

---

### TROUBLESHOOTING FOR TASK 05

| Migration Issue | Check | Fix / Decision |
|---|---|---|
| Streaming mechanism missing | `desktop_app.rs` `handle_chat_action()` doesn't call backend | Confirm `AppContext` wired (`TASK_02_DESKTOP_INTERACTION.md` 3.1 passed). If `AppContext::generate_stream()` exists but isn't used, wire it. Don't invent backend calls outside `AppContext`. |
| Message roles don't match web | `chat.rs` enum missing `Assistant` | Check `MessageRole` definition. Adjust if safe (all references in `desktop_app.rs` updated). If risky, ask user. |
| Markdown rendering missing | `chat.rs` has no `RichText` formatting | Confirm basic text displays. Document `egui_markdown` as future feature. Don't invent rendering. |
| Session persistence broken | `auto_save()` writes session file but chat messages don't restore on load | Check `load_session()` — does it read `messages` array back into `ChatWidgetState`? Fix `load_session()` if broken (reference `desktop_app.rs` lines 700-850). Don't invent persistence mechanism. |
| Web server feature needed (e.g., `/generate` endpoint) | `api.ts` uses server endpoint | Desktop uses `AppContext` direct backend. Document server feature as plugin-only (`TASK_08_PLUGIN_VERIFY.md`). Don't migrate server-only features to desktop core. |

---

*This task references `AGENTS.md` Sections A, C, D, E.2, G; `EDIT_GUIDE.md`; `TASK_01_DESKTOP_WIDGETS.md`; `TASK_02_DESKTOP_INTERACTION.md`; `TASK_03_DESKTOP_E2E.md` (E2E milestone); `STRATEGIC_PLAN.md` Phase 4.2.*
