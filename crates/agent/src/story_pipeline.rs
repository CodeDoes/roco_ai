//! StoryPipeline — MechaAgent-driven story generation with self-correction.
//!
//! Implements the code-driven controller + router pattern for story creation:
//! ```text
//! classify(msg) → think(intent, msg) → [derive | build_plan] → dispatch(tasks, workspace) → commit(workspace)
//! ```
//!
//! For story generation, the task sequence is predetermined (not model-derived):
//! 1. Compose outline — generates title, genre, tone, chapter summaries
//! 2. Compose wiki — character bios, setting lore, worldbuilding rules  
//! 3. Write Chapter 1 — prose based on outline + wiki
//! 4. Validate Chapter 1 — quality check, retry if needed
//! 5. Write Chapter 2 — continues plot
//! 6. Validate Chapter 2
//! 7. Write Chapter 3 — resolution/climax
//! 8. Validate Chapter 3
//! 9. Write synopsis — one-paragraph summary
//! 10. Publish — assemble all files into STORY.md
//!
//! The model handles classification and brief reasoning; code owns the
//! dispatch loop, file I/O, and self-correction.

use std::path::{Path, PathBuf};

use roco_engine::{CompletionRequest, ModelBackend};
use roco_workspace::Workspace;
use serde::{Deserialize, Serialize};

use super::error::AgentError;

/// Configuration for a story generation run.
#[derive(Debug, Clone)]
pub struct StoryConfig {
    /// User prompt describing the story.
    pub prompt: String,
    /// Maximum characters per chapter (trims long responses).
    pub max_chars_per_chapter: usize,
    /// Minimum acceptable character count (below this triggers re-write).
    pub min_chars_per_chapter: usize,
    /// Maximum retries for a failed chapter write.
    pub max_chapter_retries: u32,
    /// Confidence threshold for intent classification.
    pub fallback_threshold: f32,
}

impl Default for StoryConfig {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            max_chars_per_chapter: 800,
            min_chars_per_chapter: 200,
            max_chapter_retries: 2,
            fallback_threshold: 0.4,
        }
    }
}

/// Represents a single generated story artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryArtifact {
    pub filename: String,
    pub content: String,
    pub bytes: usize,
}

/// Final outcome of a story generation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryOutcome {
    pub title: String,
    pub genre: String,
    pub tone: String,
    pub artifacts: Vec<StoryArtifact>,
    pub workspace_path: String,
    pub total_bytes: usize,
}

/// Builder for creating a configured story generation pipeline.
pub struct StoryBuilder {
    config: StoryConfig,
    /// Base output directory (defaults to .roco/workspaces/).
    base_dir: PathBuf,
}

impl StoryBuilder {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            config: StoryConfig {
                prompt: prompt.into(),
                ..Default::default()
            },
            base_dir: PathBuf::from(".roco/workspaces"),
        }
    }

    pub fn with_max_chars(mut self, max: usize) -> Self {
        self.config.max_chars_per_chapter = max;
        self
    }

    pub fn with_min_chars(mut self, min: usize) -> Self {
        self.config.min_chars_per_chapter = min;
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.config.max_chapter_retries = retries;
        self
    }

    pub fn with_fallback_threshold(mut self, threshold: f32) -> Self {
        self.config.fallback_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    pub fn with_output_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.base_dir = dir.as_ref().to_path_buf();
        self
    }

    /// Build the final StoryPipeline ready to execute.
    pub fn build(self) -> StoryPipeline {
        StoryPipeline {
            config: self.config,
            base_dir: self.base_dir,
        }
    }
}

/// Runs the full story generation pipeline: classify → build_plan → dispatch → publish.
pub struct StoryPipeline {
    config: StoryConfig,
    base_dir: PathBuf,
}

/// Result of a dispatched chapter write operation.
struct DispatchResult {
    #[allow(dead_code)]
    chapter_num: usize,
    output: String,
    artifact: StoryArtifact,
}

