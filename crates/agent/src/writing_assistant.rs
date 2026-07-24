//! Writing Assistant — AI-powered writing help.
//!
//! Analyzes user input and provides:
//! - Continuation suggestions
//! - Fill-in-the-middle suggestions
//! - Tagging and categorization
//! - Cross-referencing with existing content
//! - Diff analysis
//!
//! Designed for fast, local inference with small models.

use roco_engine::ModelBackend;
use roco_grammar::{schema_to_gbnf, Schema};
use crate::util::structured_complete;
use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Writing Analysis Types
// ═════════════════════════════════════════════════════════════════════════════

/// Analysis of user's writing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingAnalysis {
    /// Detected themes
    pub themes: Vec<String>,
    /// Detected characters mentioned
    pub characters: Vec<String>,
    /// Detected locations
    pub locations: Vec<String>,
    /// Detected tone
    pub tone: String,
    /// Detected style
    pub style: String,
    /// Key phrases
    pub key_phrases: Vec<String>,
    /// Sentiment (positive, negative, neutral, mixed)
    pub sentiment: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Potential plot hooks
    pub plot_hooks: Vec<String>,
}

impl WritingAnalysis {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("themes", Schema::array(Schema::string()))
            .prop("characters", Schema::array(Schema::string()))
            .prop("locations", Schema::array(Schema::string()))
            .prop("tone", Schema::string())
            .prop("style", Schema::string())
            .prop("key_phrases", Schema::array(Schema::string()))
            .prop(
                "sentiment",
                Schema::enum_values(vec![
                    serde_json::json!("positive"),
                    serde_json::json!("negative"),
                    serde_json::json!("neutral"),
                    serde_json::json!("mixed"),
                ]),
            )
            .prop("tags", Schema::array(Schema::string()))
            .prop("plot_hooks", Schema::array(Schema::string()))
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("WritingAnalysis schema is valid")
    }
}

/// Suggestion for continuation or fill-in-the-middle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingSuggestion {
    /// Type: continuation, fill_middle, alternative, expansion
    pub suggestion_type: String,
    /// The suggested text
    pub text: String,
    /// Why this suggestion was made
    pub reasoning: String,
    /// Confidence (0-1)
    pub confidence: f32,
    /// Where to insert (for fill-middle)
    pub insert_point: Option<usize>,
}

impl WritingSuggestion {
    pub fn schema() -> Schema {
        Schema::object()
            .prop(
                "suggestion_type",
                Schema::enum_values(vec![
                    serde_json::json!("continuation"),
                    serde_json::json!("fill_middle"),
                    serde_json::json!("alternative"),
                    serde_json::json!("expansion"),
                ]),
            )
            .prop("text", Schema::string())
            .prop("reasoning", Schema::string())
            .prop("confidence", Schema::number())
            .prop("insert_point", Schema::integer())
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("WritingSuggestion schema is valid")
    }
}

/// Diff analysis between two versions of text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffAnalysis {
    /// Summary of changes
    pub summary: String,
    /// Type of changes made
    pub change_types: Vec<String>,
    /// Impact on story
    pub story_impact: String,
    /// Suggestions for the changes
    pub suggestions: Vec<String>,
    /// Tags for the changes
    pub tags: Vec<String>,
}

impl DiffAnalysis {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("summary", Schema::string())
            .prop("change_types", Schema::array(Schema::string()))
            .prop("story_impact", Schema::string())
            .prop("suggestions", Schema::array(Schema::string()))
            .prop("tags", Schema::array(Schema::string()))
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("DiffAnalysis schema is valid")
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Writing Assistant
// ═════════════════════════════════════════════════════════════════════════════

/// AI-powered writing assistant
pub struct WritingAssistant {
    /// Context from the story
    story_context: String,
    /// Characters in the story
    characters: Vec<String>,
    /// Locations in the story
    locations: Vec<String>,
    /// Themes in the story
    themes: Vec<String>,
}

impl Default for WritingAssistant {
    fn default() -> Self {
        Self::new()
    }
}

impl WritingAssistant {
    /// Create a new writing assistant
    pub fn new() -> Self {
        Self {
            story_context: String::new(),
            characters: Vec::new(),
            locations: Vec::new(),
            themes: Vec::new(),
        }
    }

    /// Set story context
    pub fn with_context(mut self, context: &str) -> Self {
        self.story_context = context.to_string();
        self
    }

    /// Set characters
    pub fn with_characters(mut self, characters: Vec<String>) -> Self {
        self.characters = characters;
        self
    }

    /// Set locations
    pub fn with_locations(mut self, locations: Vec<String>) -> Self {
        self.locations = locations;
        self
    }

    /// Set themes
    pub fn with_themes(mut self, themes: Vec<String>) -> Self {
        self.themes = themes;
        self
    }

    /// Analyze user's writing
    pub fn analyze(
        &self,
        backend: &dyn ModelBackend,
        text: &str,
    ) -> Result<WritingAnalysis, String> {
        let analysis: WritingAnalysis = structured_complete(
            backend,
            "You are a writing analyst. Analyze the text and extract key elements. Output valid JSON only.",
            &format!(
                "Analyze this text:\n\n{text}\n\n\
                 Story context:\n{}\n\n\
                 Known characters: {}\n\
                 Known locations: {}\n\
                 Known themes: {}\n\n\
                 Extract themes, characters, locations, tone, style, key phrases, \
                 sentiment, tags, and potential plot hooks.\n\
                 Output JSON matching the schema.",
                self.story_context,
                self.characters.join(", "),
                self.locations.join(", "),
                self.themes.join(", "),
            ),
            &WritingAnalysis::grammar(),
            0.3,
            400,
        )?;

        Ok(analysis)
    }

