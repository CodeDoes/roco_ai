//! Story idea generation — brainstorming and premise expansion.
//!
//! Provides a `StoryIdeaGenerator` that can generate creative story ideas
//! and expand premises into full outlines, using state-tuned model calls
//! (no grammar constraint — free-form creative output).

use roco_engine::{CompletionRequest, ModelBackend};
use serde::{Deserialize, Serialize};

/// Predefined literary genres with distinct tropes, settings, and guidelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenreTemplate {
    SciFi,
    Mystery,
    Fantasy,
    Romance,
    Horror,
    Thriller,
}

impl GenreTemplate {
    /// Associated tropes for each genre to seed brainstorming.
    pub fn tropes(&self) -> &'static [&'static str] {
        match self {
            Self::SciFi => &[
                "First Contact",
                "Dystopian Megacity",
                "AI Awakening",
                "Time Paradox",
                "Cybernetic Augmentation",
            ],
            Self::Mystery => &[
                "Locked Room Murder",
                "Unreliable Narrator",
                "Red Herrings",
                "Cold Case",
                "Dual Timeline Inquiry",
            ],
            Self::Fantasy => &[
                "Ancient Prophecy",
                "Lost Artifact of Power",
                "Hidden Magic Academy",
                "Decline of an Empire",
                "Bonded Mythical Beast",
            ],
            Self::Romance => &[
                "Enemies to Lovers",
                "Fake Relationship",
                "Forced Proximity",
                "Grumpy & Sunshine",
                "Second Chance at Love",
            ],
            Self::Horror => &[
                "Haunted Architecture",
                "Cosmic Dread",
                "Folk Magic Curse",
                "Isolation in Wilderness",
                "Survival against Unseen Force",
            ],
            Self::Thriller => &[
                "Race against Time",
                "Conspiracy at the Highest Level",
                "Mistaken Identity",
                "Cat and Mouse Game",
                "Stolen Secrets",
            ],
        }
    }

    /// Descriptive guidelines for prompt styling.
    pub fn prompt_guideline(&self) -> String {
        format!(
            "Style this story with key tropes of the {:?} genre, such as: {}",
            self,
            self.tropes().join(", ")
        )
    }
}

/// Dynamic tone options for story flavor and emotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToneFlavor {
    Epic,
    Grimdark,
    Whimsical,
    Suspenseful,
    Cozy,
    Melancholic,
}

impl ToneFlavor {
    pub fn prompt_guideline(&self) -> &'static str {
        match self {
            Self::Epic => "Ensure the stakes are grand, world-spanning, and emotionally massive.",
            Self::Grimdark => "Style the world as gritty, morally gray, and with realistic, heavy consequences.",
            Self::Whimsical => "Focus on a sense of wonder, playful language, magical realism, and charm.",
            Self::Suspenseful => "Keep tension high, focus on psychological stakes, secrets, and slow-burn pacing.",
            Self::Cozy => "Focus on comfort, close-knit communities, warm interiors, and low-stakes conflicts.",
            Self::Melancholic => "Emphasize nostalgia, beautiful sadness, quiet moments of reflection, and lost worlds.",
        }
    }
}

/// Narrative frameworks for structuring outlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeStructure {
    ThreeAct,
    HeroJourney,
    FichteanCurve,
    Kishotenketsu,
}

impl NarrativeStructure {
    pub fn act_descriptions(&self) -> &'static [&'static str] {
        match self {
            Self::ThreeAct => &[
                "Act I: Setup, Inciting Incident, and Plot Point 1",
                "Act II: Rising Action, Midpoint, and Plot Point 2",
                "Act III: Climax and Resolution",
            ],
            Self::HeroJourney => &[
                "Departure: Call to Adventure, Supernatural Aid, Crossing the Threshold",
                "Initiation: Road of Trials, Meeting with the Goddess, Apotheosis",
                "Return: Magic Flight, Rescue from Without, Master of Two Worlds",
            ],
            Self::FichteanCurve => &[
                "Inciting Incident: Rapid introduction of main crisis",
                "Rising Action: Multiple consecutive crises with increasing tension",
                "Climax and Falling Action: Major confrontation and brief resolution",
            ],
            Self::Kishotenketsu => &[
                "Ki (Introduction): Introducing characters and status quo",
                "Shō (Development): Developing the situation without major change",
                "Ten (Twist): An unexpected or seemingly unrelated turn of events",
                "Ketsu (Reconciliation): Connecting the twist back to the main thread",
            ],
        }
    }

    pub fn prompt_guideline(&self) -> String {
        format!(
            "Align the suggested chapter structure with the {:?} narrative model: {}",
            self,
            self.act_descriptions().join(" -> ")
        )
    }
}

