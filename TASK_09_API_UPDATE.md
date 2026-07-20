# TASK 09 — API DOCUMENTATION UPDATE (`API.md`)

> **Reference:** `AGENTS.md` Sections C, D, F, G; `EDIT_GUIDE.md`; `API.md` (exists — edit freely); `TASK_08_PLUGIN_VERIFY.md` (plugin verification may reveal endpoint gaps); `TASK_04_EDITOR_MIGRATION.md` (desktop uses direct backend, not server, for core — document this distinction).
> **Status:** Update only — no new server endpoints unless plugin verification (`TASK_08`) or desktop integration (`TASK_02`) revealed missing endpoints. Confirm before adding.

---

## PROBLEM (When This Needs Updates)

`API.md` documents server endpoints (`GET /health`, `POST /generate`, `POST /story/outline`, `POST /story/chapter`, workspace file endpoints). If desktop migration changes how clients connect (direct backend vs server), or if plugin verification reveals endpoint mismatches, `API.md` must reflect reality.

---

## PREREQUISITE CHECK

```bash
# Confirm server routes match documented endpoints
cat crates/server/src/routes.rs 2>/dev/null || echo "routes.rs not at expected path"
cat crates/server/src/lib.rs 2>/dev/null || echo "lib.rs not at expected path"
# Check story routes
cat crates/server/src/story_routes.rs 2>/dev/null || echo "story_routes.rs missing"
```
**If server source files moved or renamed:** Update `API.md` file references in `PROJECT_STRUCTURE.md` (if needed) and document new paths.

---

## WHAT TO READ

- `API.md` (full — confirm all endpoint descriptions match `routes.rs` / `story_routes.rs`).
- `crates/server/src/lib.rs` or `routes.rs` (confirm actual endpoint paths, request/response formats, error codes).
- `crates/server/src/story_routes.rs` (confirm story-specific endpoints: `/story/outline`, `/story/chapter`, workspace file endpoints).
- `TASK_08_PLUGIN_VERIFY.md` (check if plugin URL references match `API.md` — if plugins reference `localhost:8080` and `API.md` says `localhost:8080`, consistent; if plugin uses different port or endpoint not documented, document or fix plugin, not invent API).

---

## VERIFICATION PROCEDURE (Confirm Before Editing `API.md`)

### Step 1 — Confirm Endpoint Existence

For each endpoint in `API.md`:
```bash
grep -n "GET /health\|POST /generate\|POST /story/outline\|POST /story/chapter\|GET /workspace" crates/server/src/*.rs
```
**If endpoint documented in `API.md` but not in server source:** Document gap. Don't invent endpoint documentation. Note in `API.md`: `Endpoint documented but not implemented in server source (check crates/server/src/routes.rs, story_routes.rs). Confirm with user before adding.`

**If endpoint exists in source but not documented:** Add documentation (only confirmed endpoints).

---

### Step 2 — Confirm Request/Response Formats Match Source

For each endpoint, read server source to confirm JSON request/response structures. Example:
```bash
# Confirm generate endpoint request format
sed -n '1,80p' crates/server/src/routes.rs
```
Look for: `CompletionRequest` usage, `system`, `prompt`, `grammar`, `temperature`, `max_tokens`. Confirm these match `API.md` request body documentation.

**If request/response format changed in source (e.g., new field added):** Update `API.md` request/response JSON examples. Don't invent fields — only document what's in `routes.rs`.

---

### Step 3 — Confirm Authentication Note Remains Accurate

`API.md` currently says: `Currently no authentication — intended for local use only.` Confirm this is still true (`routes.rs` has no auth middleware). If server added auth (`middleware/`), update `API.md`. If no auth middleware exists (`find crates/server/src/ -name "*auth*" -o -name "*middleware*"` returns nothing), keep note.

---

### Step 4 — Confirm Desktop Backend Distinction Documented

Add or confirm note in `API.md`: desktop (`crates/ui/src/desktop_app.rs`) uses direct `RwkvBackend` (`Arc<dyn ModelBackend>`) through `AppContext`, not `roco-server` HTTP. `roco-server` remains for plugins and external integrations (`TASK_08_PLUGIN_VERIFY.md`). This prevents plugin users from assuming desktop requires server.

---

## EDIT RULES (`API.md` Only — No Server Source Changes Unless Confirmed)

- Only document endpoints confirmed in `routes.rs`, `story_routes.rs`.
- Only document request/response formats confirmed in server source.
- Don't invent new endpoints (`/agent/chat`, `/story/revise`) unless server source confirms them.
- Don't change server behavior based on `API.md` — `API.md` reflects reality; server source defines reality.
- Update `API.md` file references (`routes.rs`, `lib.rs`) only if file paths changed (`TASK_07_WEB_FREEZE.md` doesn't affect server paths — confirm with `find crates/server/`).

---

### TROUBLESHOOTING FOR TASK 09

| Issue | Check | Fix / Decision |
|---|---|---|
| `API.md` endpoint not in server source | `grep -rn "/generate\|/health\|/workspace" crates/server/src/` | Don't invent documentation. Note gap. Ask user if endpoint should be added to server (requires `routes.rs` edit — `Always` zone for server? `EDIT_GUIDE.md` shows `crates/server/` not listed as frozen — check `Cargo.toml` workspace; server is editable but requires caution). Confirm before editing server. |
| Plugin references endpoint not documented | `grep -n "localhost\|8080" apps/plugins/*/src/*.ts apps/plugins/*/src/*.rs` | Document endpoint in `API.md` if server confirms it. If plugin uses endpoint that doesn't exist, fix plugin to match `API.md` or document plugin limitation. Don't invent server endpoint. |
| Request/response format changed but `API.md` not updated | Compare `routes.rs` `CompletionRequest` fields to `API.md` JSON | Update `API.md` JSON to match source. Don't change source to match outdated docs. |

---

*This task references `AGENTS.md` Sections D, F, G; `EDIT_GUIDE.md`; `TASK_08_PLUGIN_VERIFY.md` (plugin endpoint verification); `API.md` (file being updated); `PLUGINS.md`; `PROJECT_STRUCTURE.md`. No source file changes required unless endpoint gap confirmed by user.*
