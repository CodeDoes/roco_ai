//! Condensed data forms — compact structured representations of story elements.
//!
//! These are used by the summarizer, planner, and UI to avoid reparsing
//! full text every time. They are also the input format for model-based
//! operations (summarization, brainstorming, critique).
//!
//! # Generation
//!
//! - `CondensedChapter`: word count, character names (wiki cross-ref),
//!   settings mentioned, plot points, tone, POV, 2-sentence summary.
//! - `CondensedWiki`: character/setting/lore entries with counts.
//!
//! Classic extraction (word count, character names via wiki cross-ref) is
//! always available. Inference-backed fields (plot points, tone, themes)
//! are generated lazily and cached per session.

use std::collections::{HashMap, HashSet};

/// Condensed representation of a single chapter.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CondensedChapter {
    pub chapter_num: usize,
    pub title: String,
    pub word_count: usize,
    pub characters_mentioned: Vec<String>,
    pub settings_mentioned: Vec<String>,
    pub plot_points: Vec<String>,
    pub tone: String,
    pub pov_character: Option<String>,
    pub summary_2_sentences: String,
    pub themes: Vec<String>,
}

impl CondensedChapter {
    /// Create a condensed chapter from raw text with minimal classic extraction.
    ///
    /// Classic extraction fills: `chapter_num`, `title`, `word_count`.
    /// Character and setting mentions require a wiki cross-reference (see
    /// `from_text_with_wiki`). Inference-backed fields are left as defaults.
    pub fn from_text(chapter_num: usize, title: &str, text: &str) -> Self {
        let word_count = text.split_whitespace().count();
        Self {
            chapter_num,
            title: title.to_string(),
            word_count,
            characters_mentioned: Vec::new(),
            settings_mentioned: Vec::new(),
            plot_points: Vec::new(),
            tone: "unknown".to_string(),
            pov_character: None,
            summary_2_sentences: String::new(),
            themes: Vec::new(),
        }
    }

    /// Create a condensed chapter with wiki cross-reference for character/setting mentions.
    ///
    /// Scans the chapter text for names found in the wiki entries.
    pub fn from_text_with_wiki(
        chapter_num: usize,
        title: &str,
        text: &str,
        character_names: &[String],
        setting_names: &[String],
    ) -> Self {
        let mut this = Self::from_text(chapter_num, title, text);

        let lower_text = text.to_lowercase();

        // Cross-reference character names
        for name in character_names {
            if lower_text.contains(&name.to_lowercase()) {
                this.characters_mentioned.push(name.clone());
            }
        }

        // Cross-reference setting names
        for name in setting_names {
            if lower_text.contains(&name.to_lowercase()) {
                this.settings_mentioned.push(name.clone());
            }
        }

        this
    }
}

/// A single entry in the wiki (character, setting, or lore item).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WikiEntry {
    pub name: String,
    pub kind: String, // "character", "setting", "lore_item"
    pub description: String,
    pub key_traits: Vec<String>,
    pub relationships: Vec<String>,
}

impl WikiEntry {
    /// Parse a wiki entry from a markdown heading + body.
    ///
    /// Assumes format: `### Name\n\ndescription text\n\n**Traits:** trait1, trait2\n**Relationships:** rel1, rel2`
    pub fn from_md_section(kind: &str, heading: &str, body: &str) -> Self {
        let name = heading.trim_start_matches('#').trim().to_string();
        let (description, key_traits, relationships) = Self::parse_body(body);
        Self {
            name,
            kind: kind.to_string(),
            description,
            key_traits,
            relationships,
        }
    }

    fn parse_body(body: &str) -> (String, Vec<String>, Vec<String>) {
        let mut description = String::new();
        let mut key_traits = Vec::new();
        let mut relationships = Vec::new();

        for line in body.lines() {
            let trimmed = line.trim();
            if let Some(traits) = trimmed.strip_prefix("**Traits:**") {
                key_traits = traits
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if let Some(rels) = trimmed.strip_prefix("**Relationships:**") {
                relationships = rels
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if !trimmed.is_empty() && !trimmed.starts_with("**") {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(trimmed);
            }
        }

        (description, key_traits, relationships)
    }

    /// Word count of the description field.
    pub fn word_count(&self) -> usize {
        self.description.split_whitespace().count()
            + self
                .key_traits
                .iter()
                .map(|t| t.split_whitespace().count())
                .sum::<usize>()
    }
}

/// Condensed representation of a wiki / world-building document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CondensedWiki {
    pub characters: Vec<WikiEntry>,
    pub settings: Vec<WikiEntry>,
    pub lore_items: Vec<WikiEntry>,
    pub entry_count: usize,
    pub total_word_count: usize,
}

impl CondensedWiki {
    /// Parse a markdown wiki document into a condensed form.
    ///
    /// Expects sections:
    /// ```markdown
    /// ## Characters
    /// ### Name
    /// description...
    ///
    /// ## Settings
    /// ### Place Name
    /// description...
    ///
    /// ## Lore
    /// ### Item/History
    /// description...
    /// ```
    pub fn from_md(wiki_text: &str) -> Self {
        let mut this = Self::default();

        let mut current_section = String::new();
        let mut current_heading = String::new();
        let mut current_body = String::new();

        for line in wiki_text.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("## ") {
                // Save previous entry if any
                if !current_heading.is_empty() && !current_body.trim().is_empty() {
                    let kind = match current_section.to_lowercase().as_str() {
                        s if s.contains("character") => "character",
                        s if s.contains("setting") => "setting",
                        _ => "lore_item",
                    };
                    let entry = WikiEntry::from_md_section(kind, &current_heading, &current_body);
                    match entry.kind.as_str() {
                        "character" => this.characters.push(entry),
                        "setting" => this.settings.push(entry),
                        _ => this.lore_items.push(entry),
                    }
                }

                current_section = trimmed.trim_start_matches("## ").to_string();
                current_heading.clear();
                current_body.clear();
            } else if trimmed.starts_with("### ") {
                // Save previous sub-entry
                if !current_heading.is_empty() && !current_body.trim().is_empty() {
                    let kind = match current_section.to_lowercase().as_str() {
                        s if s.contains("character") => "character",
                        s if s.contains("setting") => "setting",
                        _ => "lore_item",
                    };
                    let entry = WikiEntry::from_md_section(kind, &current_heading, &current_body);
                    match entry.kind.as_str() {
                        "character" => this.characters.push(entry),
                        "setting" => this.settings.push(entry),
                        _ => this.lore_items.push(entry),
                    }
                }

                current_heading = trimmed.trim_start_matches("### ").to_string();
                current_body.clear();
            } else if !trimmed.is_empty() {
                if !current_body.is_empty() {
                    current_body.push(' ');
                }
                current_body.push_str(trimmed);
            }
        }