/// A highly-detailed, feature-packed representation of a story idea.
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
    /// Target audience / reader demographic (e.g., Young Adult, New Adult, Adult)
    pub target_audience: Option<String>,
    /// Pacing description (e.g., Fast-paced, Slow-burn, Balanced)
    pub pacing: Option<String>,
    /// Key twists / surprise elements
    pub key_twists: Option<Vec<String>>,
    /// Sub-plots / secondary narrative threads
    pub sub_plots: Option<Vec<String>>,
}

/// Validation result of a generated or expanded story idea.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IdeaValidationReport {
    pub is_valid: bool,
    pub score: f32,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
    pub structural_fit: String,
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
            max_tokens: 1024,
        }
    }
}

impl StoryIdeaGenerator {
    /// Generate a set of story ideas with custom genre, tone, and prompt options.
    pub fn brainstorm_advanced(
        &self,
        backend: &dyn ModelBackend,
        prompt: &str,
        genre: Option<GenreTemplate>,
        tone: Option<ToneFlavor>,
        structure: Option<NarrativeStructure>,
    ) -> Result<Vec<StoryIdea>, String> {
        #[derive(Deserialize)]
        struct BrainstormResponse {
            ideas: Vec<StoryIdea>,
        }

        let mut guidelines = Vec::new();
        if let Some(g) = genre {
            guidelines.push(g.prompt_guideline());
        }
        if let Some(t) = tone {
            guidelines.push(t.prompt_guideline().to_string());
        }
        if let Some(s) = structure {
            guidelines.push(s.prompt_guideline());
        }

        let guidelines_str = if guidelines.is_empty() {
            String::new()
        } else {
            format!("\nFormatting & Styling Guidelines:\n- {}", guidelines.join("\n- "))
        };

        let system = "You are an elite creative writing assistant. Generate innovative, feature-packed story ideas. \
                      Output valid JSON only. No thinking, no reasoning, only JSON.";

        let full_prompt = format!(
            "Generate up to 3 creative story ideas based on this prompt: {prompt}\
             {guidelines_str}\n\n\
             For each idea, provide a JSON object matching this schema:\n\
             {{\n\
               \"title\": \"String\",\n\
               \"genre\": \"String\",\n\
               \"tone\": \"String\",\n\
               \"premise\": \"String\",\n\
               \"protagonist\": \"String\",\n\
               \"central_conflict\": \"String\",\n\
               \"suggested_chapters\": [\"Chapter 1 details\", \"Chapter 2 details\", ...],\n\
               \"themes\": [\"theme1\", \"theme2\"],\n\
               \"target_audience\": \"e.g. Young Adult, Adult, Epic Fantasy Fans\",\n\
               \"pacing\": \"e.g. Fast-paced, Slow-burn\",\n\
               \"key_twists\": [\"twist1\", \"twist2\"],\n\
               \"sub_plots\": [\"sub_plot1\", \"sub_plot2\"]\n\
             }}\n\n\
             Output JSON with an 'ideas' array."
        );

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.to_string(),
            prompt: full_prompt,
            grammar: None, // State-tuned
            temperature: self.brainstorm_temperature,
            max_tokens: self.max_tokens,
            prefill: Some("{\n  \"ideas\": [".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        let cleaned = roco_grammar::strategies::clean_json_output(&text);
        let response: BrainstormResponse = serde_json::from_str(&cleaned)
            .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))?;

