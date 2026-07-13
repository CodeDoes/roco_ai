//! Handler registry — routes user intent to the right agent persona + tools.
//!
//! Each `Handler` bundles a system prompt, a set of tools, and a route name
//! so the agent can switch context cleanly between writing, coding, research,
//! search, chat, games, and world-building.

use std::path::PathBuf;

use crate::sandbox::Sandbox;
use crate::tools::ToolRegistry;

/// A named handler with its system prompt and tool set.
pub struct Handler {
    /// Route name (e.g. `"coder"`, `"proseWriter"`).
    pub route: &'static str,
    /// Human-readable purpose description.
    pub purpose: &'static str,
    /// System prompt injected when this handler is active.
    pub system_prompt: &'static str,
    /// The tools this handler is allowed to use.
    pub tools: ToolRegistry,
}

impl Handler {
    pub fn new(
        route: &'static str,
        purpose: &'static str,
        system_prompt: &'static str,
        tools: ToolRegistry,
    ) -> Self {
        Self {
            route,
            purpose,
            system_prompt,
            tools,
        }
    }
}

/// Registry of all handlers, keyed by route name.
pub struct HandlerRegistry {
    handlers: Vec<Handler>,
    /// Fallback handler used when no route matches.
    fallback: usize,
}

impl HandlerRegistry {
    /// Build the standard set of handlers.
    pub fn standard(root: PathBuf, sandbox: Sandbox) -> Self {
        let mut reg = Self {
            handlers: Vec::new(),
            fallback: 0,
        };

        // ── proseWriter ──────────────────────────────────────────────
        reg.add(Handler::new(
            "proseWriter",
            "Creative writing, prose, poetry",
            "You are a skilled creative writer. You help users craft engaging \
             prose, poetry, and narrative text. Focus on style, tone, and \
             emotional impact. Provide constructive suggestions and multiple \
             options when appropriate.",
            crate::builtins::prose_writer_toolkit(),
        ));

        // ── coder ────────────────────────────────────────────────────
        let coder_tools = crate::builtins::standard_toolkit(root.clone(), sandbox.clone());
        reg.add(Handler::new(
            "coder",
            "Code generation, debugging, refactor",
            "You are an expert software engineer. Write clean, idiomatic, \
             well-documented code. Think step by step before implementing. \
             When debugging, analyse the root cause and propose minimal fixes. \
             Always consider edge cases and error handling.",
            coder_tools,
        ));

        // ── research ─────────────────────────────────────────────────
        reg.add(Handler::new(
            "research",
            "Deep synthesis from provided material",
            "You are a thorough research assistant. Synthesise information \
             from the provided material, cite sources, identify patterns, \
             and highlight uncertainties. Stay objective and note when \
             conclusions are speculative.",
            crate::builtins::research_toolkit(),
        ));

        // ── search ───────────────────────────────────────────────────
        reg.add(Handler::new(
            "search",
            "Live web / knowledge lookup",
            "You are a search specialist. Formulate precise queries, \
             evaluate source credibility, and summarise findings \
             impartially. Distinguish facts from opinions.",
            crate::builtins::search_toolkit(),
        ));

        // ── justChatting ─────────────────────────────────────────────
        reg.add(Handler::new(
            "justChatting",
            "Casual conversation, fallback",
            "You are a friendly and helpful conversation partner. Be warm, \
             engaging, and natural. When the user's intent is unclear, ask \
             clarifying questions. This is the default handler when no \
             other route matches.",
            ToolRegistry::new(),
        ));

        // ── adventureGame ────────────────────────────────────────────
        reg.add(Handler::new(
            "adventureGame",
            "Solo text adventure",
            "You are the narrator of a solo text adventure game. Describe \
             the setting, react to player actions, maintain game state, and \
             track inventory. Use dice rolls for outcomes when appropriate. \
             Keep the story immersive and responsive.",
            crate::builtins::adventure_game_toolkit(),
        ));

        // ── trpg ─────────────────────────────────────────────────────
        reg.add(Handler::new(
            "trpg",
            "Tabletop RPG session (GM)",
            "You are the Game Master for a tabletop RPG. Describe scenes, \
             control NPCs, adjudicate rules, and manage dice rolls. Track \
             character sheets and session state. Keep the story engaging and fair.",
            crate::builtins::trpg_toolkit(),
        ));

        // ── random ───────────────────────────────────────────────────
        reg.add(Handler::new(
            "random",
            "Jokes, games, fun distractions",
            "You are a playful entertainer. Tell jokes, propose games, share \
             trivia, and keep the tone light and fun. Read the room — if the \
             user seems serious, offer to switch to a more focused handler.",
            ToolRegistry::new(),
        ));

        // ── worldBuilding ────────────────────────────────────────────
        reg.add(Handler::new(
            "worldBuilding",
            "Constructing consistent fictional worlds",
            "You are a world-building collaborator. Help construct consistent \
             fictional settings — geography, history, culture, magic systems, \
             technology. Track established facts and flag contradictions. Build \
             on the user's ideas while suggesting complementary details.",
            crate::builtins::world_building_toolkit(),
        ));

        // Fallback = justChatting (the last added, index 4)
        reg.fallback = 4;

        reg
    }

