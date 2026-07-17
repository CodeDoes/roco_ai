//! Story Direction — capture and apply human's creative vision.
//!
//! The human sets the direction for the story:
//! - Tone (dark, light, humorous, serious)
//! - Style (literary, pulp, minimalist, ornate)
//! - Themes (redemption, revenge, love, loss)
//! - Pacing (fast, slow, building)
//! - Character focus (which characters matter most)
//!
//! This direction is applied consistently throughout generation.

use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Story Direction
// ═════════════════════════════════════════════════════════════════════════════

/// The human's creative vision for the story
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoryDirection {
    /// Overall tone (e.g., "dark", "light", "humorous", "serious")
    pub tone: Option<String>,
    /// Writing style (e.g., "literary", "pulp", "minimalist", "ornate")
    pub style: Option<String>,
    /// Key themes (e.g., "redemption", "revenge", "love", "loss")
    pub themes: Vec<String>,
    /// Pacing preference (e.g., "fast", "slow", "building")
    pub pacing: Option<String>,
    /// Characters that matter most
    pub focus_characters: Vec<String>,
    /// Any specific instructions
    pub instructions: Vec<String>,
    /// Mood (e.g., "tense", "peaceful", "mysterious")
    pub mood: Option<String>,
    /// Target audience (e.g., "young adult", "adult", "literary")
    pub audience: Option<String>,
}

impl StoryDirection {
    /// Create a new empty direction
    pub fn new() -> Self {
        Self::default()
    }

    /// Set tone
    pub fn with_tone(mut self, tone: &str) -> Self {
        self.tone = Some(tone.to_string());
        self
    }

    /// Set style
    pub fn with_style(mut self, style: &str) -> Self {
        self.style = Some(style.to_string());
        self
    }

    /// Add a theme
    pub fn with_theme(mut self, theme: &str) -> Self {
        self.themes.push(theme.to_string());
        self
    }

    /// Set pacing
    pub fn with_pacing(mut self, pacing: &str) -> Self {
        self.pacing = Some(pacing.to_string());
        self
    }

    /// Add a focus character
    pub fn with_focus_character(mut self, character: &str) -> Self {
        self.focus_characters.push(character.to_string());
        self
    }

    /// Add an instruction
    pub fn with_instruction(mut self, instruction: &str) -> Self {
        self.instructions.push(instruction.to_string());
        self
    }

    /// Set mood
    pub fn with_mood(mut self, mood: &str) -> Self {
        self.mood = Some(mood.to_string());
        self
    }

    /// Set audience
    pub fn with_audience(mut self, audience: &str) -> Self {
        self.audience = Some(audience.to_string());
        self
    }

    /// Merge another direction into this one (other takes precedence)
    pub fn merge(&mut self, other: StoryDirection) {
        if other.tone.is_some() {
            self.tone = other.tone;
        }
        if other.style.is_some() {
            self.style = other.style;
        }
        if !other.themes.is_empty() {
            self.themes = other.themes;
        }
        if other.pacing.is_some() {
            self.pacing = other.pacing;
        }
        if !other.focus_characters.is_empty() {
            self.focus_characters = other.focus_characters;
        }
        if !other.instructions.is_empty() {
            self.instructions.extend(other.instructions);
        }
        if other.mood.is_some() {
            self.mood = other.mood;
        }
        if other.audience.is_some() {
            self.audience = other.audience;
        }
    }

    /// Convert to a prompt instruction
    pub fn to_prompt_instruction(&self) -> String {
        let mut instruction = String::new();

        if let Some(ref tone) = self.tone {
            instruction.push_str(&format!("Tone: {}\n", tone));
        }

        if let Some(ref style) = self.style {
            instruction.push_str(&format!("Style: {}\n", style));
        }

        if !self.themes.is_empty() {
            instruction.push_str(&format!("Themes: {}\n", self.themes.join(", ")));
        }

        if let Some(ref pacing) = self.pacing {
            instruction.push_str(&format!("Pacing: {}\n", pacing));
        }

        if !self.focus_characters.is_empty() {
            instruction.push_str(&format!("Focus on: {}\n", self.focus_characters.join(", ")));
        }

        if !self.instructions.is_empty() {
            instruction.push_str("\nSpecial instructions:\n");
            for instr in &self.instructions {
                instruction.push_str(&format!("- {}\n", instr));
            }
        }

        if let Some(ref mood) = self.mood {
            instruction.push_str(&format!("Mood: {}\n", mood));
        }

        if let Some(ref audience) = self.audience {
            instruction.push_str(&format!("Audience: {}\n", audience));
        }

        instruction
    }

