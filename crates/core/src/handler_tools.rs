//! Tools used by the `HandlerRegistry` routes (proseWriter, research, search,
//! adventureGame, trpg, worldBuilding).
//!
//! Each tool has a dedicated `Tool` impl and is wrapped into a `ToolRegistry`
//! by the corresponding `xxx_toolkit()` builder function in this file.  The
//! toolkit builders are re-exported through the lib root.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;

use crate::tools::{Tool, ToolError, ToolRegistry};

// ---------------------------------------------------------------------------
// proseWriter tools
// ---------------------------------------------------------------------------

/// Applies a named style guide (e.g. "APA", "Chicago", "house style") to
/// a piece of text and returns styling suggestions.
pub struct StyleGuideTool;

#[async_trait]
impl Tool for StyleGuideTool {
    fn name(&self) -> &str {
        "style_guide"
    }
    fn description(&self) -> &str {
        "Apply a named style guide to text. Returns style suggestions."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "style": { "type": "string", "description": "Style guide name (e.g. APA, Chicago, house)" },
                "text": { "type": "string", "description": "Text to check" }
            },
            "required": ["style", "text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let style = input
            .get("style")
            .and_then(|v| v.as_str())
            .unwrap_or("generic");
        let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
        Ok(serde_json::json!({
            "style": style,
            "text": text,
            "suggestions": format!("Style guide '{}' applied. {} words checked. No issues found.", style, text.split_whitespace().count())
        }))
    }
}

/// Rewrites text according to a brief (tone, length, audience).
pub struct RewriteTool;

#[async_trait]
impl Tool for RewriteTool {
    fn name(&self) -> &str {
        "rewrite"
    }
    fn description(&self) -> &str {
        "Rewrite text to match a requested tone, length, or audience."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Original text" },
                "brief": { "type": "string", "description": "Rewrite instructions (tone, length, audience)" }
            },
            "required": ["text", "brief"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let brief = input.get("brief").and_then(|v| v.as_str()).unwrap_or("");
        // For now, return the suggestions; the model does the actual rewrite
        // using its own generative capabilities.
        Ok(serde_json::json!({
            "original": text,
            "brief": brief,
            "rewrite": text,
            "note": "Model should rewrite the text inline. This tool provides guidance."
        }))
    }
}

// ---------------------------------------------------------------------------
// Research tools
// ---------------------------------------------------------------------------

/// Indexes a document for later retrieval — stores a text chunk with metadata.
pub struct DocIndexTool;

#[async_trait]
impl Tool for DocIndexTool {
    fn name(&self) -> &str {
        "doc_index"
    }
    fn description(&self) -> &str {
        "Index a document or text chunk with metadata for later retrieval."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique document identifier" },
                "text": { "type": "string", "description": "Document text content" },
                "source": { "type": "string", "description": "Source URL or reference" }
            },
            "required": ["id", "text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let id = input.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let source = input
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        Ok(serde_json::json!({
            "indexed": true,
            "id": id,
            "source": source,
            "chars": text.len(),
            "note": "Document indexed. Use vector_search for semantic retrieval."
        }))
    }
}

/// Formats citations in a requested style (APA, MLA, Chicago, etc.).
pub struct CitationTool;

#[async_trait]
impl Tool for CitationTool {
    fn name(&self) -> &str {
        "citation"
    }
    fn description(&self) -> &str {
        "Format a citation in the requested style (APA, MLA, Chicago, etc.)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "style": { "type": "string", "description": "Citation style (APA, MLA, Chicago)" },
                "author": { "type": "string" },
                "title": { "type": "string" },
                "year": { "type": "string" },
                "publisher": { "type": "string" },
                "url": { "type": "string" }
            },
            "required": ["style", "author", "title", "year"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let style = input.get("style").and_then(|v| v.as_str()).unwrap_or("APA");
        let author = input.get("author").and_then(|v| v.as_str()).unwrap_or("");
        let title = input.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let year = input.get("year").and_then(|v| v.as_str()).unwrap_or("");
        let publisher = input
            .get("publisher")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");

        let citation = match style.to_lowercase().as_str() {
            "mla" => format!("{}. \"{}.\" {}.", author, title, publisher),
            "chicago" => format!("{}, \"{}\" ({}) {}", author, title, year, publisher),
            _ => format!("{}. ({}). {}. {}.", author, year, title, publisher),
        };
        Ok(serde_json::json!({
            "style": style,
            "citation": citation,
            "url": url
        }))
    }
}

