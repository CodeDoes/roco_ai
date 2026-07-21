# TASK 08 — PLUGIN VERIFICATION (VSCode, Zed, Obsidian)

> **Reference:** `AGENTS.md` Sections A, C, E.2, G; `EDIT_GUIDE.md`; `PLUGINS.md` (exists); `TASK_07_WEB_FREEZE.md` (freeze doesn't affect plugins); `STRATEGIC_PLAN.md` Phase 5.1-5.2.
> **Status:** **✅ COMPLETED 2026-07-20** — VSCode: endpoints fixed (`/chapters/:num/generate`, `/chapters/:num/revise`), Check Quality command added. Obsidian: same endpoint fixes + default URL fix. Zed: compiles to wasm. PLUGINS.md: command table corrected, `--story` requirement documented.
> **Rule:** Plugin fixes allowed (`Always` zone for plugin source files — `EDIT_GUIDE.md`). Plugin documentation updates allowed (`PLUGINS.md` editable freely).

---

## PROBLEM (Plugin State)

Plugins (`vscode/`, `zed/`, `obsidian/`) connect to `roco-server` (HTTP API). Desktop (`crates/ui/`) uses direct `RwkvBackend` via `AppContext`. Plugins need to work regardless of whether user runs desktop or server. `TASK_07_WEB_FREEZE.md` freezes web apps but doesn't change plugin architecture (plugins use server, which remains available). This task verifies plugin setup and documents current working state.

---

## WHAT TO READ (Plugin Specific)

- `PLUGINS.md` (exists — read full file — confirms setup steps for VSCode, Zed, Obsidian).
- `API.md` (exists — read server endpoints: `/generate`, `/story/outline`, `/story/chapter`, workspace file read/write, health check).
- `crates/server/src/lib.rs` or `routes.rs` (check server routes match `API.md` documentation — only if `API.md` seems outdated compared to actual server code).
- Plugin source files (`vscode/src/extension.ts`, `zed/src/lib.rs`, `obsidian/main.ts`) — read only; don't edit unless broken.

---

## VERIFICATION PROCEDURE (Step-By-Step Per Plugin)

### VSCode Plugin (`apps/plugins/vscode/`)

**Step 1 — Confirm package dependencies.**
```bash
cat apps/plugins/vscode/package.json
```
Look for: `vscode` engine version, `ai-sdk` or `roco-server` dependency, build/test scripts.

**Step 2 — Confirm command definitions.**
```bash
grep -n "RoCo:" apps/plugins/vscode/src/extension.ts | head -n 20
```
Look for: `Generate Chapter`, `Continue Writing`, `Check Quality`, `Apply Feedback`, `Edit Outline`.

**Step 3 — Confirm server URL configuration.**
```bash
grep -n "localhost\|8080\|roco-server\|ROCO_API_URL" apps/plugins/vscode/src/extension.ts
```
Look for: `http://localhost:8080` or configurable URL. Confirm it matches `API.md` default server port (`8080`).

**Step 4 — Verify extension loads (manual, no code edit unless broken).**
```bash
# In VSCode development environment (if available), or document that plugin loads correctly based on package.json and extension manifest.
cat apps/plugins/vscode/package.json | grep -A 5 '"activationEvents"'
cat apps/plugins/vscode/package.json | grep -A 10 '"commands"'
```
**Milestone:** Commands listed in `PLUGINS.md` match `extension.ts`. Server URL matches `API.md`. No broken references.

**If server URL doesn't match `API.md`:** Edit `extension.ts` to use `API.md` default (`http://localhost:8080`) or make configurable. Don't invent new endpoints — confirm server routes in `crates/server/src/routes.rs` or `story_routes.rs`.

---

### Zed Plugin (`apps/plugins/zed/`)

**Step 1 — Confirm extension manifest.**
```bash
cat apps/plugins/zed/extension.toml
cat apps/plugins/zed/src/lib.rs | head -n 30
```
Look for: `command`, `description`, server connection mechanism (if any).

**Step 2 — Confirm build mechanism.**
```bash
cat apps/plugins/zed/Cargo.toml | head -n 20
```
Look for: `crate-type` (`cdylib` or `rlib`), dependencies matching `crates/inference/` or `roco-server`.

**Step 3 — Verify plugin connects to server or backend.** Check `lib.rs` for HTTP client usage (`reqwest` or `roco_infer_client::RemoteBackend`). Confirm URL matches `API.md` default.

**If plugin doesn't connect correctly:** Check `lib.rs` for `RemoteBackend::new()` usage. Confirm URL parameter. Fix URL to match `API.md`. Don't add new connection mechanisms without user confirmation.

---

### Obsidian Plugin (`apps/plugins/obsidian/`)

**Step 1 — Confirm plugin manifest.**
```bash
cat apps/plugins/obsidian/manifest.json
```
Look for: `id`, `name`, `commands` matching `RoCo:` prefix.

**Step 2 — Confirm TypeScript commands.**
```bash
grep -n "RoCo\|generate\|continue\|check\|feedback" apps/plugins/obsidian/main.ts | head -n 15
```
Look for: command IDs matching `PLUGINS.md` list.

**Step 3 — Confirm server URL or backend access.** Check `main.ts` for `fetch()` to `localhost:8080` or `api/` endpoint. Confirm matches `API.md`.

**If commands or URLs broken:** Fix minimally (update URL, fix command name). Don't redesign plugin architecture.

---

### Cross-Plugin Verification Milestone

**Milestone:** All three plugins (`vscode`, `zed`, `obsidian`) have:
- Correct command names (`PLUGINS.md` matches source).
- Server URL matching `API.md` default (`http://localhost:8080` or configurable).
- No broken references (`extension` loads; `Cargo.toml` builds; `manifest.json` valid).
- `TASK_08_PLUGIN_VERIFY.md` (this file) updated with verification results.

**If plugin requires server endpoint not present in desktop core:** Confirm server endpoint exists in `crates/server/src/routes.rs` or `story_routes.rs`. If endpoint missing but plugin needs it, document gap (ask user — don't invent server routes without confirmation).

---

*This task references `AGENTS.md` Sections A, C, E.2; `EDIT_GUIDE.md`; `PLUGINS.md`; `API.md`; `TASK_07_WEB_FREEZE.md` (freeze doesn't affect plugins); `STRATEGIC_PLAN.md` Phase 5.1-5.2.*
