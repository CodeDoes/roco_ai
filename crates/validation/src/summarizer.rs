//! Story summarization — generates condensed summaries from chapters, wiki, and outline.
//!
//! The `StorySummarizer` combines classic data (word counts, character lists)
//! with model-generated synopses and arc status to produce a `StorySummary`.
//!
//! # Usage
//!
//! ```ignore
//! let summarizer = StorySummarizer::new(backend);
//! let summary = summarizer.summarize_story(&chapters, &wiki, &outline);
//! println!("{}", summary.synopsis);
//! ```

use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::Schema;
use serde::{Deserialize, Serialize};

/// Complete story summary combining classic data and inference-backed content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorySummary {
    pub title: String,
    pub genre: String,
    pub chapter_count: usize,
    pub total_word_count: usize,
    pub characters: Vec<String>,
    pub synopsis: String,
    pub arc_status: String,
    pub latest_chapter_preview: String,
    pub last_updated: String,
}

impl StorySummary {
    /// Schema for grammar-constrained generation of synopsis and arc status.
    pub fn synopsis_schema() -> Schema {
        Schema::object()
            .prop("synopsis", Schema::string())
            .prop(
                "arc_status",
                Schema::enum_values(vec![
                    serde_json::json!("beginning"),
                    serde_json::json!("middle"),
                    serde_json::json!("end"),
                    serde_json::json!("complete"),
                ]),
            )
            .build()
    }
}

/// Inference-backed story summarizer.
pub struct StorySummarizer {
    /// Reference to the model backend for inference calls.
    backend: Option<Box<dyn ModelBackend>>,
    /// Temperature for synopsis generation.
    pub temperature: f32,
    /// Max tokens for synopsis output.
    pub max_tokens: usize,
}

impl StorySummarizer {
    /// Create a new story summarizer with a model backend.
    pub fn new(backend: Option<Box<dyn ModelBackend>>) -> Self {
        Self {
            backend,
            temperature: 0.5,
            max_tokens: 400,
        }
    }

    /// Generate a complete story summary.
    ///
    /// # Arguments
    ///
    /// * `chapter_texts` - Full text of all chapters, in order.
    /// * `wiki_text` - Full wiki/world-building text (may be empty).
    /// * `outline_text` - Full outline text (may be empty).
    /// * `title` - Story title (extracted from outline or provided separately).
    /// * `genre` - Story genre.
    ///
    /// Returns a `StorySummary` with classic fields always filled and
    /// inference-backed fields populated if a backend is available.
    pub fn summarize_story(
        &self,
        chapter_texts: &[String],
        wiki_text: &str,
        outline_text: &str,
        title: &str,
        genre: &str,
    ) -> StorySummary {
        let chapter_count = chapter_texts.len();
        let total_word_count: usize = chapter_texts
            .iter()
            .map(|c| c.split_whitespace().count())
            .sum();

        // Extract character names from wiki (basic parsing)
        let characters = self.extract_character_names(wiki_text);

        // Get latest chapter preview (last chapter, first 200 chars)
        let latest_chapter_preview = chapter_texts
            .last()
            .map(|c| {
                let cleaned = c.trim();
                let preview: String = cleaned.chars().take(200).collect();
                if cleaned.len() > 200 {
                    format!("{preview}...")
                } else {
                    preview
                }
            })
            .unwrap_or_default();

        // Timestamp
        let last_updated = chrono_now();

        // Inference-backed fields
        let (synopsis, arc_status) = if let Some(backend) = &self.backend {
            self.generate_synopsis_and_arc(backend.as_ref(), chapter_texts, outline_text)
        } else {
            (String::new(), "unknown".to_string())
        };

        StorySummary {
            title: title.to_string(),
            genre: genre.to_string(),
            chapter_count,
            total_word_count,
            characters,
            synopsis,
            arc_status,
            latest_chapter_preview,
            last_updated,
        }
    }

