//! Story idea generation — brainstorming and premise expansion.
//!
//! Provides a `StoryIdeaGenerator` that can generate creative story ideas
//! and expand premises into full outlines, using state-tuned model calls
//! (no grammar constraint — free-form creative output).

use roco_engine::{CompletionRequest, ModelBackend};
use serde::Deserialize;

/// A single story idea with key elements.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoryIdea {
    pub title: String,
    pub genre: String,
    pub tone: String,
    pub premise: String,
    pub protagonist: String,
    pub central_conflict: String,
    pub suggested_chapters: Vec<String>,
    pub themes: Vec<String>,
}

/// Generator for story ideas and premise expansion.
///
/// Uses the model with state-tuned prompting (no BNF grammar) to allow
/// creative freedom while still producing structured JSON output.
pub struct StoryIdeaGenerator {
    /// Temperature for brainstorming (higher = more creative).
    pub brainstorm_temperature: f32,
    /// Temperature for premise expansion (lower = more structured).
    pub expand_temperature: f32,
    /// Max tokens for brainstorm output.
    pub max_tokens: usize,
}

impl Default for StoryIdeaGenerator {
    fn default() -> Self {
        Self {
            brainstorm_temperature: 0.85,
            expand_temperature: 0.6,
            max_tokens: 800,
        }
    }
}

impl StoryIdeaGenerator {
    /// Generate a set of story ideas from a prompt.
    ///
    /// Returns a vector of `StoryIdea` structs. The model is free to be
    /// creative — no grammar constraint, just state-tuned JSON prompting.
    pub fn brainstorm(
        &self,
        backend: &dyn ModelBackend,
        prompt: &str,
    ) -> Result<Vec<StoryIdea>, String> {
        #[derive(Deserialize)]
        struct BrainstormResponse {
            ideas: Vec<StoryIdea>,
        }

        let system = "You are a creative writing assistant. Generate creative story ideas. \
                      Output valid JSON only. No thinking, no reasoning, only JSON.";

        let full_prompt = format!(
            "Generate up to 3 creative story ideas based on this prompt: {prompt}\n\n\
             For each idea, provide: title, genre, tone, premise, protagonist, \
             central_conflict, suggested_chapters (array of chapter titles), themes (array of strings).\n\n\
             Output JSON with an 'ideas' array.",
        );

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.to_string(),
            prompt: full_prompt,
            grammar: None, // State-tuned: no grammar
            temperature: self.brainstorm_temperature,
            max_tokens: self.max_tokens,
            prefill: Some("{\n".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        let cleaned = roco_grammar::strategies::clean_json_output(&text);
        let response: BrainstormResponse = serde_json::from_str(&cleaned)
            .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))?;

        Ok(response.ideas)
    }

    /// Expand a premise into a full story idea with chapter outline.
    ///
    /// Returns a single `StoryIdea` with detailed suggested chapters.
    pub fn expand_premise(
        &self,
        backend: &dyn ModelBackend,
        premise: &str,
    ) -> Result<StoryIdea, String> {
        let system = "You expand story premises into detailed outlines. \
                      Output valid JSON only. No thinking.";

        let prompt = format!(
            "Expand this premise into a detailed story outline:\n\n{premise}\n\n\
             Output JSON with: title, genre, tone, premise, protagonist, \
             central_conflict, suggested_chapters (array of ~5 chapter descriptions), \
             themes (array of strings).",
        );

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.to_string(),
            prompt,
            grammar: None, // State-tuned: no grammar
            temperature: self.expand_temperature,
            max_tokens: self.max_tokens,
            prefill: Some("{\n".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        let cleaned = roco_grammar::strategies::clean_json_output(&text);
        serde_json::from_str::<StoryIdea>(&cleaned)
            .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;

    #[test]
    fn test_brainstorm_idea_schema() {
        // Just verify the types are well-formed
        let idea = StoryIdea {
            title: "Test".into(),
            genre: "Fantasy".into(),
            tone: "Light".into(),
            premise: "A story.".into(),
            protagonist: "Hero".into(),
            central_conflict: "Good vs Evil".into(),
            suggested_chapters: vec!["Chapter 1".into()],
            themes: vec!["Courage".into()],
        };
        let json = serde_json::to_value(&idea).unwrap();
        assert_eq!(json["title"], "Test");
        assert_eq!(json["themes"][0], "Courage");
    }

    #[test]
    fn test_generator_default_config() {
        let gen = StoryIdeaGenerator::default();
        assert_eq!(gen.brainstorm_temperature, 0.85);
        assert_eq!(gen.expand_temperature, 0.6);
    }
}