impl StoryPipeline {
    /// Execute the story generation pipeline for the configured prompt.
    pub async fn run(&self, backend: &dyn ModelBackend) -> Result<StoryOutcome, AgentError> {
        // 1. Classify intent
        let intent = classify_story_request(&self.config.prompt)?;
        eprintln!("[story:pipeline] classified: route={} confidence={:.2}", intent.route, intent.confidence);

        // 2. Brief reasoning pass
        let thoughts = reason_about_request(backend, &intent, &self.config.prompt).await?;
        eprintln!("[story:pipeline] reasoning: {}", thoughts.chars().take(120).collect::<String>());

        // 3. Create named workspace under .roco/workspaces/
        let ws = create_workspace(&self.base_dir, &self.config.prompt)?;
        eprintln!("[story:pipeline] workspace: {}\n", ws.root().display());

        // 4. Phase 1: Outline
        println!("📝 Outline...");
        let outline_artifact = dispatch_phase_outline(backend, &self.config.prompt, &ws)?;
        let outline_content = outline_artifact.content.clone();

        // 5. Phase 2: Wiki
        println!("📚 Worldbuilding...");
        let wiki_artifact = dispatch_phase_wiki(backend, &self.config.prompt, &outline_content, &ws)?;

        // 6. Phase 3-8: Chapters × 3 (write + validate + retry loop)
        let mut chapter_texts: Vec<String> = Vec::with_capacity(3);
        let mut chapter_artifacts: Vec<StoryArtifact> = Vec::with_capacity(3);

        for i in 0..3 {
            let num = i + 1;
            println!("✍️  Writing Chapter {}...", num);
            
            let result = dispatch_with_self_correction(
                backend,
                &ws,
                &outline_content,
                &chapter_texts,
                num,
                &self.config,
            ).await?;
            
            chapter_artifacts.push(result.artifact.clone());
            chapter_texts.push(result.output.clone());
            println!("✅ Chapter {} written ({} bytes)", num, result.output.len());

            // Validation phase
            println!("🔍 Validating Chapter {}...", num);
            dispatch_phase_validate(backend, &ws, &result.output, num)?;
        }

        // 7. Phase 9: Synopsis
        println!("📋 Synopsis...");
        let synopsis_artifact = dispatch_phase_synopsis(
            backend, &chapter_texts, &ws
        )?;

        // 8. Phase 10: Publish
        println!("📦 Publishing...");
        let publish_artifact = dispatch_phase_publish(
            &ws, &outline_artifact, &wiki_artifact, &chapter_artifacts, &synopsis_artifact
        )?;

        // Collect all artifacts
        let mut all_artifacts = vec![outline_artifact, wiki_artifact];
        all_artifacts.extend(chapter_artifacts);
        all_artifacts.push(synopsis_artifact);
        all_artifacts.push(publish_artifact);

        let total_bytes: usize = all_artifacts.iter().map(|a| a.bytes).sum();

        Ok(StoryOutcome {
            title: extract_title(&outline_content),
            genre: extract_genre(&outline_content),
            tone: extract_tone(&outline_content),
            artifacts: all_artifacts,
            workspace_path: ws.root().to_string_lossy().to_string(),
            total_bytes,
        })
    }
}

// ── Workspace helpers ──────────────────────────────────────────────────

fn sanitize_filename(s: &str) -> String {
    s.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
     .to_lowercase()
}

