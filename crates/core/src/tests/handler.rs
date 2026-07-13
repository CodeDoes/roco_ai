//! Tests for `crate::handler`.
//!
//! Included from `handler.rs` via `#[path = "tests/handler.rs"]` so this
//! file is treated as part of the lib crate's `tests` module and runs
//! in the same `cargo test` invocation as the other unit tests.

use crate::handler::HandlerRegistry;

fn reg() -> HandlerRegistry {
    HandlerRegistry::standard(
        std::path::PathBuf::from("/tmp"),
        crate::sandbox::Sandbox::new(),
    )
}

#[test]
fn standard_has_all_routes() {
    let reg = reg();
    assert_eq!(reg.len(), 9);
    for route in &[
        "coder",
        "proseWriter",
        "research",
        "search",
        "justChatting",
        "adventureGame",
        "trpg",
        "random",
        "worldBuilding",
    ] {
        assert!(reg.get(route).is_some(), "missing route: {route}");
    }
}

#[test]
fn select_coder_for_code_question() {
    let reg = reg();
    let h = reg.select("Can you help me debug this Rust function?");
    assert_eq!(h.route, "coder");
}

#[test]
fn select_prose_writer_for_story() {
    let reg = reg();
    let h = reg.select("Write a short story about a dragon.");
    assert_eq!(h.route, "proseWriter");
}

#[test]
fn select_trpg_for_rpg() {
    let reg = reg();
    let h = reg.select("Let's start a D&D campaign.");
    assert_eq!(h.route, "trpg");
}

#[test]
fn select_world_building_for_lore() {
    let reg = reg();
    let h = reg.select("Help me define the magic system for my fictional world.");
    assert_eq!(h.route, "worldBuilding");
}

#[test]
fn fallback_to_just_chatting() {
    let reg = reg();
    let h = reg.select("How's the weather?");
    assert_eq!(h.route, "justChatting");
}

#[test]
fn coder_has_standard_tools() {
    let reg = reg();
    let h = reg.get("coder").unwrap();
    assert!(h.tools.len() > 0, "coder should have standard tools");
}

#[test]
fn prose_writer_has_style_and_rewrite_tools() {
    let reg = reg();
    let h = reg.get("proseWriter").unwrap();
    assert!(
        h.tools.get("style_guide").is_some(),
        "proseWriter should have style_guide"
    );
    assert!(
        h.tools.get("rewrite").is_some(),
        "proseWriter should have rewrite"
    );
}

#[test]
fn research_has_doc_index_and_citation() {
    let reg = reg();
    let h = reg.get("research").unwrap();
    assert!(h.tools.get("doc_index").is_some());
    assert!(h.tools.get("citation").is_some());
}

#[test]
fn search_has_web_search() {
    let reg = reg();
    let h = reg.get("search").unwrap();
    assert!(h.tools.get("web_search").is_some());
}

#[test]
fn adventure_game_has_state_and_inventory() {
    let reg = reg();
    let h = reg.get("adventureGame").unwrap();
    assert!(h.tools.get("game_state").is_some());
    assert!(h.tools.get("inventory").is_some());
}

#[test]
fn trpg_has_dice_and_character_sheet() {
    let reg = reg();
    let h = reg.get("trpg").unwrap();
    assert!(h.tools.get("dice_roll").is_some());
    assert!(h.tools.get("character_sheet").is_some());
}

#[test]
fn world_building_has_lore_and_consistency() {
    let reg = reg();
    let h = reg.get("worldBuilding").unwrap();
    assert!(h.tools.get("lore_graph").is_some());
    assert!(h.tools.get("consistency_check").is_some());
}

#[test]
fn just_chatting_and_random_have_no_tools() {
    let reg = reg();
    assert_eq!(reg.get("justChatting").unwrap().tools.len(), 0);
    assert_eq!(reg.get("random").unwrap().tools.len(), 0);
}