    /// Suggest continuation of user's text
    pub fn suggest_continuation(
        &self,
        backend: &dyn ModelBackend,
        text: &str,
        num_suggestions: usize,
    ) -> Result<Vec<WritingSuggestion>, String> {
        let mut suggestions = Vec::new();

        for _ in 0..num_suggestions {
            let suggestion: WritingSuggestion = structured_complete(
                backend,
                "You are a creative writing assistant. Suggest a continuation of the text. Output valid JSON only.",
                &format!(
                    "Continue this text naturally:\n\n{text}\n\n\
                     Story context:\n{}\n\n\
                     Write 1-3 sentences that continue naturally from where the text left off.\n\
                     Output JSON matching the schema.",
                    self.story_context,
                ),
                &WritingSuggestion::grammar(),
                0.7,
                200,
            )?;

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// Suggest fill-in-the-middle
    pub fn suggest_fill_middle(
        &self,
        backend: &dyn ModelBackend,
        before: &str,
        after: &str,
        num_suggestions: usize,
    ) -> Result<Vec<WritingSuggestion>, String> {
        let mut suggestions = Vec::new();

        for _ in 0..num_suggestions {
            let suggestion: WritingSuggestion = structured_complete(
                backend,
                "You are a creative writing assistant. Fill in the missing text between two passages. Output valid JSON only.",
                &format!(
                    "Fill in the text between these two passages:\n\n\
                     Before:\n{before}\n\n\
                     After:\n{after}\n\n\
                     Story context:\n{}\n\n\
                     Write 1-3 sentences that bridge naturally between the two passages.\n\
                     Output JSON matching the schema.",
                    self.story_context,
                ),
                &WritingSuggestion::grammar(),
                0.7,
                200,
            )?;

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// Analyze a diff between two versions
    pub fn analyze_diff(
        &self,
        backend: &dyn ModelBackend,
        old_text: &str,
        new_text: &str,
    ) -> Result<DiffAnalysis, String> {
        let analysis: DiffAnalysis = structured_complete(
            backend,
            "You are a writing analyst. Analyze the changes between two versions of text. Output valid JSON only.",
            &format!(
                "Analyze the changes between these two versions:\n\n\
                 Original:\n{old_text}\n\n\
                 Revised:\n{new_text}\n\n\
                 Story context:\n{}\n\n\
                 Summarize the changes, categorize them, assess story impact, \
                 and provide suggestions.\n\
                 Output JSON matching the schema.",
                self.story_context,
            ),
            &DiffAnalysis::grammar(),
            0.3,
            400,
        )?;

        Ok(analysis)
    }

    /// Cross-reference text with existing story content
    pub fn cross_reference(
        &self,
        backend: &dyn ModelBackend,
        text: &str,
        existing_content: &str,
    ) -> Result<CrossReference, String> {
        let xref: CrossReference = structured_complete(
            backend,
            "You are a writing analyst. Find connections between the new text and existing content. Output valid JSON only.",
            &format!(
                "Find connections between this new text and existing content:\n\n\
                 New text:\n{text}\n\n\
                 Existing content:\n{existing_content}\n\n\
                 Identify: character references, location references, theme connections, \
                 plot connections, contradictions, and foreshadowing.\n\
                 Output JSON matching the schema.",
            ),
            &CrossReference::grammar(),
            0.3,
            400,
        )?;

        Ok(xref)
    }
}

/// Cross-reference analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossReference {
    /// Character references found
    pub character_refs: Vec<String>,
    /// Location references found
    pub location_refs: Vec<String>,
    /// Theme connections
    pub theme_connections: Vec<String>,
    /// Plot connections
    pub plot_connections: Vec<String>,
    /// Contradictions found
    pub contradictions: Vec<String>,
    /// Foreshadowing detected
    pub foreshadowing: Vec<String>,
}

impl CrossReference {
    pub fn schema() -> Schema {
        Schema::object()
            .prop("character_refs", Schema::array(Schema::string()))
            .prop("location_refs", Schema::array(Schema::string()))
            .prop("theme_connections", Schema::array(Schema::string()))
            .prop("plot_connections", Schema::array(Schema::string()))
            .prop("contradictions", Schema::array(Schema::string()))
            .prop("foreshadowing", Schema::array(Schema::string()))
            .build()
    }

    pub fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("CrossReference schema is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_writing_analysis_schema() {
        let grammar = WritingAnalysis::grammar();
        assert!(grammar.contains("themes"));
        assert!(grammar.contains("characters"));
        assert!(grammar.contains("tone"));
    }

    #[test]
    fn test_writing_suggestion_schema() {
        let grammar = WritingSuggestion::grammar();
        assert!(grammar.contains("suggestion_type"));
        assert!(grammar.contains("text"));
        assert!(grammar.contains("confidence"));
    }

    #[test]
    fn test_diff_analysis_schema() {
        let grammar = DiffAnalysis::grammar();
        assert!(grammar.contains("summary"));
        assert!(grammar.contains("change_types"));
        assert!(grammar.contains("story_impact"));
    }

    #[test]
    fn test_cross_reference_schema() {
        let grammar = CrossReference::grammar();
        assert!(grammar.contains("character_refs"));
        assert!(grammar.contains("contradictions"));
        assert!(grammar.contains("foreshadowing"));
    }
}