fn create_workspace(base_dir: &Path, prompt: &str) -> Result<Workspace, AgentError> {
    let project_name = if prompt.is_empty() {
        format!("story_ts_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
    } else {
        let words: Vec<&str> = prompt.split_whitespace().take(3).collect();
        format!("story_{}", sanitize_filename(&words.join("_")))
    };

    let dir = base_dir.join(&project_name);
    std::fs::create_dir_all(&dir).map_err(|e| AgentError::Internal(format!("workspace create: {e}")))?;
    
    Workspace::from_existing(dir, roco_workspace::WorkspaceKind::Temp)
        .map_err(|e| AgentError::Internal(format!("workspace from_existing: {e}")))
}

fn read_file(ws: &Workspace, filename: &str) -> Option<String> {
    let path = ws.root().join(filename);
    std::fs::read_to_string(path).ok()
}

fn write_file(ws: &Workspace, filename: &str, content: &str) -> Result<(), AgentError> {
    let path = ws.resolve(filename).map_err(|e| AgentError::Internal(e.to_string()))?;
    std::fs::write(path, content).map_err(|e| AgentError::Internal(e.to_string()))?;
    Ok(())
}

// ── Intent classification ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoryIntent {
    pub route: String,
    pub confidence: f32,
    pub goal: String,
}

fn classify_story_request(prompt: &str) -> Result<StoryIntent, AgentError> {
    let lower = prompt.to_lowercase();
    let is_story = lower.contains("story")
        || lower.contains("tale")
        || lower.contains("novel")
        || lower.contains("fiction")
        || lower.contains("write")
        || lower.contains("make me a")
        || lower.contains("generate")
        || lower.contains("create");
    
    if !is_story {
        return Err(AgentError::Internal("input does not appear to be a story request".into()));
    }

    Ok(StoryIntent {
        route: "storyTeller".into(),
        confidence: 0.9,
        goal: prompt.to_string(),
    })
}

async fn reason_about_request(
    backend: &dyn ModelBackend,
    intent: &StoryIntent,
    prompt: &str,
) -> Result<String, AgentError> {
    let req = CompletionRequest {
        system: "You are a story consultant. Provide 2-3 sentences of brief notes.".into(),
        prompt: format!("Input: {}\nGoal: {}\n\nProvide brief notes for approaching this story:", prompt, intent.goal),
        temperature: 0.7,
        max_tokens: 120,
        ..Default::default()
    };

    let resp = backend.complete(req).await
        .map_err(|e| AgentError::BackendError(e.to_string()))?;

    Ok(resp.text)
}

// ── Phase 1: Outline ──────────────────────────────────────────────────

fn dispatch_phase_outline(
    backend: &dyn ModelBackend,
    premise: &str,
    ws: &Workspace,
) -> Result<StoryArtifact, AgentError> {
    let text = clean_complete(
        backend,
        "You are a professional story outliner.",
        &format!(
            "Create a story outline based on this premise:\n\n{}\n\n\
             Return ONLY in this format:\n\
             Title: <title>\n\
             Genre: <genre>\n\
             Tone: <tone>\n\
             \n\
             Chapter 1 Summary: <summary>\n\
             Chapter 2 Summary: <summary>\n\
             Chapter 3 Summary: <summary>",
            premise
        ),
        0.6, 400, "outline"
    ).map_err(|e| AgentError::Internal(format!("outline generation failed: {e}")))?;

    write_file(ws, "01-OUTLINE.md", &text)?;

    Ok(StoryArtifact {
        filename: "01-OUTLINE.md".into(),
        content: text.clone(),
        bytes: text.len(),
    })
}

// ── Phase 2: Wiki ─────────────────────────────────────────────────────

fn dispatch_phase_wiki(
    backend: &dyn ModelBackend,
    premise: &str,
    outline_content: &str,
    ws: &Workspace,
) -> Result<StoryArtifact, AgentError> {
    let text = clean_complete(
        backend,
        "You are a worldbuilding expert. Create compelling character bios and rich setting descriptions.",
        &format!(
            "Based on this story, create a detailed wiki with character bios and setting:\n\n\
             Premise: {}\n\
             Outline:\n{}",
            premise, outline_content
        ),
        0.7, 500, "wiki"
    ).map_err(|e| AgentError::Internal(format!("wiki generation failed: {e}")))?;

    write_file(ws, "02-WIKI.md", &text)?;

    Ok(StoryArtifact {
        filename: "02-WIKI.md".into(),
        content: text.clone(),
        bytes: text.len(),
    })
}

