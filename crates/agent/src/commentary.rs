//! Commentary — agent-generated explanations for artifacts.
//!
//! Every artifact the agent creates includes commentary explaining:
//! - Why certain decisions were made
//! - What alternatives were considered
//! - What trade-offs were made
//! - What the human should review
//!
//! This makes the agent's reasoning transparent and debuggable.

use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Commentary Types
// ═════════════════════════════════════════════════════════════════════════════

/// Commentary attached to an artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commentary {
    /// Who authored this commentary: "agent" or "human"
    pub author: String,
    /// What was done
    pub action: String,
    /// Why it was done this way
    pub reasoning: String,
    /// Alternatives that were considered
    pub alternatives: Vec<Alternative>,
    /// Trade-offs made
    pub trade_offs: Vec<TradeOff>,
    /// What the human should review (agent) or what needs attention (human)
    pub review_points: Vec<String>,
    /// Human's verdict: approved, rejected, needs_changes, pending
    pub verdict: Option<String>,
    /// Human's notes
    pub human_notes: Vec<String>,
    /// Confidence (0-1)
    pub confidence: f32,
    /// Timestamp
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    /// Description of the alternative
    pub description: String,
    /// Why it was not chosen
    pub reason_rejected: String,
    /// When it might be preferable
    pub when_preferable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeOff {
    /// What was gained
    pub gained: String,
    /// What was lost
    pub lost: String,
    /// Why this trade-off was made
    pub reasoning: String,
}

impl Commentary {
    /// Create a new agent commentary
    pub fn new(action: &str, reasoning: &str) -> Self {
        Self {
            author: "agent".to_string(),
            action: action.to_string(),
            reasoning: reasoning.to_string(),
            alternatives: Vec::new(),
            trade_offs: Vec::new(),
            review_points: Vec::new(),
            verdict: None,
            human_notes: Vec::new(),
            confidence: 0.8,
            timestamp: now(),
        }
    }

    /// Create a new human commentary
    pub fn human(action: &str, reasoning: &str) -> Self {
        Self {
            author: "human".to_string(),
            action: action.to_string(),
            reasoning: reasoning.to_string(),
            alternatives: Vec::new(),
            trade_offs: Vec::new(),
            review_points: Vec::new(),
            verdict: None,
            human_notes: Vec::new(),
            confidence: 1.0,
            timestamp: now(),
        }
    }

    /// Add an alternative
    pub fn with_alternative(mut self, description: &str, reason_rejected: &str, when_preferable: &str) -> Self {
        self.alternatives.push(Alternative {
            description: description.to_string(),
            reason_rejected: reason_rejected.to_string(),
            when_preferable: when_preferable.to_string(),
        });
        self
    }

    /// Add a trade-off
    pub fn with_trade_off(mut self, gained: &str, lost: &str, reasoning: &str) -> Self {
        self.trade_offs.push(TradeOff {
            gained: gained.to_string(),
            lost: lost.to_string(),
            reasoning: reasoning.to_string(),
        });
        self
    }

    /// Add a review point
    pub fn with_review_point(mut self, point: &str) -> Self {
        self.review_points.push(point.to_string());
        self
    }

    /// Set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set human verdict
    pub fn with_verdict(mut self, verdict: &str) -> Self {
        self.verdict = Some(verdict.to_string());
        self
    }

    /// Add human note
    pub fn with_human_note(mut self, note: &str) -> Self {
        self.human_notes.push(note.to_string());
        self
    }

    /// Format as markdown comment block
    pub fn to_markdown_comment(&self) -> String {
        let mut comment = String::new();

        comment.push_str(&format!("<!-- COMMENTARY [{}]\n", self.author.to_uppercase()));
        comment.push_str(&format!("Action: {}\n", self.action));
        comment.push_str(&format!("Reasoning: {}\n", self.reasoning));

        if !self.alternatives.is_empty() {
            comment.push_str(&format!("\nAlternatives considered:\n"));
            for alt in &self.alternatives {
                comment.push_str(&format!("- {}\n", alt.description));
                comment.push_str(&format!("  Rejected: {}\n", alt.reason_rejected));
                comment.push_str(&format!("  When preferable: {}\n", alt.when_preferable));
            }
        }

        if !self.trade_offs.is_empty() {
            comment.push_str(&format!("\nTrade-offs:\n"));
            for trade in &self.trade_offs {
                comment.push_str(&format!("- Gained: {}\n", trade.gained));
                comment.push_str(&format!("  Lost: {}\n", trade.lost));
                comment.push_str(&format!("  Reasoning: {}\n", trade.reasoning));
            }
        }

        if !self.review_points.is_empty() {
            comment.push_str(&format!("\nPlease review:\n"));
            for point in &self.review_points {
                comment.push_str(&format!("- {}\n", point));
            }
        }

        if let Some(ref verdict) = self.verdict {
            comment.push_str(&format!("\nVerdict: {}\n", verdict));
        }

        if !self.human_notes.is_empty() {
            comment.push_str(&format!("\nHuman notes:\n"));
            for note in &self.human_notes {
                comment.push_str(&format!("- {}\n", note));
            }
        }

        comment.push_str(&format!("\nConfidence: {:.0}%\n", self.confidence * 100.0));
        comment.push_str(&format!("Timestamp: {}\n", self.timestamp));
        comment.push_str(&format!("-->\n"));

        comment
    }