        Ok(response.ideas)
    }

    /// Backwards compatibility method for simple brainstorm.
    pub fn brainstorm(
        &self,
        backend: &dyn ModelBackend,
        prompt: &str,
    ) -> Result<Vec<StoryIdea>, String> {
        self.brainstorm_advanced(backend, prompt, None, None, None)
    }

    /// Expand a premise into a feature-packed story idea with customizable parameters.
    pub fn expand_premise_advanced(
        &self,
        backend: &dyn ModelBackend,
        premise: &str,
        genre: Option<GenreTemplate>,
        tone: Option<ToneFlavor>,
        structure: Option<NarrativeStructure>,
    ) -> Result<StoryIdea, String> {
        let mut guidelines = Vec::new();
        if let Some(g) = genre {
            guidelines.push(g.prompt_guideline());
        }
        if let Some(t) = tone {
            guidelines.push(t.prompt_guideline().to_string());
        }
        if let Some(s) = structure {
            guidelines.push(s.prompt_guideline());
        }

        let guidelines_str = if guidelines.is_empty() {
            String::new()
        } else {
            format!("\nFormatting & Styling Guidelines:\n- {}", guidelines.join("\n- "))
        };

        let system = "You expand story premises into highly-detailed, feature-packed outlines. \
                      Output valid JSON only. No thinking.";

        let prompt = format!(
            "Expand this premise into a detailed story outline:\n\n{premise}\
             {guidelines_str}\n\n\
             Output a JSON object matching this schema:\n\
             {{\n\
               \"title\": \"String\",\n\
               \"genre\": \"String\",\n\
               \"tone\": \"String\",\n\
               \"premise\": \"String\",\n\
               \"protagonist\": \"String\",\n\
               \"central_conflict\": \"String\",\n\
               \"suggested_chapters\": [\"Chapter 1: description\", ...],\n\
               \"themes\": [\"theme1\", ...],\n\
               \"target_audience\": \"String\",\n\
               \"pacing\": \"String\",\n\
               \"key_twists\": [\"twist1\", ...],\n\
               \"sub_plots\": [\"sub_plot1\", ...]\n\
             }}"
        );

        let text = futures::executor::block_on(backend.complete(CompletionRequest {
            system: system.to_string(),
            prompt,
            grammar: None, // State-tuned
            temperature: self.expand_temperature,
            max_tokens: self.max_tokens,
            prefill: Some("{\n  \"title\":".into()),
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?
        .text;

        let cleaned = roco_grammar::strategies::clean_json_output(&text);
        serde_json::from_str::<StoryIdea>(&cleaned)
            .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))
    }

    /// Backwards compatibility method for simple expand_premise.
    pub fn expand_premise(
        &self,
        backend: &dyn ModelBackend,
        premise: &str,
    ) -> Result<StoryIdea, String> {
        self.expand_premise_advanced(backend, premise, None, None, None)
    }

    /// Validate a story idea programmatically and against narrative principles.
    pub fn validate_idea(
        &self,
        idea: &StoryIdea,
        preferred_structure: Option<NarrativeStructure>,
    ) -> IdeaValidationReport {
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();
        let mut score: f32 = 10.0;

        // 1. Programmatic validation of lengths and fields
        if idea.title.trim().is_empty() || idea.title.to_lowercase().contains("untitled") {
            score -= 1.5;
            issues.push("Title is missing or contains generic 'Untitled' placeholder.".to_string());
            suggestions.push("Create a highly engaging, specific title for the story.".to_string());
        }

        if idea.premise.split_whitespace().count() < 15 {
            score -= 1.0;
            issues.push("Premise description is too brief (under 15 words).".to_string());
            suggestions.push("Expand the premise to explain the hook and unique world elements.".to_string());
        }

        if idea.suggested_chapters.len() < 3 {
            score -= 1.5;
            issues.push(format!(
                "Too few chapters (found {}, minimum recommended is 3 for an arc).",
                idea.suggested_chapters.len()
            ));
            suggestions.push("Flesh out the outline to span at least three major chapters/acts.".to_string());
        }

        if idea.themes.is_empty() {
            score -= 0.5;
            issues.push("No core themes identified.".to_string());
            suggestions.push("Identify 1-2 driving themes (e.g., identity, betrayal, hope) to ground the story.".to_string());
        }

        // 2. Twist & Subplot checks
        if let Some(ref twists) = idea.key_twists {
            if twists.is_empty() {
                score -= 0.5;
                issues.push("Twist array is empty.".to_string());
                suggestions.push("Add at least one dramatic twist or unexpected revelation.".to_string());
            }
        } else {
            score -= 0.5;
            issues.push("No key twists provided.".to_string());
            suggestions.push("Incorporate unexpected twists to elevate mystery and narrative tension.".to_string());
        }

        if let Some(ref subplots) = idea.sub_plots {
            if subplots.is_empty() {
                suggestions.push("Consider adding a subplot (romantic, personal growth, rivalries) to enrich the story.".to_string());
            }
        }

        // 3. Structural alignment evaluation
        let structural_fit = if let Some(structure) = preferred_structure {
            let act_count = structure.act_descriptions().len();
            let chapters_count = idea.suggested_chapters.len();
            if chapters_count % act_count != 0 && chapters_count < act_count {
                score -= 1.0;
                issues.push(format!(
                    "Chapter count ({}) is too small to fully cover the {:?} structure phases (requires {}).",
                    chapters_count, structure, act_count
                ));
                suggestions.push(format!(
                    "Add more chapters to properly transition through all required acts of the {:?}.",
                    structure
                ));
                format!("Poor alignment with {:?}", structure)
            } else {
                format!("Excellent alignment with {:?}", structure)
            }
        } else {
            "No specific structure requested; structure is open-ended.".to_string()
        };

        IdeaValidationReport {
            is_valid: score >= 6.0 && issues.is_empty(),
            score: score.max(0.0),
            issues,
            suggestions,
            structural_fit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brainstorm_idea_schema() {
        let idea = StoryIdea {
            title: "Test".into(),
            genre: "Fantasy".into(),
            tone: "Light".into(),
            premise: "A story of magical proportions.".into(),
            protagonist: "Hero".into(),
            central_conflict: "Good vs Evil".into(),
            suggested_chapters: vec!["Chapter 1".into(), "Chapter 2".into(), "Chapter 3".into()],
            themes: vec!["Courage".into()],
            target_audience: Some("Young Adult".into()),
            pacing: Some("Fast-paced".into()),
            key_twists: Some(vec!["The mentor was the villain!".into()]),
            sub_plots: Some(vec!["Reconciling with an estranged sibling.".into()]),
        };
        let json = serde_json::to_value(&idea).unwrap();
        assert_eq!(json["title"], "Test");
        assert_eq!(json["themes"][0], "Courage");
        assert_eq!(json["target_audience"], "Young Adult");
        assert_eq!(json["pacing"], "Fast-paced");
        assert_eq!(json["key_twists"][0], "The mentor was the villain!");
    }

    #[test]
    fn test_generator_default_config() {
        let gen = StoryIdeaGenerator::default();
        assert_eq!(gen.brainstorm_temperature, 0.85);
        assert_eq!(gen.expand_temperature, 0.6);
    }

    #[test]
    fn test_genre_templates() {
        let sci_fi = GenreTemplate::SciFi;
        assert!(sci_fi.tropes().contains(&"First Contact"));
        assert!(sci_fi.prompt_guideline().contains("Dystopian Megacity"));

        let mystery = GenreTemplate::Mystery;
        assert!(mystery.tropes().contains(&"Locked Room Murder"));
    }

    #[test]
    fn test_narrative_structures() {
        let three_act = NarrativeStructure::ThreeAct;
        assert!(three_act.prompt_guideline().contains("Act I"));

        let hero_journey = NarrativeStructure::HeroJourney;
        assert!(hero_journey.prompt_guideline().contains("Departure"));
    }

    #[test]
    fn test_validate_idea_perfect() {
        let gen = StoryIdeaGenerator::default();
        let idea = StoryIdea {
            title: "The Iron Nebula".into(),
            genre: "Sci-Fi".into(),
            tone: "Grimdark".into(),
            premise: "A scavenger crew deep in the outer rim uncovers a silent, ancient spaceship containing a dormant, world-ending artificial intelligence.".into(),
            protagonist: "Captain Silas Thorne".into(),
            central_conflict: "Silas must decide whether to destroy the ship or sell its secrets to save his crew.".into(),
            suggested_chapters: vec![
                "Chapter 1: The Signal".into(),
                "Chapter 2: Boarding the Titan".into(),
                "Chapter 3: The AI's Eye".into(),
            ],
            themes: vec!["Survival".into(), "Morality".into()],
            target_audience: Some("Adult".into()),
            pacing: Some("Suspenseful".into()),
            key_twists: Some(vec!["The crew's contractor is an android under the AI's control.".into()]),
            sub_plots: Some(vec!["Silas coming to terms with his past betrayal.".into()]),
        };

        let report = gen.validate_idea(&idea, Some(NarrativeStructure::ThreeAct));
        println!("REPORT: {:?}", report);
        assert!(report.is_valid);
        assert_eq!(report.score, 10.0);
        assert!(report.issues.is_empty());
        assert_eq!(report.structural_fit, "Excellent alignment with ThreeAct");
    }

    #[test]
    fn test_validate_idea_poor() {
        let gen = StoryIdeaGenerator::default();
        let poor_idea = StoryIdea {
            title: "Untitled Story".into(),
            genre: "Mystery".into(),
            tone: "Light".into(),
            premise: "Brief premise.".into(), // under 15 words
            protagonist: "Detective".into(),
            central_conflict: "Mystery".into(),
            suggested_chapters: vec!["Chapter 1".into()], // under 3 chapters
            themes: vec![], // empty
            target_audience: None,
            pacing: None,
            key_twists: None,
            sub_plots: None,
        };

        let report = gen.validate_idea(&poor_idea, Some(NarrativeStructure::HeroJourney));
        assert!(!report.is_valid);
        assert!(report.score < 8.0);
        assert!(report.issues.len() >= 4);
        assert!(report.suggestions.len() >= 4);
        assert_eq!(report.structural_fit, "Poor alignment with HeroJourney");
    }
}
