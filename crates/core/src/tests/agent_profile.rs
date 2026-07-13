//! Tests for `crate::agent_profile`.
//!
//! Included from `agent_profile.rs` via `#[path = "tests/agent_profile.rs"]` so
//! this file is treated as part of the lib crate's `tests` module and runs
//! in the same `cargo test` invocation as the other unit tests.

use super::presets::*;
use super::*;

#[test]
fn test_profile_creation() {
    let p = storyteller_rwkv();
    assert_eq!(p.id, "storyteller/fast");
    assert_eq!(p.role, AgentRole::Worker);
    assert_eq!(p.model_ref, "rwkv-2.9b");
    assert!(p.capabilities.contains(&"creative".to_string()));
}

#[test]
fn test_coder_profile() {
    let p = coder_fast();
    assert_eq!(p.id, "coder/fast");
    assert!(p.capabilities.contains(&"code".to_string()));
    assert!(p.weaknesses.contains(&"creative".to_string()));
}

#[test]
fn test_build_system_prompt_without_examples() {
    let p = assistant_fast();
    let prompt = p.build_system_prompt();
    assert!(prompt.contains("helpful assistant"));
    assert!(!prompt.contains("--- Examples ---"));
}

#[test]
fn test_build_system_prompt_with_examples() {
    let p = coder_fast().with_few_shot(vec![FewShotExample {
        input: "Write a function that adds two numbers".into(),
        output: "fn add(a: i32, b: i32) -> i32 { a + b }".into(),
        reasoning: None,
    }]);
    let prompt = p.build_system_prompt();
    assert!(prompt.contains("--- Examples ---"));
    assert!(prompt.contains("fn add"));
}

#[test]
fn test_build_request_uses_strategy_settings() {
    let p = storyteller_rwkv();
    let req = p.build_request("Write a story", None);
    assert!((req.temperature - 0.6).abs() < 0.01);
    assert_eq!(req.max_tokens, 1024);

    let p2 = coder_fast();
    let req2 = p2.build_request("Write a function", None);
    assert!((req2.temperature - 0.1).abs() < 0.01);
    assert_eq!(req2.max_tokens, 512);
}

#[test]
fn test_registry_basic_ops() {
    let mut reg = AgentProfileRegistry::new();
    assert_eq!(reg.profile_count(), 0);

    reg.register(storyteller_rwkv());
    reg.register(coder_fast());
    assert_eq!(reg.profile_count(), 2);

    assert!(reg.get("storyteller/fast").is_some());
    assert!(reg.get("coder/fast").is_some());
    assert!(reg.get("nonexistent").is_none());

    let removed = reg.unregister("coder/fast");
    assert!(removed.is_some());
    assert_eq!(reg.profile_count(), 1);
}

#[test]
fn test_registry_role_filter() {
    let mut reg = AgentProfileRegistry::new();
    reg.register(storyteller_rwkv()); // Worker
    reg.register(coder_fast()); // Worker
    reg.register(orchestrator_cpu()); // Orchestrator
    reg.register(assistant_fast()); // Worker
    reg.register(theorist()); // Critic

    let workers = reg.all_profiles_for_role(AgentRole::Worker);
    assert_eq!(workers.len(), 3);

    let orchestrators = reg.all_profiles_for_role(AgentRole::Orchestrator);
    assert_eq!(orchestrators.len(), 1);

    let critics = reg.all_profiles_for_role(AgentRole::Critic);
    assert_eq!(critics.len(), 1);
}

#[test]
fn test_registry_groups() {
    let mut reg = AgentProfileRegistry::new();
    for p in presets::all_presets() {
        reg.register(p);
    }

    for g in presets::default_groups() {
        reg.register_group(g);
    }

    assert_eq!(reg.all_groups().len(), 5);

    let writing = reg.get_group("writing").unwrap();
    assert_eq!(writing.profile_ids, vec!["storyteller/fast"]);
}

#[test]
fn test_select_from_group_by_capability() {
    let mut reg = AgentProfileRegistry::new();
    reg.register(coder_fast());
    reg.register(coder_review_tiny());
    reg.register(storyteller_rwkv());

    reg.register_group(
        AgentGroup::new("all", "All profiles")
            .with_profiles(vec!["coder/fast", "coder/review", "storyteller/fast"])
            .with_routing(RoutingStrategy::ByCapability),
    );

    let code_profiles = reg.select_from_group("all", Some("code"));
    assert_eq!(code_profiles.len(), 1);
    assert_eq!(code_profiles[0].id, "coder/fast");

    let review_profiles = reg.select_from_group("all", Some("code-review"));
    assert_eq!(review_profiles.len(), 1);
    assert_eq!(review_profiles[0].id, "coder/review");
}

#[test]
fn test_memory_entry() {
    let entry = MemoryEntry::new("user", "favorite_color", "blue", 0.8);
    assert_eq!(entry.key, "favorite_color");
    assert_eq!(entry.value, "blue");
    assert_eq!(entry.namespace, "user");
    assert!(entry.timestamp_ms > 0);
}

#[test]
fn test_serialization_roundtrip() {
    let p = storyteller_rwkv();
    let json = serde_json::to_string_pretty(&p).unwrap();
    let deserialized: AgentProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(p.id, deserialized.id);
    assert_eq!(p.name, deserialized.name);
    assert_eq!(p.role, deserialized.role);
    assert_eq!(p.model_ref, deserialized.model_ref);
    assert_eq!(p.capabilities, deserialized.capabilities);
}

#[test]
fn test_save_load_roundtrip() {
    let p = coder_fast();
    let path = std::env::temp_dir().join("test_agent_profile.json");
    p.save_to_file(&path).unwrap();
    let loaded = AgentProfile::load_from_file(&path).unwrap();
    assert_eq!(p.id, loaded.id);
    assert_eq!(p.name, loaded.name);
    assert_eq!(p.model_ref, loaded.model_ref);
    std::fs::remove_file(path).ok();
}
