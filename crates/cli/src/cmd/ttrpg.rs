//! TTRPG Game, World Building, and Character Chat System.
//!
//! Implements a rich terminal-based campaign manager and interactive game
//! master, with dynamic NLU-driven triggers and checks checked on each turn.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::daemon;
use crate::rich_output as r;

// ═══════════════════════════════════════════════════════════════════════════
// TTRPG Data Models
// ═══════════════════════════════════════════════════════════════════════════

/// A character sheet for players or NPCs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtrpgCharacter {
    pub name: String,
    pub class_or_role: String,
    pub level: u32,
    pub hp: i32,
    pub max_hp: i32,
    pub attributes: HashMap<String, i32>, // Strength, Dexterity, Constitution, Intelligence, Wisdom, Charisma
    pub inventory: Vec<String>,
    pub skills: Vec<String>,
}

impl TtrpgCharacter {
    /// Calculate the D&D-style modifier for an attribute value.
    pub fn get_modifier(&self, attr: &str) -> i32 {
        let val = self.attributes.get(attr).copied().unwrap_or(10);
        (val - 10) / 2
    }
}

/// A world location.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtrpgLocation {
    pub name: String,
    pub description: String,
    pub exits: HashMap<String, String>, // Direction -> Destination Location Name
}

/// A faction in the world.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtrpgFaction {
    pub name: String,
    pub description: String,
    pub disposition: String, // e.g. "Friendly", "Neutral", "Suspicious", "Hostile"
}

/// Lore artifact, historical item, or region record.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtrpgLore {
    pub name: String,
    pub description: String,
}

/// A natural language trigger checked by the NLU on each turn.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtrpgTrigger {
    pub trigger_text: String,
    pub is_active: bool,
}

/// The entire world building structure.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtrpgWorld {
    pub name: String,
    pub description: String,
    pub locations: Vec<TtrpgLocation>,
    pub factions: Vec<TtrpgFaction>,
    pub lore: Vec<TtrpgLore>,
    pub npcs: Vec<TtrpgCharacter>,
}

/// The complete TTRPG and campaign state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TtrpgState {
    pub player: TtrpgCharacter,
    pub world: TtrpgWorld,
    pub triggers: Vec<TtrpgTrigger>,
    pub current_location: String,
    pub turn_count: u32,
    pub history: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// State Persistence & Initialization
// ═══════════════════════════════════════════════════════════════════════════

/// Get the path where TTRPG game state is saved.
fn get_state_path() -> PathBuf {
    let base_dir = if let Ok(dir) = std::env::var("ROCO_DIR") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(".roco")
    };
    base_dir.join("ttrpg_state.json")
}