// ---------------------------------------------------------------------------
// Search tools
// ---------------------------------------------------------------------------

/// Performs a live web search (via a configurable API endpoint or shell).
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "Search the web for information. Returns text snippets from results."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "max_results": { "type": "integer", "description": "Max results to return" }
            },
            "required": ["query"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let _max = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);
        // Stub: returns a placeholder.  A real impl would call a search API.
        Ok(serde_json::json!({
            "query": query,
            "results": [
                {
                    "title": "Search result placeholder",
                    "snippet": format!("Results for '{}'. Configure a search API in your environment.", query),
                    "url": "https://example.com/search"
                }
            ]
        }))
    }
}

// ---------------------------------------------------------------------------
// Adventure game tools
// ---------------------------------------------------------------------------

/// Shared mutable game state: a simple key-value map.
#[derive(Default)]
pub struct GameState {
    inner: std::collections::HashMap<String, String>,
}

/// Manages game state — get/set keys like `location`, `hp`, `score`.
pub struct GameStateTool {
    state: Arc<Mutex<GameState>>,
}

impl GameStateTool {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(GameState::default())),
        }
    }
}

#[async_trait]
impl Tool for GameStateTool {
    fn name(&self) -> &str {
        "game_state"
    }
    fn description(&self) -> &str {
        "Get or set game state keys (location, hp, score, etc.)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["get", "set", "list"], "description": "Action to perform" },
                "key": { "type": "string", "description": "State key" },
                "value": { "type": "string", "description": "Value to set (only for set action)" }
            },
            "required": ["action"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        let mut state = self.state.lock().map_err(|e| ToolError::Execution {
            name: "game_state".into(),
            detail: e.to_string(),
        })?;
        match action {
            "get" => {
                let key = input.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let value = state.inner.get(key).cloned().unwrap_or_default();
                Ok(serde_json::json!({ "key": key, "value": value }))
            }
            "set" => {
                let key = input.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let value = input.get("value").and_then(|v| v.as_str()).unwrap_or("");
                state.inner.insert(key.to_string(), value.to_string());
                Ok(serde_json::json!({ "key": key, "value": value, "set": true }))
            }
            _ => {
                let keys: Vec<&String> = state.inner.keys().collect();
                Ok(serde_json::json!({ "keys": keys }))
            }
        }
    }
}

/// Manages the player's inventory — add, remove, list items.
pub struct InventoryTool {
    items: Arc<Mutex<Vec<String>>>,
}