    /// Format as JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Artifact Commentary
// ═════════════════════════════════════════════════════════════════════════════

/// Commentary specifically for story artifacts
pub struct StoryCommentary;

impl StoryCommentary {
    /// Commentary for outline generation
    pub fn outline(premise: &str, chapter_count: usize) -> Commentary {
        Commentary::new(
            "Generated story outline",
            &format!("Created {} chapters based on premise: {}", chapter_count, premise),
        )
        .with_alternative(
            "More chapters with shorter arcs",
            "Would fragment the narrative",
            "When writing episodic content",
        )
        .with_alternative(
            "Fewer chapters with longer arcs",
            "Would make chapters too dense",
            "When writing novellas",
        )
        .with_trade_off(
            "Clear narrative structure",
            "Less flexibility for organic growth",
            "Structure helps maintain coherence in long stories",
        )
        .with_review_point("Check if the chapter count feels right for the story scope")
        .with_review_point("Verify the arc progression makes sense")
        .with_confidence(0.7)
    }

    /// Commentary for chapter generation
    pub fn chapter(chapter_num: usize, title: &str, word_count: usize) -> Commentary {
        Commentary::new(
            &format!("Generated Chapter {}: {}", chapter_num, title),
            &format!("Wrote {} words following the outline", word_count),
        )
        .with_trade_off(
            "Consistent pacing and tone",
            "May lack surprising twists",
            "Consistency is more important than surprise in early drafts",
        )
        .with_review_point("Check if the chapter advances the plot")
        .with_review_point("Verify character voices are distinct")
        .with_review_point("Look for any contradictions with previous chapters")
        .with_confidence(0.6)
    }

    /// Commentary for plot state extraction
    pub fn plot_state(chapter_num: usize) -> Commentary {
        Commentary::new(
            &format!("Extracted plot state after Chapter {}", chapter_num),
            "Identified characters, conflicts, and foreshadowing",
        )
        .with_trade_off(
            "Structured tracking of story elements",
            "May miss subtle nuances",
            "Explicit tracking prevents plot holes",
        )
        .with_review_point("Verify all characters are tracked")
        .with_review_point("Check if conflicts are accurately captured")
        .with_confidence(0.75)
    }

    /// Commentary for quality evaluation
    pub fn quality_evaluation(chapter_num: usize, score: f32) -> Commentary {
        Commentary::new(
            &format!("Evaluated quality of Chapter {}", chapter_num),
            &format!("Overall score: {:.1}/10", score),
        )
        .with_review_point("Check if the evaluation criteria match your standards")
        .with_review_point("Consider if the issues identified are valid")
        .with_confidence(0.5)
    }

    /// Commentary for revision
    pub fn revision(chapter_num: usize, reason: &str) -> Commentary {
        Commentary::new(
            &format!("Revised Chapter {}", chapter_num),
            &format!("Revised because: {}", reason),
        )
        .with_trade_off(
            "Improved quality based on feedback",
            "May have changed the original voice",
            "Preserved strengths while addressing weaknesses",
        )
        .with_review_point("Check if the revision preserves what you liked")
        .with_review_point("Verify the issues were actually fixed")
        .with_confidence(0.65)
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helper functions
// ═════════════════════════════════════════════════════════════════════════════

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commentary_markdown() {
        let commentary = Commentary::new(
            "Generated outline",
            "Created 3 chapters for a short story",
        )
        .with_alternative(
            "5 chapters",
            "Too many for a short story",
            "When writing a novella",
        )
        .with_review_point("Check chapter count")
        .with_confidence(0.8);

        let md = commentary.to_markdown_comment();
        assert!(md.contains("<!-- COMMENTARY [AGENT]"));
        assert!(md.contains("Generated outline"));
        assert!(md.contains("5 chapters"));
        assert!(md.contains("Check chapter count"));
        assert!(md.contains("Confidence: 80%"));
    }

    #[test]
    fn test_human_commentary() {
        let commentary = Commentary::human(
            "Reviewed outline",
            "Looks good but need more character development",
        )
        .with_verdict("needs_changes")
        .with_human_note("Add backstory for the antagonist")
        .with_human_note("Chapter 2 feels too short");

        let md = commentary.to_markdown_comment();
        assert!(md.contains("<!-- COMMENTARY [HUMAN]"));
        assert!(md.contains("Reviewed outline"));
        assert!(md.contains("Verdict: needs_changes"));
        assert!(md.contains("Add backstory for the antagonist"));
        assert!(md.contains("Chapter 2 feels too short"));
    }

    #[test]
    fn test_story_commentary() {
        let commentary = StoryCommentary::outline("A dark fantasy", 3);
        let md = commentary.to_markdown_comment();
        assert!(md.contains("Generated story outline"));
        assert!(md.contains("3 chapters"));
    }
}