impl TtrpgState {
    /// Save the current campaign state to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = get_state_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let serialized =
            serde_json::to_string_pretty(self).map_err(|e| format!("Serialization error: {e}"))?;
        std::fs::write(&path, serialized)
            .map_err(|e| format!("Failed to write state to file {}: {e}", path.display()))?;
        Ok(())
    }

    /// Load state from disk, or return a rich default campaign state if missing/corrupt.
    pub fn load_or_default() -> Self {
        let path = get_state_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(state) = serde_json::from_str::<TtrpgState>(&content) {
                    return state;
                }
            }
        }
        Self::create_default()
    }

    /// Create a beautifully realized, high-quality starting campaign setting.
    fn create_default() -> Self {
        let mut attributes = HashMap::new();
        attributes.insert("Strength".to_string(), 10);
        attributes.insert("Dexterity".to_string(), 16);
        attributes.insert("Constitution".to_string(), 12);
        attributes.insert("Intelligence".to_string(), 14);
        attributes.insert("Wisdom".to_string(), 13);
        attributes.insert("Charisma".to_string(), 15);

        let player = TtrpgCharacter {
            name: "Elara".to_string(),
            class_or_role: "Rogue".to_string(),
            level: 1,
            hp: 12,
            max_hp: 12,
            attributes,
            inventory: vec![
                "Thieves' Tools".to_string(),
                "Steel Dagger".to_string(),
                "Leather Armor".to_string(),
                "Rope (50ft)".to_string(),
            ],
            skills: vec![
                "Acrobatics".to_string(),
                "Stealth".to_string(),
                "Sleight of Hand".to_string(),
                "Deception".to_string(),
            ],
        };

        let loc1 = TtrpgLocation {
            name: "The Whispering Tavern".to_string(),
            description: "A dimly-lit tavern filled with smoke, hushed conversations, and the smell of roasted mutton. Barnd the Barkeep cleans glasses behind the counter.".to_string(),
            exits: [
                ("north".to_string(), "The Dark Crypt".to_string()),
                ("east".to_string(), "The City Gates".to_string()),
            ].into_iter().collect(),
        };

        let loc2 = TtrpgLocation {
            name: "The Dark Crypt".to_string(),
            description: "An ancient tomb where shadows pool in corners. Stone sarcophagi are lined along the walls, covered in thick dust and cobwebs. It is pitch black.".to_string(),
            exits: [
                ("south".to_string(), "The Whispering Tavern".to_string()),
            ].into_iter().collect(),
        };

        let loc3 = TtrpgLocation {
            name: "The City Gates".to_string(),
            description: "Towering stone arches guarded by City Watch soldiers. Beyond lies the untamed wilderness. Grum the Orc Guard stands watch here.".to_string(),
            exits: [
                ("west".to_string(), "The Whispering Tavern".to_string()),
            ].into_iter().collect(),
        };

        let mut barnd_attrs = HashMap::new();
        barnd_attrs.insert("Strength".to_string(), 14);
        barnd_attrs.insert("Dexterity".to_string(), 10);
        barnd_attrs.insert("Constitution".to_string(), 15);
        barnd_attrs.insert("Intelligence".to_string(), 11);
        barnd_attrs.insert("Wisdom".to_string(), 12);
        barnd_attrs.insert("Charisma".to_string(), 13);

        let barnd = TtrpgCharacter {
            name: "Barnd the Barkeep".to_string(),
            class_or_role: "Barkeep & Informant".to_string(),
            level: 3,
            hp: 24,
            max_hp: 24,
            attributes: barnd_attrs,
            inventory: vec![
                "Copper Tankard".to_string(),
                "Heavy Wood Club".to_string(),
                "Tavern Keys".to_string(),
            ],
            skills: vec!["Insight".to_string(), "History".to_string()],
        };

        let mut grum_attrs = HashMap::new();
        grum_attrs.insert("Strength".to_string(), 16);
        grum_attrs.insert("Dexterity".to_string(), 12);
        grum_attrs.insert("Constitution".to_string(), 15);
        grum_attrs.insert("Intelligence".to_string(), 8);
        grum_attrs.insert("Wisdom".to_string(), 9);
        grum_attrs.insert("Charisma".to_string(), 8);

        let grum = TtrpgCharacter {
            name: "Grum the Orc Guard".to_string(),
            class_or_role: "City Watch Sentry".to_string(),
            level: 4,
            hp: 38,
            max_hp: 38,
            attributes: grum_attrs,
            inventory: vec![
                "Iron Halberd".to_string(),
                "Chainmail Armor".to_string(),
                "Watch Horn".to_string(),
            ],
            skills: vec!["Athletics".to_string(), "Intimidation".to_string()],
        };

        let faction1 = TtrpgFaction {
            name: "The Guild of Whispers".to_string(),
            description: "A secret underground organization of thieves, spies, and shadows. They value secrecy and profit.".to_string(),
            disposition: "Friendly".to_string(),
        };

        let faction2 = TtrpgFaction {
            name: "The City Watch".to_string(),
            description: "The law enforcement arm of the city council. They patrol the streets, keep peace, and despise troublemakers.".to_string(),
            disposition: "Neutral".to_string(),
        };

        let lore1 = TtrpgLore {
            name: "The Crimson Ring".to_string(),
            description: "A cursed artifact rumored to grant power over blood magic, stolen from the high archmage years ago.".to_string(),
        };

        let trigger1 = TtrpgTrigger {
            trigger_text: "If the player enters The Dark Crypt without a light source (like a torch or lantern) in their inventory, they must make a Wisdom check DC 14 or panic in the dark.".to_string(),
            is_active: true,
        };

        let trigger2 = TtrpgTrigger {
            trigger_text: "If the player mentions the Crimson Ring to Barnd, he becomes extremely suspicious and alters his disposition to Suspicious.".to_string(),
            is_active: true,
        };

        let trigger3 = TtrpgTrigger {
            trigger_text: "If the player attacks any guard or breaks the law near the City Gates, Grum the Orc Guard immediately attacks and enters combat.".to_string(),
            is_active: true,
        };

        let world = TtrpgWorld {
            name: "The Shadowed Kingdom".to_string(),
            description: "A fantasy realm where darkness encroaches, shadows harbor secrets, and power-hungry guilds clash in silence.".to_string(),
            locations: vec![loc1, loc2, loc3],
            factions: vec![faction1, faction2],
            lore: vec![lore1],
            npcs: vec![barnd, grum],
        };

        TtrpgState {
            player,
            world,
            triggers: vec![trigger1, trigger2, trigger3],
            current_location: "The Whispering Tavern".to_string(),
            turn_count: 0,
            history: vec![],
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Unit Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_modifier_math() {
        let mut attributes = HashMap::new();
        attributes.insert("Strength".to_string(), 15); // +2 modifier
        attributes.insert("Dexterity".to_string(), 8); // -1 modifier
        attributes.insert("Constitution".to_string(), 10); // 0 modifier

        let character = TtrpgCharacter {
            name: "TestHero".to_string(),
            class_or_role: "Warrior".to_string(),
            level: 1,
            hp: 10,
            max_hp: 10,
            attributes,
            inventory: vec![],
            skills: vec![],
        };

        assert_eq!(character.get_modifier("Strength"), 2);
        assert_eq!(character.get_modifier("Dexterity"), -1);
        assert_eq!(character.get_modifier("Constitution"), 0);
        // Default when missing
        assert_eq!(character.get_modifier("Charisma"), 0);
    }

    #[test]
    fn test_roll_d20_bounds() {
        for _ in 0..100 {
            let roll = roll_d20();
            assert!((1..=20).contains(&roll), "Roll was: {}", roll);
        }
    }

    #[test]
    fn test_create_default_state() {
        let state = TtrpgState::create_default();
        assert_eq!(state.player.name, "Elara");
        assert_eq!(state.current_location, "The Whispering Tavern");
        assert_eq!(state.world.locations.len(), 3);
        assert_eq!(state.triggers.len(), 3);
        assert!(state.triggers[0].is_active);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Custom Pseudo-Random Roll Engine
// ═══════════════════════════════════════════════════════════════════════════

/// Generates a pseudo-random number from 1 to 20 without external dependencies.
pub fn roll_d20() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let lcg = (nanos ^ (nanos >> 13)) % 20 + 1;
    lcg as i32
}

// ═══════════════════════════════════════════════════════════════════════════
// NLU Integration and Trigger/Check Evaluation
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize, Debug)]
struct NluTriggerResponse {
    trigger_fired: bool,
    fired_trigger_index: Option<usize>,
    consequence: String,
}