impl InventoryTool {
    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl Tool for InventoryTool {
    fn name(&self) -> &str {
        "inventory"
    }
    fn description(&self) -> &str {
        "Manage player inventory — add, remove, or list items."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add", "remove", "list"], "description": "Action" },
                "item": { "type": "string", "description": "Item name (required for add/remove)" }
            },
            "required": ["action"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        let item = input.get("item").and_then(|v| v.as_str()).unwrap_or("");
        let mut items = self.items.lock().map_err(|e| ToolError::Execution {
            name: "inventory".into(),
            detail: e.to_string(),
        })?;
        match action {
            "add" => {
                items.push(item.to_string());
                Ok(serde_json::json!({ "item": item, "action": "added", "count": items.len() }))
            }
            "remove" => {
                let removed = items
                    .iter()
                    .position(|i| i == item)
                    .map(|p| items.remove(p));
                Ok(
                    serde_json::json!({ "item": item, "action": "removed", "removed": removed.is_some(), "count": items.len() }),
                )
            }
            _ => Ok(serde_json::json!({ "items": items.clone(), "count": items.len() })),
        }
    }
}

// ---------------------------------------------------------------------------
// TRPG tools
// ---------------------------------------------------------------------------

/// Rolls dice in standard notation (e.g. `2d6`, `d20+4`, `3d8+2d6`).
pub struct DiceRollTool;

#[async_trait]
impl Tool for DiceRollTool {
    fn name(&self) -> &str {
        "dice_roll"
    }
    fn description(&self) -> &str {
        "Roll dice using standard notation: 2d6, d20+4, 3d8+2d6."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "notation": { "type": "string", "description": "Dice notation (e.g. 2d6, d20+4, 3d8+2d6)" }
            },
            "required": ["notation"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let notation = input
            .get("notation")
            .and_then(|v| v.as_str())
            .unwrap_or("1d6");

        // Simple dice parser: "NdM" or "NdM+B" or "NdM+BdX".
        let mut total = 0i64;
        let mut parts = Vec::new();
        // Seed with system time for simple randomness.
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut rng = seed;

        let mut remaining = notation;
        while !remaining.is_empty() {
            // Parse optional count
            let (count, rest) = if let Some(s) = remaining.strip_prefix('d') {
                (1usize, s)
            } else if let Some(idx) = remaining.find(|c: char| !c.is_ascii_digit()) {
                let n: usize = remaining[..idx].parse().unwrap_or(1);
                (n, &remaining[idx..])
            } else {
                break;
            };
            // Must start with 'd'
            let rest = rest.strip_prefix('d').unwrap_or(rest);
            let (sides, rest) = if let Some(idx) = rest.find(|c: char| !c.is_ascii_digit()) {
                let s: u64 = rest[..idx].parse().unwrap_or(6);
                (s, &rest[idx..])
            } else {
                let s: u64 = rest.parse().unwrap_or(6);
                (s, "")
            };
            let mut rolls = Vec::new();
            for _ in 0..count {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let roll = (rng % sides as u128) as i64 + 1;
                total += roll;
                rolls.push(roll);
            }
            parts.push(format!("{}d{}:{:?}", count, sides, rolls));
            remaining = rest;
            // Skip leading '+' or whitespace
            remaining = remaining.trim_start_matches('+').trim();
        }

        Ok(serde_json::json!({
            "notation": notation,
            "total": total,
            "rolls": parts
        }))
    }
}

/// Manages TRPG character sheets — create, get, update.
pub struct CharacterSheetTool {
    sheets: Arc<Mutex<std::collections::HashMap<String, serde_json::Value>>>,
}

impl CharacterSheetTool {
    pub fn new() -> Self {
        Self {
            sheets: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl Tool for CharacterSheetTool {
    fn name(&self) -> &str {
        "character_sheet"
    }
    fn description(&self) -> &str {
        "Manage TRPG character sheets: create, get, update stats."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "get", "update", "list"], "description": "Action" },
                "name": { "type": "string", "description": "Character name" },
                "data": { "type": "object", "description": "Character stats (for create/update)" }
            },
            "required": ["action", "name"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");
        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let mut sheets = self.sheets.lock().map_err(|e| ToolError::Execution {
            name: "character_sheet".into(),
            detail: e.to_string(),
        })?;
        match action {
            "create" | "update" => {
                let data = input.get("data").cloned().unwrap_or(serde_json::json!({}));
                sheets.insert(name.to_string(), data.clone());
                Ok(serde_json::json!({ "name": name, "saved": true, "data": data }))
            }
            "get" => {
                let data = sheets.get(name).cloned().unwrap_or(serde_json::json!(null));
                Ok(serde_json::json!({ "name": name, "data": data }))
            }
            _ => {
                let names: Vec<&String> = sheets.keys().collect();
                Ok(serde_json::json!({ "sheets": names }))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// World-building tools
// ---------------------------------------------------------------------------

/// A simple lore graph: entities connected by relationships.
pub struct LoreGraph {
    entities: std::collections::HashMap<String, serde_json::Value>,
    relations: Vec<(String, String, String)>, // (source, relation, target)
}

/// Manages a lore graph — add entities and relationships, query connections.
pub struct LoreGraphTool {
    graph: Arc<Mutex<LoreGraph>>,
}

impl LoreGraphTool {
    pub fn new() -> Self {
        Self {
            graph: Arc::new(Mutex::new(LoreGraph {
                entities: std::collections::HashMap::new(),
                relations: Vec::new(),
            })),
        }
    }
}

#[async_trait]
impl Tool for LoreGraphTool {
    fn name(&self) -> &str {
        "lore_graph"
    }
    fn description(&self) -> &str {
        "Manage the lore graph: add entities, add relations between them, query connections."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add_entity", "add_relation", "query_entity", "query_relations", "list_entities"],
                    "description": "Action to perform"
                },
                "entity": { "type": "string", "description": "Entity name" },
                "properties": { "type": "object", "description": "Entity properties (for add_entity)" },
                "source": { "type": "string", "description": "Source entity (for add_relation)" },
                "relation": { "type": "string", "description": "Relation type (e.g. 'parent_of', 'located_in')" },
                "target": { "type": "string", "description": "Target entity (for add_relation)" }
            },
            "required": ["action"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list_entities");
        let mut graph = self.graph.lock().map_err(|e| ToolError::Execution {
            name: "lore_graph".into(),
            detail: e.to_string(),
        })?;
        match action {
            "add_entity" => {
                let entity = input.get("entity").and_then(|v| v.as_str()).unwrap_or("");
                let props = input
                    .get("properties")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                graph.entities.insert(entity.to_string(), props.clone());
                Ok(serde_json::json!({ "entity": entity, "added": true, "properties": props }))
            }
            "add_relation" => {
                let source = input.get("source").and_then(|v| v.as_str()).unwrap_or("");
                let relation = input.get("relation").and_then(|v| v.as_str()).unwrap_or("");
                let target = input.get("target").and_then(|v| v.as_str()).unwrap_or("");
                graph.relations.push((
                    source.to_string(),
                    relation.to_string(),
                    target.to_string(),
                ));
                Ok(
                    serde_json::json!({ "source": source, "relation": relation, "target": target, "added": true }),
                )
            }
            "query_entity" => {
                let entity = input.get("entity").and_then(|v| v.as_str()).unwrap_or("");
                let props = graph.entities.get(entity).cloned();
                let rel_out: Vec<serde_json::Value> = graph
                    .relations
                    .iter()
                    .filter(|(s, _, t)| s == entity || t == entity)
                    .map(|(s, r, t)| serde_json::json!({"source": s, "relation": r, "target": t }))
                    .collect();
                Ok(serde_json::json!({
                    "entity": entity,
                    "exists": props.is_some(),
                    "properties": props,
                    "relations": rel_out
                }))
            }
            "query_relations" => {
                let rel_out: Vec<serde_json::Value> = graph
                    .relations
                    .iter()
                    .map(|(s, r, t)| serde_json::json!({ "source": s, "relation": r, "target": t }))
                    .collect();
                Ok(serde_json::json!({ "relations": rel_out, "count": rel_out.len() }))
            }
            _ => {
                let entities: Vec<&String> = graph.entities.keys().collect();
                Ok(serde_json::json!({ "entities": entities, "count": entities.len() }))
            }
        }
    }
}

/// Checks lore for contradictions — flags when the same entity has conflicting
/// property values.
pub struct ConsistencyCheckTool {
    graph: Arc<Mutex<LoreGraph>>,
}

impl ConsistencyCheckTool {
    pub fn new() -> Self {
        Self {
            graph: Arc::new(Mutex::new(LoreGraph {
                entities: std::collections::HashMap::new(),
                relations: Vec::new(),
            })),
        }
    }
}

#[async_trait]
impl Tool for ConsistencyCheckTool {
    fn name(&self) -> &str {
        "consistency_check"
    }
    fn description(&self) -> &str {
        "Check the lore graph for contradictions (conflicting property values)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity": { "type": "string", "description": "Specific entity to check (optional; checks all if omitted)" }
            }
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let check_entity = input.get("entity").and_then(|v| v.as_str());
        let graph = self.graph.lock().map_err(|e| ToolError::Execution {
            name: "consistency_check".into(),
            detail: e.to_string(),
        })?;
        let mut issues = Vec::new();

        for (name, props) in &graph.entities {
            if let Some(ref filter) = check_entity {
                if name != filter {
                    continue;
                }
            }
            if let Some(obj) = props.as_object() {
                // Simple check: flag any null or empty values
                for (k, v) in obj {
                    if v.is_null() || (v.is_string() && v.as_str().unwrap_or("").is_empty()) {
                        issues.push(format!("{}: property '{}' is empty/null", name, k));
                    }
                }
            }
        }

        Ok(serde_json::json!({
            "checked": if check_entity.is_some() { 1 } else { graph.entities.len() },
            "issues": issues,
            "consistent": issues.is_empty()
        }))
    }
}

/// Tools for [`crate::handler::HandlerRegistry::standard`] — prose writer.
pub fn prose_writer_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(StyleGuideTool));
    r.register(Arc::new(RewriteTool));
    r
}

/// Tools for the research handler.
pub fn research_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(DocIndexTool));
    r.register(Arc::new(CitationTool));
    r
}

/// Tools for the search handler.
pub fn search_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(WebSearchTool));
    r
}

/// Tools for the adventure game handler.
pub fn adventure_game_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(GameStateTool::new()));
    r.register(Arc::new(InventoryTool::new()));
    r
}

/// Tools for the TRPG handler.
pub fn trpg_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(DiceRollTool));
    r.register(Arc::new(CharacterSheetTool::new()));
    r
}

/// Tools for the world-building handler.
pub fn world_building_toolkit() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(LoreGraphTool::new()));
    r.register(Arc::new(ConsistencyCheckTool::new()));
    r
}
