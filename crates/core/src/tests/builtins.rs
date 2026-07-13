//! Tests for `crate::builtins`.
//!
//! Included from `builtins.rs` via `#[path = "tests/builtins.rs"]` so this
//! file is treated as part of the lib crate's `tests` module and runs
//! in the same `cargo test` invocation as the other unit tests.

use std::path::PathBuf;

use crate::builtins::{agent_toolkit, standard_toolkit, BashTool};
use crate::handler_tools::{
    adventure_game_toolkit, prose_writer_toolkit, research_toolkit, search_toolkit, trpg_toolkit,
    world_building_toolkit, CharacterSheetTool, ConsistencyCheckTool, DiceRollTool, LoreGraphTool,
};
use crate::sandbox::Sandbox;
use crate::tools::Tool;

fn temp_root() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("roco-builtins-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn write_then_read_roundtrip() {
    let root = temp_root();
    let reg = standard_toolkit(root.clone(), Sandbox::new());

    let w = reg
        .dispatch(
            "write",
            serde_json::json!({ "path": "note.txt", "content": "hello builtins" }),
        )
        .await
        .unwrap();
    assert_eq!(w["bytes"], 14);

    let r = reg
        .dispatch("read", serde_json::json!({ "path": "note.txt" }))
        .await
        .unwrap();
    assert_eq!(r["content"], "hello builtins");
}

#[tokio::test]
async fn path_escape_is_rejected() {
    let root = temp_root();
    let reg = standard_toolkit(root, Sandbox::new());
    let err = reg
        .dispatch(
            "write",
            serde_json::json!({ "path": "../../escape.txt", "content": "x" }),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, crate::tools::ToolError::Execution { .. }));
}

