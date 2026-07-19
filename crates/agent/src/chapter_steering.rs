//! Chapter Steering — pause and redirect mid-generation.
//!
//! The human can steer a chapter while it's being generated:
//! - Pause generation
//! - Give direction
//! - Resume with new direction
//! - See what was generated so far

use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Steering State
// ═════════════════════════════════════════════════════════════════════════════

/// State of chapter generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GenerationState {
    /// Not started
    NotStarted,
    /// Currently generating
    Generating,
    /// Paused by human
    Paused,
    /// Completed
    Completed,
    /// Failed
    Failed,
}

/// A checkpoint in chapter generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationCheckpoint {
    /// How many words generated so far
    pub words_generated: usize,
    /// The text generated so far
    pub text_so_far: String,
    /// The last sentence (for context)
    pub last_sentence: String,
    /// Timestamp
    pub timestamp: u64,
}

// ═════════════════════════════════════════════════════════════════════════════
// Chapter Steerer
// ═════════════════════════════════════════════════════════════════════════════

/// Manages chapter generation with pause/resume/steer capabilities
pub struct ChapterSteerer {
    /// Current state
    state: GenerationState,
    /// Text generated so far
    text_buffer: String,
    /// Checkpoints for pause/resume
    checkpoints: Vec<GenerationCheckpoint>,
    /// Direction given while paused
    pending_direction: Option<String>,
    /// Target words
    target_words: usize,
}

impl ChapterSteerer {
    /// Create a new chapter steerer
    pub fn new(target_words: usize) -> Self {
        Self {
            state: GenerationState::NotStarted,
            text_buffer: String::new(),
            checkpoints: Vec::new(),
            pending_direction: None,
            target_words,
        }
    }

    /// Get current state
    pub fn state(&self) -> &GenerationState {
        &self.state
    }

    /// Get text generated so far
    pub fn text_so_far(&self) -> &str {
        &self.text_buffer
    }

    /// Get word count
    pub fn word_count(&self) -> usize {
        self.text_buffer.split_whitespace().count()
    }

    /// Get progress (0-1)
    pub fn progress(&self) -> f32 {
        if self.target_words == 0 {
            return 1.0;
        }
        (self.word_count() as f32 / self.target_words as f32).min(1.0)
    }

    /// Check if generation is complete
    pub fn is_complete(&self) -> bool {
        self.state == GenerationState::Completed
    }

    /// Start generation
    pub fn start(&mut self) {
        self.state = GenerationState::Generating;
        self.text_buffer.clear();
        self.checkpoints.clear();
        self.pending_direction = None;
    }

    /// Add generated text (called as tokens stream in)
    pub fn add_text(&mut self, text: &str) {
        if self.state != GenerationState::Generating {
            return;
        }

        self.text_buffer.push_str(text);

        // Create checkpoint every 100 words
        let word_count = self.word_count();
        if word_count % 100 == 0 && word_count > 0 {
            self.checkpoints.push(GenerationCheckpoint {
                words_generated: word_count,
                text_so_far: self.text_buffer.clone(),
                last_sentence: self.get_last_sentence(),
                timestamp: now(),
            });
        }
    }

    /// Mark generation as complete
    pub fn complete(&mut self) {
        self.state = GenerationState::Completed;
        self.checkpoints.push(GenerationCheckpoint {
            words_generated: self.word_count(),
            text_so_far: self.text_buffer.clone(),
            last_sentence: self.get_last_sentence(),
            timestamp: now(),
        });
    }

    /// Pause generation (human wants to steer)
    pub fn pause(&mut self) {
        if self.state == GenerationState::Generating {
            self.state = GenerationState::Paused;
        }
    }

    /// Give direction while paused
    pub fn steer(&mut self, direction: &str) {
        if self.state == GenerationState::Paused {
            self.pending_direction = Some(direction.to_string());
        }
    }

    /// Resume generation with pending direction
    pub fn resume(&mut self) -> Option<String> {
        if self.state == GenerationState::Paused {
            self.state = GenerationState::Generating;
            self.pending_direction.take()
        } else {
            None
        }
    }

    /// Get the last sentence (for context)
    fn get_last_sentence(&self) -> String {
        self.text_buffer
            .split(['.', '!', '?'])
            .last()
            .unwrap_or("")
            .trim()
            .to_string()
    }

    /// Get a preview of what's been generated
    pub fn preview(&self, max_chars: usize) -> String {
        if self.text_buffer.len() <= max_chars {
            self.text_buffer.clone()
        } else {
            format!("{}...", &self.text_buffer[..max_chars])
        }
    }

    /// Get status message
    pub fn status_message(&self) -> String {
        match &self.state {
            GenerationState::NotStarted => "Not started".to_string(),
            GenerationState::Generating => {
                format!(
                    "Generating... ({}/{} words)",
                    self.word_count(),
                    self.target_words
                )
            }
            GenerationState::Paused => {
                format!(
                    "Paused at {} words. Give direction or [r]esume",
                    self.word_count()
                )
            }
            GenerationState::Completed => {
                format!("Complete ({} words)", self.word_count())
            }
            GenerationState::Failed => "Failed".to_string(),
        }
    }

    /// Get the prompt for when paused
    pub fn pause_prompt(&self) -> String {
        format!(
            "Chapter generation paused at {}/{} words.\n\n\
             What's been generated so far:\n{}\n\n\
             What would you like to do?\n\
             [d] Give direction for continuation\n\
             [r] Resume without changes\n\
             [s] Stop and keep what's generated\n\
             [a] Abort and discard",
            self.word_count(),
            self.target_words,
            self.preview(500)
        )
    }
}

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
    fn test_chapter_steerer_lifecycle() {
        let mut steerer = ChapterSteerer::new(100);

        // Start
        steerer.start();
        assert_eq!(*steerer.state(), GenerationState::Generating);

        // Add text
        steerer.add_text("The knight drew his sword. ");
        assert_eq!(steerer.word_count(), 6);
        assert!(steerer.progress() > 0.0);

        // Pause
        steerer.pause();
        assert_eq!(*steerer.state(), GenerationState::Paused);

        // Steer
        steerer.steer("Make it more dramatic");
        assert!(steerer.pending_direction.is_some());

        // Resume
        let direction = steerer.resume();
        assert_eq!(direction, Some("Make it more dramatic".to_string()));
        assert_eq!(*steerer.state(), GenerationState::Generating);

        // Complete
        steerer.complete();
        assert_eq!(*steerer.state(), GenerationState::Completed);
        assert!(steerer.is_complete());
    }

    #[test]
    fn test_preview() {
        let mut steerer = ChapterSteerer::new(100);
        steerer.start();
        steerer.add_text("This is a test sentence. And another one.");

        let preview = steerer.preview(20);
        assert!(preview.len() <= 23); // 20 + "..."
    }

    #[test]
    fn test_status_message() {
        let mut steerer = ChapterSteerer::new(100);
        steerer.start();
        steerer.add_text("Test");

        let msg = steerer.status_message();
        assert!(msg.contains("Generating"));
        assert!(msg.contains("1/100"));
    }
}