    /// Generate synopsis and arc status using the model.
    fn generate_synopsis_and_arc(
        &self,
        backend: &dyn ModelBackend,
        chapters: &[String],
        outline: &str,
    ) -> (String, String) {
        // Build a condensed chapter list for the prompt
        let chapter_summaries: Vec<String> = chapters
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let words: Vec<&str> = text.split_whitespace().collect();
                let preview: String = words
                    .iter()
                    .take(100)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("Chapter {}: {preview}...", i + 1)
            })
            .collect();

        let chapters_str = chapter_summaries.join("\n\n");

        let prompt = format!(
            "Based on the following story, write a 3-5 paragraph synopsis and determine the arc status.\n\
             \nOutline: {outline}\n\nChapters:\n{chapters_str}\n\n\
             Output JSON with:\n\
             - synopsis: A 3-5 paragraph summary of the entire story.\n\
             - arc_status: One of \"beginning\", \"middle\", \"end\", or \"complete\".",
        );

        #[derive(Deserialize)]
        struct SynopsisResponse {
            synopsis: String,
            arc_status: String,
        }

        let grammar = StorySummary::synopsis_schema()
            .to_gbnf("SynopsisResponse")
            .ok();

        let completion_result = futures::executor::block_on(
            backend.complete(CompletionRequest {
                system: "You are a literary summarizer. Output valid JSON only. \
                     Do NOT include thinking, reasoning, or meta-commentary."
                    .to_string(),
                prompt,
                grammar,
                temperature: self.temperature,
                max_tokens: self.max_tokens,
                prefill: Some("{\n".into()),
                ..Default::default()
            }),
        );

        let result: Result<SynopsisResponse, String> = match completion_result {
            Ok(response) => {
                let text = response.text;
                let cleaned = roco_grammar::strategies::clean_json_output(&text);
                serde_json::from_str::<SynopsisResponse>(&cleaned)
                    .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))
            }
            Err(e) => Err(format!("model error: {e}")),
        };

        match result {
            Ok(r) => (r.synopsis, r.arc_status),
            Err(e) => {
                tracing::warn!("Synopsis generation failed: {e}");
                (String::new(), "unknown".to_string())
            }
        }
    }

    /// Basic character name extraction from wiki text.
    fn extract_character_names(&self, wiki_text: &str) -> Vec<String> {
        crate::condensed::CondensedWiki::from_md(wiki_text).character_names()
    }
}

/// Get current timestamp as a formatted string.
fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple formatting: YYYY-MM-DD HH:MM:SS
    let h = (now / 3600) % 24;
    let m = (now / 60) % 60;
    let s = now % 60;
    let days = now / 86400;
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };
    format!("{yr:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_story_summary_basic_fields() {
        let chapters = vec![
            "Chapter one content here. It tells a story of adventure.".to_string(),
            "Chapter two continues the journey. The hero faces challenges.".to_string(),
        ];
        let wiki = "## Characters\n### Alice\nBrave hero\n### Bob\nWise mentor\n";
        let outline = "A story about adventure.";

        let summarizer = StorySummarizer::new(None);
        let summary =
            summarizer.summarize_story(&chapters, wiki, outline, "Adventure Story", "fantasy");

        assert_eq!(summary.title, "Adventure Story");
        assert_eq!(summary.genre, "fantasy");
        assert_eq!(summary.chapter_count, 2);
        assert!(summary.total_word_count > 0);
        assert_eq!(summary.characters.len(), 2);
        assert!(summary.characters.contains(&"Alice".to_string()));
    }

    #[test]
    fn test_character_name_extraction() {
        let wiki =
            "## Characters\n### Alice\nBrave hero\n### Bob\nWise mentor\n### Charlie\nSidekick\n";
        let summarizer = StorySummarizer::new(None);
        let names = summarizer.extract_character_names(wiki);
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"Alice".to_string()));
        assert!(names.contains(&"Bob".to_string()));
        assert!(names.contains(&"Charlie".to_string()));
    }

    #[test]
    fn test_story_summary_empty_chapters() {
        let summarizer = StorySummarizer::new(None);
        let summary = summarizer.summarize_story(&[], "", "", "Empty", "unknown");
        assert_eq!(summary.chapter_count, 0);
        assert_eq!(summary.total_word_count, 0);
        assert_eq!(summary.total_word_count, 0);
    }

    #[test]
    fn test_synopsis_schema_is_valid() {
        let schema = StorySummary::synopsis_schema();
        let gbnf = schema.to_gbnf("SynopsisResponse");
        assert!(
            gbnf.is_ok(),
            "Schema should convert to GBNF: {:?}",
            gbnf.err()
        );
    }
}