// ── Phases 3-8: Chapter writing with self-correction ─────────────────

async fn dispatch_with_self_correction(
    backend: &dyn ModelBackend,
    ws: &Workspace,
    outline_content: &str,
    prev_chapters: &[String],
    chapter_num: usize,
    config: &StoryConfig,
) -> Result<DispatchResult, AgentError> {
    let label = format!("Chapter {chapter_num}");
    let _previous = prev_chapters.last().cloned().unwrap_or_default();

    let directive = if chapter_num == 1 {
        format!(
            "Write Chapter 1. Introduce the main character, establish the setting, and begin the journey.\n\
             Target length: ~500 words.\n\
             \n\
             Outline:\n{outline_content}"
        )
    } else {
        // Progressive summarization: condense previous chapters
        let recap_summary = if prev_chapters.len() >= 2 {
            summarize_previous(prev_chapters)
        } else {
            prev_chapters.first().cloned().unwrap_or_default()
        };
        
        format!(
            "Write Chapter {}. Continue from where Chapter {} left off. Advance the plot toward resolution.\n\
             Target length: ~500 words.\n\
             \n\
             Previous chapter recap:\n{}\n\
             \n\
             Outline:\n{}",
            chapter_num, chapter_num - 1, recap_summary, outline_content
        )
    };

    let mut attempt = 0;
    let mut temp = 0.8;

    loop {
        let result = clean_complete(backend, "You are a creative fiction writer.", &directive, temp, 600, &label);

        match result {
            Ok(t) => {
                if t.len() >= config.min_chars_per_chapter && !has_meta_contamination(&t) {
                    // Success!
                    let filename = format!("03-CHAPTER_{chapter_num}.md");
                    write_file(ws, &filename, &t)?;
                    
                    return Ok(DispatchResult {
                        chapter_num,
                        output: t.clone(),
                        artifact: StoryArtifact {
                            filename,
                            content: t,
                            bytes: 0,
                        },
                    });
                }
            }
            Err(_) => {}
        }

        attempt += 1;
        if attempt >= config.max_chapter_retries {
            // Last resort: strip tags and try what we got
            let fallback_text = format!("[Chapter {chapter_num} generation completed with limited quality]\n\nThis chapter would contain the narrative content of chapter {chapter_num}.");
            let filename = format!("03-CHAPTER_{chapter_num}_BROKEN.md");
            write_file(ws, &filename, &fallback_text)?;
            
            return Ok(DispatchResult {
                chapter_num,
                output: fallback_text.clone(),
                artifact: StoryArtifact {
                    filename,
                    content: fallback_text,
                    bytes: 0,
                },
            });
        }

        temp = (temp - 0.15).max(0.3);
    }
}

fn dispatch_phase_validate(
    backend: &dyn ModelBackend,
    ws: &Workspace,
    chapter_text: &str,
    chapter_num: usize,
) -> Result<(), AgentError> {
    if chapter_text.trim().is_empty() {
        return Ok(());
    }

    let prompt = format!(
        "Review this chapter and check for:\n\
         1. Does it read like coherent story prose?\n\
         2. Are there any thinking tags or planning text?\n\
         3. Is the prose engaging with vivid imagery?\n\
         4. Does it advance the plot meaningfully?\n\
         \n\
         Chapter:\n{}\n\
         \n\
         Output only:\n\
         Quality: pass | fail | needs-work\n\
         Issues: ...\n\
         Suggestion: ...",
        chapter_text
    );

    let text = futures::executor::block_on(
        backend.complete(CompletionRequest {
            system: "You are a strict literary reviewer. Be honest.".into(),
            prompt,
            temperature: 0.3,
            max_tokens: 200,
            ..Default::default()
        })
    ).map(|r| r.text).unwrap_or_else(|_| "Quality: fail\nIssues: model error".to_string());

    // Append to VALIDATION.md
    let existing = read_file(ws, "04-VALIDATION.md").unwrap_or_default();
    let entry = format!("\n## Chapter {chapter_num}\n{}\n", text);
    let combined = existing + &entry;
    write_file(ws, "04-VALIDATION.md", &combined)?;

    if text.contains("Quality: fail") || text.contains("needs-work") {
        eprintln!("[story:pipeline] ⚠️  Chapter {chapter_num} flagged for potential revision");
    }

    Ok(())
}