    /// Add a handler.
    pub fn add(&mut self, handler: Handler) {
        self.handlers.push(handler);
    }

    /// Look up a handler by route name.
    pub fn get(&self, route: &str) -> Option<&Handler> {
        self.handlers.iter().find(|h| h.route == route)
    }

    /// Select the best handler for a given user message using simple keyword
    /// scoring. Falls back to `justChatting` when no route scores above
    /// the threshold.
    pub fn select(&self, message: &str) -> &Handler {
        let lower = message.to_lowercase();
        let mut best_score: i32 = 0;
        let mut best_idx = self.fallback;

        for (i, h) in self.handlers.iter().enumerate() {
            let score = match h.route {
                "coder" => {
                    let mut s = 0;
                    if lower.contains("code") || lower.contains("function") || lower.contains("bug")
                       || lower.contains("debug") || lower.contains("refactor") || lower.contains("implement")
                       || lower.contains("rust") || lower.contains("python") || lower.contains("javascript")
                       || lower.contains("write a program") || lower.contains("fix") { s += 3; }
                    if lower.contains("file") || lower.contains("read") || lower.contains("write") { s += 1; }
                    s
                }
                "proseWriter" => {
                    let mut s = 0;
                    if lower.contains("write") || lower.contains("story") || lower.contains("poem")
                       || lower.contains("essay") || lower.contains("prose") || lower.contains("creative") { s += 2; }
                    if lower.contains("style") || lower.contains("rewrite") || lower.contains("tone") { s += 1; }
                    s
                }
                "research" => {
                    let mut s = 0;
                    if lower.contains("research") || lower.contains("synthesise") || lower.contains("analyse")
                       || lower.contains("analyse") || lower.contains("compare") || lower.contains("summarise") { s += 2; }
                    if lower.contains("source") || lower.contains("citation") || lower.contains("reference") { s += 1; }
                    s
                }
                "search" => {
                    let mut s = 0;
                    if lower.contains("search") || lower.contains("find") || lower.contains("look up")
                       || lower.contains("google") || lower.contains("what is") || lower.contains("who is") { s += 2; }
                    s
                }
                "adventureGame" => {
                    let mut s = 0;
                    if lower.contains("adventure") || lower.contains("go ") || lower.contains("look")
                       || lower.contains("inventory") || lower.contains("north") || lower.contains("south")
                       || lower.contains("take ") || lower.contains("use ") { s += 2; }
                    s
                }
                "trpg" => {
                    let mut s = 0;
                    if lower.contains("rpg") || lower.contains("roll") || lower.contains("character sheet")
                       || lower.contains("d20") || lower.contains("gm") || lower.contains("campaign")
                       || lower.contains("dungeon") || lower.contains("d&d") { s += 3; }
                    s
                }
                "random" => {
                    let mut s = 0;
                    if lower.contains("joke") || lower.contains("funny") || lower.contains("game")
                       || lower.contains("trivia") || lower.contains("distract") || lower.contains("fun") { s += 2; }
                    s
                }
                "worldBuilding" => {
                    let mut s = 0;
                    if lower.contains("world") || lower.contains("lore") || lower.contains("fictional")
                       || lower.contains("setting") || lower.contains("magic system") || lower.contains("planet")
                       || lower.contains("culture") || lower.contains("history of") { s += 2; }
                    s
                }
                _ => 0,
            };
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }

        &self.handlers[best_idx]
    }

    /// Iterate over all handlers.
    pub fn iter(&self) -> impl Iterator<Item = &Handler> {
        self.handlers.iter()
    }