#[derive(Deserialize, Debug)]
struct NluActionResponse {
    requires_check: bool,
    attribute: Option<String>,
    dc: Option<i32>,
    consequence_success: String,
    consequence_failure: String,
    simple_outcome: String,
}

/// Use NLU to evaluate if any active natural language trigger is fired by the player's action.
fn evaluate_nlu_triggers(
    backend: &dyn roco_engine::ModelBackend,
    state: &TtrpgState,
    action: &str,
) -> Option<(usize, String)> {
    let mut triggers_text = String::new();
    for (i, t) in state.triggers.iter().enumerate() {
        if t.is_active {
            triggers_text.push_str(&format!("{}. {}\n", i, t.trigger_text));
        }
    }
    if triggers_text.is_empty() {
        return None;
    }

    let prompt = format!(
        "You are the NLU trigger evaluation engine for a TTRPG game.\n\n\
         CURRENT STATE:\n\
         Location: {}\n\
         Turn Count: {}\n\
         Player: {} (Class: {}, HP: {}/{}, Inventory: {:?})\n\n\
         ACTIVE LORE TRIGGERS:\n\
         {}\n\n\
         PLAYER'S LAST ACTION:\n\
         \"{}\"\n\n\
         Determine if any of the active triggers are fired/activated by the player's action and current state.\n\
         Output ONLY a JSON object with this exact schema:\n\
         {{\n\
           \"trigger_fired\": bool,\n\
           \"fired_trigger_index\": integer_or_null,\n\
           \"consequence\": \"description of what happens, or empty string\"\n\
         }}",
        state.current_location,
        state.turn_count,
        state.player.name,
        state.player.class_or_role,
        state.player.hp,
        state.player.max_hp,
        state.player.inventory,
        triggers_text,
        action
    );

    let schema = roco_engine::grammar::Schema::object()
        .prop("trigger_fired", roco_engine::grammar::Schema::boolean())
        .prop(
            "fired_trigger_index",
            roco_engine::grammar::Schema::integer(),
        )
        .prop("consequence", roco_engine::grammar::Schema::string())
        .build();

    let request = roco_engine::CompletionRequest {
        prompt: format!(
            "System: Evaluate TTRPG triggers. Output valid JSON.\n\n{}",
            prompt
        ),
        temperature: 0.1,
        max_tokens: 150,
        prefill: Some("{\n".into()),
        grammar: schema.to_gbnf("TriggerEval").ok(),
        ..Default::default()
    };

    if let Ok(resp) = futures::executor::block_on(backend.complete(request)) {
        let cleaned = roco_engine::grammar::strategies::clean_json_output(&resp.text);
        if let Ok(parsed) = serde_json::from_str::<NluTriggerResponse>(&cleaned) {
            if parsed.trigger_fired {
                if let Some(idx) = parsed.fired_trigger_index {
                    if idx < state.triggers.len() {
                        return Some((idx, parsed.consequence));
                    }
                }
            }
        }
    }
    None
}