#[tokio::test]
async fn list_tool_lists_entries() {
    let root = temp_root();
    std::fs::write(root.join("a.txt"), "a").unwrap();
    std::fs::write(root.join("b.txt"), "b").unwrap();
    let reg = standard_toolkit(root, Sandbox::new());
    let out = reg
        .dispatch("list", serde_json::json!({ "path": "." }))
        .await
        .unwrap();
    let names: Vec<&str> = out["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"a.txt"));
    assert!(names.contains(&"b.txt"));
}

#[tokio::test]
async fn bash_tool_runs_through_sandbox() {
    let tool = crate::builtins::BashTool::new(Sandbox::new());
    let out = tool
        .run(serde_json::json!({ "command": "echo builtins-ok" }))
        .await
        .unwrap();
    assert!(out["stdout"].as_str().unwrap().contains("builtins-ok"));
    assert_eq!(out["exit_code"], 0);
}

#[tokio::test]
async fn vector_upsert_then_search_finds_item() {
    use crate::vector::{HashingEmbedder, SharedVectorStore, VectorStore};
    use std::sync::{Arc, Mutex};

    let store: SharedVectorStore = Arc::new(Mutex::new(VectorStore::new(256)));
    let embedder: Arc<dyn crate::vector::Embedder> = Arc::new(HashingEmbedder::new(256));
    let reg = agent_toolkit(temp_root(), Sandbox::new(), store, embedder);

    reg.dispatch(
        "vector_upsert",
        serde_json::json!({ "id": "doc1", "text": "the cat sat on the mat" }),
    )
    .await
    .unwrap();

    let out = reg
        .dispatch(
            "vector_search",
            serde_json::json!({ "query": "cat mat", "k": 3 }),
        )
        .await
        .unwrap();
    let hits = out["hits"].as_array().expect("hits is an array");
    assert!(!hits.is_empty(), "expected at least one hit");
    assert_eq!(hits[0]["id"], "doc1");
}

#[tokio::test]
async fn prose_writer_tools_work() {
    let reg = prose_writer_toolkit();
    let out = reg
        .dispatch(
            "style_guide",
            serde_json::json!({
                "style": "APA", "text": "Some prose."
            }),
        )
        .await
        .unwrap();
    assert!(out["suggestions"].as_str().unwrap().contains("APA"));

    let out = reg
        .dispatch(
            "rewrite",
            serde_json::json!({
                "text": "Original text.", "brief": "make it shorter"
            }),
        )
        .await
        .unwrap();
    assert!(out["brief"].as_str().unwrap().contains("shorter"));
}

#[tokio::test]
async fn research_tools_work() {
    let reg = research_toolkit();
    let out = reg
        .dispatch(
            "doc_index",
            serde_json::json!({
                "id": "doc1", "text": "important content", "source": "https://example.com"
            }),
        )
        .await
        .unwrap();
    assert!(out["indexed"].as_bool().unwrap());

    let out = reg.dispatch("citation", serde_json::json!({
        "style": "APA", "author": "Smith", "title": "Hello", "year": "2026", "publisher": "X"
    })).await.unwrap();
    assert!(out["citation"].as_str().unwrap().contains("Smith"));
}

#[tokio::test]
async fn web_search_tool_works() {
    let reg = search_toolkit();
    let out = reg
        .dispatch(
            "web_search",
            serde_json::json!({
                "query": "rust language"
            }),
        )
        .await
        .unwrap();
    let results = out["results"].as_array().unwrap();
    assert!(!results.is_empty());
}

#[tokio::test]
async fn adventure_game_tools_work() {
    let reg = adventure_game_toolkit();
    reg.dispatch(
        "game_state",
        serde_json::json!({
            "action": "set", "key": "location", "value": "tavern"
        }),
    )
    .await
    .unwrap();
    let out = reg
        .dispatch(
            "game_state",
            serde_json::json!({
                "action": "get", "key": "location"
            }),
        )
        .await
        .unwrap();
    assert_eq!(out["value"], "tavern");

    reg.dispatch(
        "inventory",
        serde_json::json!({
            "action": "add", "item": "sword"
        }),
    )
    .await
    .unwrap();
    let out = reg
        .dispatch(
            "inventory",
            serde_json::json!({
                "action": "list"
            }),
        )
        .await
        .unwrap();
    let items = out["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], "sword");
}

#[tokio::test]
async fn dice_roll_within_range() {
    let tool = DiceRollTool;
    let out = tool
        .run(serde_json::json!({ "notation": "2d6" }))
        .await
        .unwrap();
    let total = out["total"].as_i64().unwrap();
    assert!(total >= 2 && total <= 12, "2d6 should be 2-12, got {total}");
}

#[tokio::test]
async fn character_sheet_round_trip() {
    let tool = CharacterSheetTool::new();
    tool.run(serde_json::json!({
        "action": "create",
        "name": "Aragorn",
        "data": { "class": "ranger", "level": 10 }
    }))
    .await
    .unwrap();
    let out = tool
        .run(serde_json::json!({
            "action": "get", "name": "Aragorn"
        }))
        .await
        .unwrap();
    assert_eq!(out["data"]["class"], "ranger");
    assert_eq!(out["data"]["level"], 10);
}

#[tokio::test]
async fn lore_graph_and_consistency() {
    let graph = LoreGraphTool::new();
    graph
        .run(serde_json::json!({
            "action": "add_entity",
            "entity": "Gandalf",
            "properties": { "race": "Maiar", "color": "grey" }
        }))
        .await
        .unwrap();
    graph
        .run(serde_json::json!({
            "action": "add_relation",
            "source": "Gandalf", "relation": "member_of", "target": "Istari"
        }))
        .await
        .unwrap();
    let out = graph
        .run(serde_json::json!({
            "action": "query_entity", "entity": "Gandalf"
        }))
        .await
        .unwrap();
    assert!(out["exists"].as_bool().unwrap());
    let rels = out["relations"].as_array().unwrap();
    assert_eq!(rels.len(), 1);

    // consistency_check on a fresh tool — no issues because graph is empty.
    let check = ConsistencyCheckTool::new();
    let out = check.run(serde_json::json!({})).await.unwrap();
    assert!(out["consistent"].as_bool().unwrap());
}

#[tokio::test]
async fn trpg_toolkit_has_dice_and_sheet() {
    let reg = trpg_toolkit();
    assert!(reg.get("dice_roll").is_some());
    assert!(reg.get("character_sheet").is_some());
}

#[tokio::test]
async fn world_building_toolkit_has_lore_and_consistency() {
    let reg = world_building_toolkit();
    assert!(reg.get("lore_graph").is_some());
    assert!(reg.get("consistency_check").is_some());
}