    /// Check if direction is empty
    pub fn is_empty(&self) -> bool {
        self.tone.is_none()
            && self.style.is_none()
            && self.themes.is_empty()
            && self.pacing.is_none()
            && self.focus_characters.is_empty()
            && self.instructions.is_empty()
            && self.mood.is_none()
            && self.audience.is_none()
    }

    /// Get a summary of the direction
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref tone) = self.tone {
            parts.push(format!("Tone: {}", tone));
        }
        if let Some(ref style) = self.style {
            parts.push(format!("Style: {}", style));
        }
        if !self.themes.is_empty() {
            parts.push(format!("Themes: {}", self.themes.join(", ")));
        }
        if let Some(ref pacing) = self.pacing {
            parts.push(format!("Pacing: {}", pacing));
        }
        if let Some(ref mood) = self.mood {
            parts.push(format!("Mood: {}", mood));
        }

        parts.join(" | ")
    }

    /// Parse direction from natural language
    pub fn from_natural_language(text: &str) -> Self {
        let mut direction = Self::new();
        let lower = text.to_lowercase();

        // Tone
        if lower.contains("dark") || lower.contains("grim") {
            direction.tone = Some("dark".to_string());
        } else if lower.contains("light") || lower.contains("bright") {
            direction.tone = Some("light".to_string());
        } else if lower.contains("humorous") || lower.contains("funny") {
            direction.tone = Some("humorous".to_string());
        } else if lower.contains("serious") || lower.contains("gritty") {
            direction.tone = Some("serious".to_string());
        }

        // Style
        if lower.contains("literary") {
            direction.style = Some("literary".to_string());
        } else if lower.contains("pulp") || lower.contains("action") {
            direction.style = Some("pulp".to_string());
        } else if lower.contains("minimalist") || lower.contains("sparse") {
            direction.style = Some("minimalist".to_string());
        }

        // Pacing
        if lower.contains("fast") || lower.contains("quick") || lower.contains("action") {
            direction.pacing = Some("fast".to_string());
        } else if lower.contains("slow") || lower.contains("contemplative") {
            direction.pacing = Some("slow".to_string());
        }

        // Mood
        if lower.contains("tense") || lower.contains("suspenseful") {
            direction.mood = Some("tense".to_string());
        } else if lower.contains("peaceful") || lower.contains("calm") {
            direction.mood = Some("peaceful".to_string());
        } else if lower.contains("mysterious") || lower.contains("enigmatic") {
            direction.mood = Some("mysterious".to_string());
        }

        direction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direction_builder() {
        let direction = StoryDirection::new()
            .with_tone("dark")
            .with_style("literary")
            .with_theme("redemption")
            .with_theme("loss")
            .with_pacing("slow")
            .with_mood("tense")
            .with_audience("adult");

        assert_eq!(direction.tone, Some("dark".to_string()));
        assert_eq!(direction.style, Some("literary".to_string()));
        assert_eq!(direction.themes, vec!["redemption", "loss"]);
        assert_eq!(direction.pacing, Some("slow".to_string()));
        assert_eq!(direction.mood, Some("tense".to_string()));
        assert_eq!(direction.audience, Some("adult".to_string()));
    }

    #[test]
    fn test_direction_to_prompt() {
        let direction = StoryDirection::new()
            .with_tone("dark")
            .with_theme("revenge")
            .with_pacing("fast");

        let prompt = direction.to_prompt_instruction();
        assert!(prompt.contains("Tone: dark"));
        assert!(prompt.contains("Themes: revenge"));
        assert!(prompt.contains("Pacing: fast"));
    }

    #[test]
    fn test_direction_merge() {
        let mut direction = StoryDirection::new()
            .with_tone("dark")
            .with_style("literary");

        let other = StoryDirection::new()
            .with_tone("light")
            .with_theme("love");

        direction.merge(other);

        assert_eq!(direction.tone, Some("light".to_string())); // Other takes precedence
        assert_eq!(direction.style, Some("literary".to_string())); // Kept
        assert_eq!(direction.themes, vec!["love"]); // Added
    }

    #[test]
    fn test_from_natural_language() {
        let direction = StoryDirection::from_natural_language("I want a dark, gritty story with fast action");
        assert_eq!(direction.tone, Some("dark".to_string()));
        assert_eq!(direction.pacing, Some("fast".to_string()));
    }

    #[test]
    fn test_direction_summary() {
        let direction = StoryDirection::new()
            .with_tone("dark")
            .with_style("literary")
            .with_mood("tense");

        let summary = direction.summary();
        assert!(summary.contains("Tone: dark"));
        assert!(summary.contains("Style: literary"));
        assert!(summary.contains("Mood: tense"));
    }
}