        // Save last entry
        if !current_heading.is_empty() && !current_body.trim().is_empty() {
            let kind = match current_section.to_lowercase().as_str() {
                s if s.contains("character") => "character",
                s if s.contains("setting") => "setting",
                _ => "lore_item",
            };
            let entry = WikiEntry::from_md_section(kind, &current_heading, &current_body);
            match entry.kind.as_str() {
                "character" => this.characters.push(entry),
                "setting" => this.settings.push(entry),
                _ => this.lore_items.push(entry),
            }
        }

        this.entry_count = this.characters.len() + this.settings.len() + this.lore_items.len();
        this.total_word_count = this
            .characters
            .iter()
            .map(|e| e.word_count())
            .sum::<usize>()
            + this.settings.iter().map(|e| e.word_count()).sum::<usize>()
            + this
                .lore_items
                .iter()
                .map(|e| e.word_count())
                .sum::<usize>();

        this
    }

    /// Get all character names.
    pub fn character_names(&self) -> Vec<String> {
        self.characters.iter().map(|c| c.name.clone()).collect()
    }

    /// Get all setting/location names.
    pub fn setting_names(&self) -> Vec<String> {
        self.settings.iter().map(|s| s.name.clone()).collect()
    }

    /// Find entries whose name or description contains a query string.
    pub fn search(&self, query: &str) -> Vec<&WikiEntry> {
        let lower = query.to_lowercase();
        self.characters
            .iter()
            .chain(self.settings.iter())
            .chain(self.lore_items.iter())
            .filter(|e| {
                e.name.to_lowercase().contains(&lower)
                    || e.description.to_lowercase().contains(&lower)
            })
            .collect()
    }
}

/// Extract the title from a chapter heading line (`# Title`).
pub fn extract_chapter_title(line: &str) -> String {
    line.trim_start_matches('#').trim().to_string()
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condensed_chapter_from_text() {
        let cc = CondensedChapter::from_text(1, "The Beginning", "Once upon a time...");
        assert_eq!(cc.chapter_num, 1);
        assert_eq!(cc.title, "The Beginning");
        assert_eq!(cc.word_count, 4);
    }

    #[test]
    fn test_condensed_chapter_with_wiki() {
        let text = "Alice walked through the forest to Mystic Valley.";
        let cc = CondensedChapter::from_text_with_wiki(
            1,
            "Chapter 1",
            text,
            &["Alice".to_string(), "Bob".to_string()],
            &["Mystic Valley".to_string(), "Dark Forest".to_string()],
        );
        assert!(cc.characters_mentioned.contains(&"Alice".to_string()));
        assert!(!cc.characters_mentioned.contains(&"Bob".to_string()));
        assert!(cc.settings_mentioned.contains(&"Mystic Valley".to_string()));
        assert!(!cc.settings_mentioned.contains(&"Dark Forest".to_string()));
    }

    #[test]
    fn test_wiki_entry_from_md() {
        let entry = WikiEntry::from_md_section(
            "character",
            "Alice",
            "A brave adventurer who seeks the lost crystal.\n**Traits:** brave, curious, kind\n**Relationships:** Bob (friend)",
        );
        assert_eq!(entry.name, "Alice");
        assert_eq!(entry.kind, "character");
        assert!(entry.description.contains("brave adventurer"));
        assert!(entry.key_traits.contains(&"brave".to_string()));
        assert!(entry.relationships.contains(&"Bob (friend)".to_string()));
    }

    #[test]
    fn test_condensed_wiki_from_md() {
        let md = r#"## Characters
### Alice
A brave adventurer.
**Traits:** brave, curious
**Relationships:** Bob (friend)

### Bob
A wise wizard.
**Traits:** wise, old

## Settings
### Mystic Valley
A beautiful valley surrounded by mountains.
"#;
        let cw = CondensedWiki::from_md(&md);
        assert_eq!(cw.characters.len(), 2);
        assert_eq!(cw.settings.len(), 1);
        assert_eq!(cw.entry_count, 3);
        assert!(cw.character_names().contains(&"Alice".to_string()));
        assert!(cw.setting_names().contains(&"Mystic Valley".to_string()));
    }

    #[test]
    fn test_condensed_wiki_search() {
        let md = r#"## Characters
### Alice
A brave adventurer.
### Bob
A wise wizard.
"#;
        let cw = CondensedWiki::from_md(&md);
        let results = cw.search("brave");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alice");

        let results = cw.search("wizard");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Bob");
    }

    #[test]
    fn test_wiki_entry_word_count() {
        let entry = WikiEntry::from_md_section(
            "character",
            "Test",
            "A very long description with many words here and there.\n**Traits:** trait1, trait2",
        );
        assert!(entry.word_count() > 5);
    }
}