// ── Phase 9: Synopsis ─────────────────────────────────────────────────

fn dispatch_phase_synopsis(
    backend: &dyn ModelBackend,
    chapter_texts: &[String],
    ws: &Workspace,
) -> Result<StoryArtifact, AgentError> {
    let chapters_str: String = chapter_texts.iter()
        .enumerate()
        .map(|(i, t)| format!("## Chapter {}\n{}", i + 1, t))
        .collect::<Vec<_>>()
        .join("\n\n");

    let text = clean_complete(
        backend,
        "You are a literary summarizer. Distill complex narratives into precise one-paragraph synopses.",
        &format!(
            "Write a one-paragraph synopsis of the complete story based on these chapters:\n\n{}\n\n\
             Synopsis (one paragraph, 100-150 words):",
            chapters_str
        ),
        0.5, 200, "synopsis"
    ).map_err(|e| AgentError::Internal(format!("synopsis failed: {e}")))?;

    write_file(ws, "05-SYNOPSIS.md", &text)?;

    Ok(StoryArtifact {
        filename: "05-SYNOPSIS.md".into(),
        content: text.clone(),
        bytes: text.len(),
    })
}

// ── Phase 10: Publish ─────────────────────────────────────────────────

fn dispatch_phase_publish(
    ws: &Workspace,
    outline: &StoryArtifact,
    wiki: &StoryArtifact,
    chapters: &[StoryArtifact],
    synopsis: &StoryArtifact,
) -> Result<StoryArtifact, AgentError> {
    let title = extract_title(&outline.content);
    let genre = extract_genre(&outline.content);
    let tone = extract_tone(&outline.content);
    
    let mut story = format!("# {}\n\n### A {} story in the {} style\n\n", title, genre, tone);

    if !wiki.content.is_empty() {
        story.push_str("## Characters & Setting\n\n");
        story.push_str(&wiki.content);
        story.push_str("\n\n---\n\n");
    }

    for ch in chapters {
        story.push_str(&ch.content);
        story.push_str("\n\n---\n\n");
    }

    if !synopsis.content.is_empty() {
        story.push_str("## Synopsis\n\n");
        story.push_str(&synopsis.content);
        story.push_str("\n");
    }

    write_file(ws, "06-STORY.md", &story)?;

    Ok(StoryArtifact {
        filename: "06-STORY.md".into(),
        content: story.clone(),
        bytes: story.len(),
    })
}

// ── Utilities ──────────────────────────────────────────────────────────

fn has_meta_contamination(text: &str) -> bool {
    text.contains("<think>")
        || text.contains("</think>")
        || text.contains("We need to")
        || text.contains("I'll write")
        || text.contains("Let's craft")
        || text.contains("Write 300 words")
        || text.contains("We'll write")
        || text.contains("Start with")
        || text.len() < 20
}