    /// Number of registered handlers.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_has_all_routes() {
        let root = std::path::PathBuf::from("/tmp");
        let sandbox = crate::sandbox::Sandbox::new();
        let reg = HandlerRegistry::standard(root, sandbox);
        assert_eq!(reg.len(), 9);
        for route in &["coder", "proseWriter", "research", "search", "justChatting",
                        "adventureGame", "trpg", "random", "worldBuilding"] {
            assert!(reg.get(route).is_some(), "missing route: {route}");
        }
    }

    #[test]
    fn select_coder_for_code_question() {
        let root = std::path::PathBuf::from("/tmp");
        let sandbox = crate::sandbox::Sandbox::new();
        let reg = HandlerRegistry::standard(root, sandbox);
        let h = reg.select("Can you help me debug this Rust function?");
        assert_eq!(h.route, "coder");
    }

    #[test]
    fn select_prose_writer_for_story() {
        let root = std::path::PathBuf::from("/tmp");
        let sandbox = crate::sandbox::Sandbox::new();
        let reg = HandlerRegistry::standard(root, sandbox);
        let h = reg.select("Write a short story about a dragon.");
        assert_eq!(h.route, "proseWriter");
    }

    #[test]
    fn select_trpg_for_rpg() {
        let root = std::path::PathBuf::from("/tmp");
        let sandbox = crate::sandbox::Sandbox::new();
        let reg = HandlerRegistry::standard(root, sandbox);
        let h = reg.select("Let's start a D&D campaign.");
        assert_eq!(h.route, "trpg");
    }

    #[test]
    fn select_world_building_for_lore() {
        let root = std::path::PathBuf::from("/tmp");
        let sandbox = crate::sandbox::Sandbox::new();
        let reg = HandlerRegistry::standard(root, sandbox);
        let h = reg.select("Help me define the magic system for my fictional world.");
        assert_eq!(h.route, "worldBuilding");
    }

    #[test]
    fn fallback_to_just_chatting() {
        let root = std::path::PathBuf::from("/tmp");
        let sandbox = crate::sandbox::Sandbox::new();
        let reg = HandlerRegistry::standard(root, sandbox);
        let h = reg.select("How's the weather?");
        assert_eq!(h.route, "justChatting");
    }

    #[test]
    fn coder_has_standard_tools() {
        let root = std::path::PathBuf::from("/tmp");
        let sandbox = crate::sandbox::Sandbox::new();
        let reg = HandlerRegistry::standard(root, sandbox);
        let h = reg.get("coder").unwrap();
        assert!(h.tools.len() > 0, "coder should have standard tools");
    }

    #[test]
    fn prose_writer_has_style_and_rewrite_tools() {
        let reg = HandlerRegistry::standard(
            std::path::PathBuf::from("/tmp"),
            crate::sandbox::Sandbox::new(),
        );
        let h = reg.get("proseWriter").unwrap();
        assert!(h.tools.get("style_guide").is_some(), "proseWriter should have style_guide");
        assert!(h.tools.get("rewrite").is_some(), "proseWriter should have rewrite");
    }

    #[test]
    fn research_has_doc_index_and_citation() {
        let reg = HandlerRegistry::standard(
            std::path::PathBuf::from("/tmp"),
            crate::sandbox::Sandbox::new(),
        );
        let h = reg.get("research").unwrap();
        assert!(h.tools.get("doc_index").is_some());
        assert!(h.tools.get("citation").is_some());
    }

    #[test]
    fn search_has_web_search() {
        let reg = HandlerRegistry::standard(
            std::path::PathBuf::from("/tmp"),
            crate::sandbox::Sandbox::new(),
        );
        let h = reg.get("search").unwrap();
        assert!(h.tools.get("web_search").is_some());
    }

    #[test]
    fn adventure_game_has_state_and_inventory() {
        let reg = HandlerRegistry::standard(
            std::path::PathBuf::from("/tmp"),
            crate::sandbox::Sandbox::new(),
        );
        let h = reg.get("adventureGame").unwrap();
        assert!(h.tools.get("game_state").is_some());
        assert!(h.tools.get("inventory").is_some());
    }

    #[test]
    fn trpg_has_dice_and_character_sheet() {
        let reg = HandlerRegistry::standard(
            std::path::PathBuf::from("/tmp"),
            crate::sandbox::Sandbox::new(),
        );
        let h = reg.get("trpg").unwrap();
        assert!(h.tools.get("dice_roll").is_some());
        assert!(h.tools.get("character_sheet").is_some());
    }

    #[test]
    fn world_building_has_lore_and_consistency() {
        let reg = HandlerRegistry::standard(
            std::path::PathBuf::from("/tmp"),
            crate::sandbox::Sandbox::new(),
        );
        let h = reg.get("worldBuilding").unwrap();
        assert!(h.tools.get("lore_graph").is_some());
        assert!(h.tools.get("consistency_check").is_some());
    }

    #[test]
    fn just_chatting_and_random_have_no_tools() {
        let reg = HandlerRegistry::standard(
            std::path::PathBuf::from("/tmp"),
            crate::sandbox::Sandbox::new(),
        );
        assert_eq!(reg.get("justChatting").unwrap().tools.len(), 0);
        assert_eq!(reg.get("random").unwrap().tools.len(), 0);
    }
}
