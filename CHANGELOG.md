# Changelog

## 2026-07-29 — Real Model End-to-End Working

### Full Pipeline Works with Real Model
- `roco story` now works end-to-end with the real 2.9B RWKV model
- All 6 phases complete: outline → wiki → chapters → validation → synopsis → publish
- 8 workspace files generated: outline, wiki, 3 chapters, validation, synopsis, final story

### Prose Fallback for JSON Parsing
- Added `prose_to_outline()` function that parses the model's prose outline format into `StoryOutline`
- When JSON parsing fails, the system tries the prose fallback before giving up
- The 2.9B model outputs prose instead of JSON; the prose fallback handles this gracefully

### Metadata Extraction Fix
- Fixed `extract_title()`, `extract_genre()`, `extract_tone()` to handle YAML front matter correctly
- Now properly strips `title:`, `genre:`, `tone:` prefixes (case-insensitive)
- Properly trims quotes from values

### What Still Needs Work
- The model outputs prose instead of JSON for most phases — the prose fallback handles this but the structured data quality depends on the model's prose formatting
- Chapter files are short (2-8 lines) — the model's chapter output is truncated
- The `prose_to_outline()` function only handles the outline format; wiki and chapter prose outputs are stored as raw markdown

### What Works
- Mock tests: 817 pass, zero warnings
- Real model end-to-end: outline ✓, wiki ✓, chapters ✓, validation ✓, synopsis ✓, publish ✓
- Resume/phase/fix flags: implemented and tested
- JSON repair infrastructure: handles think blocks, tool tags, extra braces, code fences
- Schema flexibility: `StoryCharacter.setting` accepts both string and object

## 2026-03-24 — Story Pipeline Resumability & Robustness

### Resumability (`roco story --resume`)
- Added `--resume` flag to continue an interrupted story from where it left off
- Added `--phase <name>` to re-run a specific pipeline phase (outline, wiki, chapters, validation, synopsis, publish)
- Added `--fix chapter <num>` to regenerate a single chapter
- Added `--workspace <path>` to target a specific workspace directory
- Workspace detection: finds latest `.roco/workspaces/` directory automatically
- Existing phases (outline, wiki) are loaded from disk, not regenerated
- Resume only regenerates chapters that don't exist yet
- `--phase synopsis` correctly skips all phases before synopsis

### JSON Parsing Robustness
- `clean_json_output` now validates extracted JSON via `serde_json::from_str`
- Progressive suffix stripping: if JSON has extra trailing `}`, removes them one by one until valid
- Code fence extraction validates before returning; falls through to direct extraction if invalid
- `repair_truncated_json` simplified: removed redundant depth recalculation loop
- Model output tags (`<tool_use_code>`, `<tool_use_output>`) stripped before JSON extraction

### Schema Flexibility
- `StoryCharacter.setting`: added `#[serde(default)]` so characters without `setting` field still parse
- `StoryWiki.setting`: uses `string_or_setting_object` deserializer (accepts both string and `{name, description}` object)
- `StoryCharacter`: added `#[allow(dead_code)]` for optional fields the model sometimes outputs

### Compiler Hygiene
- Fixed 5 compiler warnings: unused variables (`start`, `label`, `title`, `wiki_text`, `publish_result`)
- Fixed dead code warning for `StoryCharacter.role` and `setting` fields
- All tests pass (817 total), zero warnings

### Files Modified
- `crates/cli/src/cmd/story.rs` — Resume flags, phase filtering, chapter detection, workspace detection, schema fixes, improved prompts, prose fallback, metadata extraction
- `crates/engine/src/grammar/strategies.rs` — `clean_json_output` validation, `repair_truncated_json` simplification, code fence validation, prose-to-JSON heuristic