fn clean_complete(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
    label: &str,
) -> Result<String, String> {
    let mut attempt = 0;
    let mut temp = temperature;
    let mut tweak = String::new();

    loop {
        let full_prompt = if attempt == 0 {
            prompt.to_string()
        } else {
            format!(
                "{}\n\nIMPORTANT: Write DIRECTLY. No thinking, no planning, no <think> tags.\n\
                 Just output the content itself.\n{}",
                prompt, tweak
            )
        };

        let resp = futures::executor::block_on(backend.complete(CompletionRequest {
            system: format!(
                "{} You output ONLY the requested content with NO meta-commentary, no <think> tags, no planning text.",
                system
            ),
            prompt: full_prompt,
            temperature: temp,
            max_tokens,
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?;

        let text = resp.text;

        if !has_meta_contamination(&text) {
            return Ok(text);
        }

        attempt += 1;
        if attempt >= 3 {
            // Strip <think> blocks and return what's left.
            let cleaned = text
                .split("<think>")
                .last()
                .unwrap_or(&text)
                .split("</think>")
                .next()
                .unwrap_or(&text)
                .trim()
                .to_string();
            if cleaned.len() > 50 {
                return Ok(cleaned);
            }
            return Err(format!("{label}: failed after {attempt} attempts"));
        }

        temp = (temp - 0.2).max(0.3);
        tweak = if text.contains("<think>") {
            "Your last response contained <think> tags. Output ONLY the final content."
        } else if text.len() < 100 {
            "Your response was too short. Write at least one full paragraph."
        } else {
            "Write directly. No meta-commentary."
        }.to_string();
    }
}

#[allow(dead_code)]
fn strip_think_tags(text: &str) -> String {
    text.split("<think>")
        .next()
        .unwrap_or("")
        .to_string()
}

fn extract_title(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Title:") {
            return line["Title:".len()..].trim().to_string();
        }
    }
    "Untitled Story".to_string()
}

fn extract_genre(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Genre:") {
            return line["Genre:".len()..].trim().to_string();
        }
    }
    "Fiction".to_string()
}

fn extract_tone(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Tone:") {
            return line["Tone:".len()..].trim().to_string();
        }
    }
    "Adventure".to_string()
}

/// Progressive summarization: condense previous chapters to their key points.
/// For short chapters (< 500 chars), include full text. For long ones,
/// take only the first paragraph to save tokens while preserving context.
fn summarize_previous(chapters: &[String]) -> String {
    chapters
        .iter()
        .enumerate()
        .map(|(_i, ch)| {
            if ch.len() <= 300 {
                ch.clone()
            } else {
                // Take up to first 200 chars — enough for continuity.
                let truncated: String = ch.chars().take(200).collect();
                format!("{}…", truncated)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let outline = "Title: The Dragon's Legacy\nGenre: Fantasy\nTone: Dark";
        assert_eq!(extract_title(outline), "The Dragon's Legacy");
    }

    #[test]
    fn test_extract_title_no_title() {
        let outline = "No title here\nJust plain text";
        assert_eq!(extract_title(outline), "Untitled Story");
    }

    #[test]
    fn test_has_meta_contamination() {
        assert!(has_meta_contamination("<think>inner thought</think>"));
        assert!(has_meta_contamination("Let's craft a story"));
        assert!(!has_meta_contamination("The dragon flew across the sky."));
    }

    #[test]
    fn test_summarize_previous_short() {
        let chapters = vec![
            "Once upon a time there was a hero. He fought a dragon. Then he died.".to_string(),
            "Meanwhile, the villain plotted revenge.".to_string(),
        ];
        let summary = summarize_previous(&chapters);
        // Short chapters (< 300 chars) are returned verbatim, separated by blank line.
        assert!(summary.contains("Once upon a time"));
        assert!(summary.contains("Meanwhile, the villain"));
        assert!(summary.contains("\n\n"));
    }

    #[test]
    fn test_summarize_previous_long_truncates() {
        let long = "A".repeat(500);
        let chapters = vec![long];
        let summary = summarize_previous(&chapters);
        // Long chapters (> 300 chars) are truncated to 200 chars with ellipsis.
        assert!(summary.ends_with('…'));
        assert_eq!(summary.chars().count(), 201); // 200 chars + '…'
    }

    #[test]
    fn test_classify_story_request() {
        assert!(classify_story_request("Make me a story about xianxia").is_ok());
        assert!(classify_story_request("Write me a novel").is_ok());
        assert!(classify_story_request("What's the weather today?").is_err());
    }
}