/// Use NLU to analyze a player's action and determine if it requires a stat/attribute check.
fn evaluate_nlu_action(
    backend: &dyn roco_engine::ModelBackend,
    state: &TtrpgState,
    action: &str,
) -> NluActionResponse {
    let prompt = format!(
        "You are the NLU action interpreter for a TTRPG game.\n\n\
         CURRENT STATE:\n\
         Location: {}\n\
         Player: {} (Class: {}, HP: {}/{}, Skills: {:?})\n\n\
         PLAYER'S ACTION:\n\
         \"{}\"\n\n\
         Determine if this action requires an attribute check (Strength, Dexterity, Constitution, Intelligence, Wisdom, Charisma) with a Difficulty Class (DC), or if it is simple/conversational.\n\
         Output ONLY a JSON object with this exact schema:\n\
         {{\n\
           \"requires_check\": bool,\n\
           \"attribute\": \"Strength|Dexterity|Constitution|Intelligence|Wisdom|Charisma|null\",\n\
           \"dc\": integer_or_null,\n\
           \"consequence_success\": \"narrative if check succeeds\",\n\
           \"consequence_failure\": \"narrative if check fails\",\n\
           \"simple_outcome\": \"narrative outcome if no check is required\"\n\
         }}",
        state.current_location,
        state.player.name,
        state.player.class_or_role,
        state.player.hp,
        state.player.max_hp,
        state.player.skills,
        action
    );

    let schema = roco_engine::grammar::Schema::object()
        .prop("requires_check", roco_engine::grammar::Schema::boolean())
        .prop("attribute", roco_engine::grammar::Schema::string())
        .prop("dc", roco_engine::grammar::Schema::integer())
        .prop(
            "consequence_success",
            roco_engine::grammar::Schema::string(),
        )
        .prop(
            "consequence_failure",
            roco_engine::grammar::Schema::string(),
        )
        .prop("simple_outcome", roco_engine::grammar::Schema::string())
        .build();

    let request = roco_engine::CompletionRequest {
        prompt: format!(
            "System: Interpret TTRPG action. Output valid JSON.\n\n{}",
            prompt
        ),
        temperature: 0.1,
        max_tokens: 300,
        prefill: Some("{\n".into()),
        grammar: schema.to_gbnf("ActionEval").ok(),
        ..Default::default()
    };

    if let Ok(resp) = futures::executor::block_on(backend.complete(request)) {
        let cleaned = roco_engine::grammar::strategies::clean_json_output(&resp.text);
        if let Ok(parsed) = serde_json::from_str::<NluActionResponse>(&cleaned) {
            return parsed;
        }
    }

    // High quality fallback in case of model error / mock backend
    NluActionResponse {
        requires_check: false,
        attribute: None,
        dc: None,
        consequence_success: "".to_string(),
        consequence_failure: "".to_string(),
        simple_outcome: format!("You perform: {action} successfully, navigating the environment."),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Public CLI Subcommands & Interactive Game Loop
// ═══════════════════════════════════════════════════════════════════════════

/// Run the TTRPG Game & World Building System.
pub fn cmd_ttrpg(_extra: &[&str]) {
    let backend = daemon::ensure_sync_backend();
    let mut state = TtrpgState::load_or_default();

    r::header("⚔️ RoCo TTRPG Campaign System ⚔️");
    println!(
        "  World:   {}{}{}",
        r::Colors::CYAN,
        state.world.name,
        r::Colors::RESET
    );
    println!(
        "  Player:  {}{}{} (Level {} {})",
        r::Colors::GREEN,
        state.player.name,
        r::Colors::RESET,
        state.player.level,
        state.player.class_or_role
    );
    println!("  Location: {}\n", state.current_location);
    r::dim("  Type actions in natural language, or use command system.");
    r::dim("  Type ':help' or '/help' to see available triggers, chat, and stats commands.\n");

    // Print starting location description
    if let Some(loc) = state
        .world
        .locations
        .iter()
        .find(|l| l.name == state.current_location)
    {
        println!("{}{}{}", r::Colors::CYAN, loc.name, r::Colors::RESET);
        println!("{}", loc.description);
        if !loc.exits.is_empty() {
            let exit_strs: Vec<_> = loc
                .exits
                .iter()
                .map(|(d, l)| format!("{d} -> {l}"))
                .collect();
            r::dim(&format!("Exits: {}", exit_strs.join(", ")));
        }
    }

    let mut stdin_buf = String::new();

    loop {
        print!(
            "\n{}⚔️ [{}] {} >{} ",
            r::Colors::DIM,
            state.turn_count,
            state.player.name,
            r::Colors::RESET
        );
        io::stdout().flush().ok();

        stdin_buf.clear();
        if io::stdin().read_line(&mut stdin_buf).is_err() {
            break;
        }
        let input = stdin_buf.trim().to_string();

        if input.is_empty() {
            continue;
        }

        // Handle Slash Commands / Game Master commands
        if input.starts_with(':') || input.starts_with('/') {
            let cmd_text = input
                .trim_start_matches(':')
                .trim_start_matches('/')
                .trim()
                .to_string();
            let mut parts = cmd_text.split_whitespace();
            let command = parts.next().unwrap_or("").to_lowercase();
            let args = cmd_text[command.len()..].trim().to_string();

            match command.as_str() {
                "help" | "h" | "?" => {
                    r::panel(
                        "TTRPG Subcommands",
                        &[
                            "  :sheet / :s              Display your character sheet & attributes",
                            "  :world / :w              Display world locations, factions, and NPCs",
                            "  :triggers / :t           List registered natural language triggers",
                            "  :roll <stat>             Roll a d20 + attribute modifier (e.g. :roll Dexterity)",
                            "  :chat <npc_name>         Start a conversation chat session with an NPC",
                            "  :add_location <name>     Dynamically build a new location in the world",
                            "  :add_faction <name>      Dynamically build a new faction",
                            "  :add_npc <name>          Dynamically build/register an NPC character sheet",
                            "  :add_trigger <text>      Add a natural language trigger checked every turn",
                            "  :add_lore <name>         Dynamically build a lore artifact/record",
                            "  :save                    Save campaign state manually",
                            "  :restart                 Reset campaign to original default setting",
                            "  :quit / :q               Save and exit the game",
                        ]
                        .join("\n"),
                    );
                }
                "sheet" | "s" => {
                    r::header(&format!("Character Sheet: {}", state.player.name));
                    println!(
                        "Role:    {} (Level {})",
                        state.player.class_or_role, state.player.level
                    );
                    println!("HP:      {}/{}", state.player.hp, state.player.max_hp);
                    println!("\nAttributes:");
                    let mut keys: Vec<_> = state.player.attributes.keys().cloned().collect();
                    keys.sort();
                    for key in keys {
                        let val = state.player.attributes.get(&key).unwrap();
                        let modifier = state.player.get_modifier(&key);
                        let sign = if modifier >= 0 { "+" } else { "" };
                        println!("  - {:<12}: {:2} ({}{})", key, val, sign, modifier);
                    }
                    println!("\nSkills:    {}", state.player.skills.join(", "));
                    println!("Inventory: {}", state.player.inventory.join(", "));
                }
                "world" | "w" => {
                    r::header(&format!("Worldbuilding: {}", state.world.name));
                    println!("{}", state.world.description);

                    println!("\n📚 Locations:");
                    for loc in &state.world.locations {
                        let active = if loc.name == state.current_location {
                            " (Current)"
                        } else {
                            ""
                        };
                        println!("  - {}{}{}", r::Colors::CYAN, loc.name, active);
                        println!("    {}", loc.description);
                    }

                    println!("\n👥 NPCs:");
                    for npc in &state.world.npcs {
                        println!(
                            "  - {}{} ({}){}",
                            r::Colors::GREEN,
                            npc.name,
                            npc.class_or_role,
                            r::Colors::RESET
                        );
                    }

                    println!("\n🚩 Factions:");
                    for f in &state.world.factions {
                        let color = match f.disposition.as_str() {
                            "Friendly" => r::Colors::GREEN,
                            "Hostile" => r::Colors::RED,
                            "Suspicious" => r::Colors::CYAN,
                            _ => r::Colors::DIM,
                        };
                        println!(
                            "  - {} ({}Status: {}{})",
                            f.name,
                            color,
                            f.disposition,
                            r::Colors::RESET
                        );
                        println!("    {}", f.description);
                    }

                    println!("\n🔮 Lore Artifacts:");
                    for l in &state.world.lore {
                        println!("  - {}: {}", l.name, l.description);
                    }
                }
                "triggers" | "t" => {
                    r::header("Natural Language Triggers & Checks");
                    if state.triggers.is_empty() {
                        println!("No active triggers.");
                    } else {
                        for (i, t) in state.triggers.iter().enumerate() {
                            let status = if t.is_active { "Active" } else { "Inactive" };
                            println!("  {}. [{}] {}", i + 1, status, t.trigger_text);
                        }
                    }
                }
                "roll" => {
                    if args.is_empty() {
                        r::warning("Usage: :roll <AttributeName> (e.g. :roll Dexterity)");
                        continue;
                    }
                    let attr = state
                        .player
                        .attributes
                        .keys()
                        .find(|k| k.to_lowercase() == args.to_lowercase())
                        .cloned();

                    match attr {
                        Some(attr_name) => {
                            let modifier = state.player.get_modifier(&attr_name);
                            let roll = roll_d20();
                            let total = roll + modifier;
                            r::success(&format!("Rolling d20 for {} check!", attr_name));
                            println!(
                                "  Result:  {}d20[{}] + {}Modifier[{}] = {}{}{}",
                                r::Colors::CYAN,
                                roll,
                                r::Colors::GREEN,
                                modifier,
                                r::Colors::CYAN,
                                total,
                                r::Colors::RESET
                            );
                        }
                        None => {
                            r::warning(&format!("Attribute '{}' not found. Available: Strength, Dexterity, Constitution, Intelligence, Wisdom, Charisma", args));
                        }
                    }
                }
                "add_location" => {
                    if args.is_empty() {
                        r::warning("Usage: :add_location <LocationName>");
                        continue;
                    }
                    print!("Enter Location Description: ");
                    io::stdout().flush().ok();
                    let mut desc = String::new();
                    io::stdin().read_line(&mut desc).ok();
                    let desc = desc.trim().to_string();

                    state.world.locations.push(TtrpgLocation {
                        name: args.clone(),
                        description: desc,
                        exits: HashMap::new(),
                    });
                    r::success(&format!(
                        "Location '{}' added to worldbuilding system.",
                        args
                    ));
                    let _ = state.save();
                }
                "add_faction" => {
                    if args.is_empty() {
                        r::warning("Usage: :add_faction <FactionName>");
                        continue;
                    }
                    print!("Enter Faction Description: ");
                    io::stdout().flush().ok();
                    let mut desc = String::new();
                    io::stdin().read_line(&mut desc).ok();
                    let desc = desc.trim().to_string();

                    state.world.factions.push(TtrpgFaction {
                        name: args.clone(),
                        description: desc,
                        disposition: "Neutral".to_string(),
                    });
                    r::success(&format!("Faction '{}' added to factions database.", args));
                    let _ = state.save();
                }
                "add_npc" => {
                    if args.is_empty() {
                        r::warning("Usage: :add_npc <NpcName>");
                        continue;
                    }
                    print!("Enter Role/Class: ");
                    io::stdout().flush().ok();
                    let mut role = String::new();
                    io::stdin().read_line(&mut role).ok();
                    let role = role.trim().to_string();

                    let mut npc_attrs = HashMap::new();
                    npc_attrs.insert("Strength".to_string(), 10);
                    npc_attrs.insert("Dexterity".to_string(), 10);
                    npc_attrs.insert("Constitution".to_string(), 10);
                    npc_attrs.insert("Intelligence".to_string(), 10);
                    npc_attrs.insert("Wisdom".to_string(), 10);
                    npc_attrs.insert("Charisma".to_string(), 10);

                    state.world.npcs.push(TtrpgCharacter {
                        name: args.clone(),
                        class_or_role: role,
                        level: 1,
                        hp: 10,
                        max_hp: 10,
                        attributes: npc_attrs,
                        inventory: vec![],
                        skills: vec![],
                    });
                    r::success(&format!("NPC Character Sheet created for '{}'.", args));
                    let _ = state.save();
                }
                "add_trigger" => {
                    if args.is_empty() {
                        r::warning("Usage: :add_trigger <Trigger text describing consequence>");
                        continue;
                    }
                    state.triggers.push(TtrpgTrigger {
                        trigger_text: args.clone(),
                        is_active: true,
                    });
                    r::success(&format!(
                        "Natural language trigger registered: \"{}\"",
                        args
                    ));
                    let _ = state.save();
                }
                "add_lore" => {
                    if args.is_empty() {
                        r::warning("Usage: :add_lore <LoreArtifactName>");
                        continue;
                    }
                    print!("Enter Lore Description: ");
                    io::stdout().flush().ok();
                    let mut desc = String::new();
                    io::stdin().read_line(&mut desc).ok();
                    let desc = desc.trim().to_string();

                    state.world.lore.push(TtrpgLore {
                        name: args.clone(),
                        description: desc,
                    });
                    r::success(&format!("Lore record '{}' added to world database.", args));
                    let _ = state.save();
                }
                "chat" => {
                    if args.is_empty() {
                        r::warning("Usage: :chat <NPC Name>");
                        continue;
                    }
                    run_character_chat(&args, &mut state, &*backend);
                }
                "save" => {
                    if state.save().is_ok() {
                        r::success("Campaign state saved successfully.");
                    }
                }
                "restart" => {
                    state = TtrpgState::create_default();
                    let _ = state.save();
                    r::success("Campaign restarted to default setting.");
                }
                "quit" | "q" | "exit" => {
                    let _ = state.save();
                    r::info("Campaign saved. Goodbye!");
                    break;
                }
                _ => {
                    r::warning(&format!(
                        "Unknown subcommand: :{command}. Type :help for all subcommands."
                    ));
                }
            }
            continue;
        }

        // Natural Language Turn Processing
        state.turn_count += 1;
        state.history.push(format!("Player: {input}"));

        print!(
            "{}{}[GM is evaluating NLU triggers/actions...]{}\r",
            r::Colors::DIM,
            r::Colors::CYAN,
            r::Colors::RESET
        );
        io::stdout().flush().ok();

        // 1. Evaluate Natural Language Triggers
        if let Some((idx, consequence)) = evaluate_nlu_triggers(&*backend, &state, &input) {
            print!("\r\x1b[K"); // Clear the evaluating line
            let triggered_text = &state.triggers[idx].trigger_text;
            r::warning(&format!(
                "⚡ Natural Language Trigger Fired: \"{}\"",
                triggered_text
            ));
            println!("\n{}", consequence);
            state
                .history
                .push(format!("Trigger [{}]: {}", triggered_text, consequence));

            // Apply minor state outcomes based on trigger keywords dynamically
            let consequence_lower = consequence.to_lowercase();
            if consequence_lower.contains("lose") && consequence_lower.contains("hp") {
                state.player.hp = (state.player.hp - 2).max(0);
                r::error(&format!(
                    "HP decreased! Current HP: {}/{}",
                    state.player.hp, state.player.max_hp
                ));
            }
            let _ = state.save();
            continue;
        }

        // 2. Evaluate Normal Action using NLU
        let action_evaluation = evaluate_nlu_action(&*backend, &state, &input);
        print!("\r\x1b[K"); // Clear evaluating line

        if action_evaluation.requires_check {
            let attr = action_evaluation
                .attribute
                .unwrap_or_else(|| "Dexterity".to_string());
            let dc = action_evaluation.dc.unwrap_or(12);

            println!(
                "\n🎲 NLU demands an attribute check! Action requires a {} check (DC {}).",
                attr, dc
            );

            let modifier = state.player.get_modifier(&attr);
            let roll = roll_d20();
            let total = roll + modifier;

            print!("Press Enter to roll d20...");
            io::stdout().flush().ok();
            let mut unused = String::new();
            io::stdin().read_line(&mut unused).ok();

            println!(
                "Roll result: d20[{}] + {}Modifier[{}] = {}",
                roll, attr, modifier, total
            );

            if total >= dc {
                println!(
                    "{}{}{} Success! DC {} met.",
                    r::Colors::GREEN,
                    r::Colors::BOLD,
                    r::Colors::RESET,
                    dc
                );
                println!("\n{}", action_evaluation.consequence_success);
                state.history.push(format!(
                    "Action success: {}",
                    action_evaluation.consequence_success
                ));
            } else {
                println!(
                    "{}{}{} Failure! DC {} was not met.",
                    r::Colors::RED,
                    r::Colors::BOLD,
                    r::Colors::RESET,
                    dc
                );
                println!("\n{}", action_evaluation.consequence_failure);
                state.history.push(format!(
                    "Action failure: {}",
                    action_evaluation.consequence_failure
                ));

                // Apply dynamic HP consequences if narrative suggests damage
                let fail_lower = action_evaluation.consequence_failure.to_lowercase();
                if fail_lower.contains("damage")
                    || fail_lower.contains("hurt")
                    || fail_lower.contains("wound")
                {
                    state.player.hp = (state.player.hp - 3).max(0);
                    r::error(&format!(
                        "You sustained damage! HP: {}/{}",
                        state.player.hp, state.player.max_hp
                    ));
                }
            }
        } else {
            // No attribute check needed, evaluate simple outcome via NLU description
            println!("\n{}", action_evaluation.simple_outcome);
            state
                .history
                .push(format!("Outcome: {}", action_evaluation.simple_outcome));
        }

        // Handle exits transition if player declared movement to a known exit
        let input_lower = input.to_lowercase();
        if let Some(loc) = state
            .world
            .locations
            .iter()
            .find(|l| l.name == state.current_location)
        {
            for (dir, dest) in &loc.exits {
                if input_lower.contains(&dir.to_lowercase())
                    || input_lower.contains(&dest.to_lowercase())
                {
                    r::success(&format!("Traveling to {}...", dest));
                    state.current_location = dest.clone();
                    if let Some(new_loc) = state
                        .world
                        .locations
                        .iter()
                        .find(|l| l.name == state.current_location)
                    {
                        println!("\n{}{}{}", r::Colors::CYAN, new_loc.name, r::Colors::RESET);
                        println!("{}", new_loc.description);
                    }
                    break;
                }
            }
        }

        let _ = state.save();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Character Chat Mode
// ═══════════════════════════════════════════════════════════════════════════

fn run_character_chat(
    npc_query: &str,
    state: &mut TtrpgState,
    backend: &dyn roco_engine::ModelBackend,
) {
    let npc = state
        .world
        .npcs
        .iter()
        .find(|n| n.name.to_lowercase().contains(&npc_query.to_lowercase()))
        .cloned();

    let npc = match npc {
        Some(n) => n,
        None => {
            r::warning(&format!(
                "NPC matching '{}' not found in world registry.",
                npc_query
            ));
            return;
        }
    };

    r::header(&format!("💬 Entering immersive chat with {}", npc.name));
    println!("  Role:    {}", npc.class_or_role);
    println!("  Level:   {}", npc.level);
    println!("  Type ':exit' or ':quit' to finish conversing and return to GM.\n");

    let mut buf = String::new();

    loop {
        print!(
            "\n{}💬 Chat ({}) >{} ",
            r::Colors::GREEN,
            npc.name,
            r::Colors::RESET
        );
        io::stdout().flush().ok();

        buf.clear();
        if io::stdin().read_line(&mut buf).is_err() {
            break;
        }
        let input = buf.trim().to_string();

        if input.is_empty() {
            continue;
        }

        if input == ":exit" || input == ":quit" || input == "/exit" || input == "/quit" {
            r::info(&format!("Leaving conversation with {}.", npc.name));
            break;
        }

        // Construct roleplay system prompt
        let system_prompt = format!(
            "You are roleplaying in-character as {}.\n\
             ROLE: {}\n\
             WORLD CONTEXT:\n\
             Location: {}\n\
             World description: {}\n\n\
             RULES:\n\
             - Strictly stay in character as {}.\n\
             - Keep responses immersive, flavorful, and concise (1-3 sentences).\n\
             - Adopt their personality and motivations naturally.",
            npc.name, npc.class_or_role, state.current_location, state.world.description, npc.name
        );

        let request = roco_engine::CompletionRequest {
            prompt: format!("System: {}\n\nUser: {}\n\nAssistant:", system_prompt, input),
            temperature: 0.8,
            max_tokens: 200,
            ..Default::default()
        };

        print!(
            "{}{}[Responding...]{}\r",
            r::Colors::DIM,
            r::Colors::GREEN,
            r::Colors::RESET
        );
        io::stdout().flush().ok();

        match futures::executor::block_on(backend.complete(request)) {
            Ok(resp) => {
                print!("\r\x1b[K"); // Clear responding message
                let reply = resp.text.trim().to_string();
                println!("{}", reply);
                state.history.push(format!("{}: {}", npc.name, reply));
            }
            Err(e) => {
                print!("\r\x1b[K");
                r::error(&format!("Conversation failed: {e}"));
            }
        }
    }
}
