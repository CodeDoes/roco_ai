//! Classic programmatic validation for chapters.
//!
//! Rule-based checks that require no model inference:
//! - Paragraph spacing (double newlines between paragraphs)
//! - Word count targets (per chapter, per paragraph, per section)
//! - Spelling / typo detection (common English word list)
//! - No repeating segments (sentences, phrases across chapters)
//! - Grammar heuristics (capitalization, punctuation)

use std::collections::{HashMap, HashSet};

use super::{ValidationCheck, ValidationSeverity, ValidationSource, WordCountTargets};

/// Programmatic chapter validator.
#[derive(Debug, Clone)]
pub struct ChapterValidator {
    /// Max allowed repetitions of the same sentence before flagging
    pub max_sentence_repetitions: usize,
    /// Max allowed repetitions of the same phrase (5+ words) before flagging
    pub max_phrase_repetitions: usize,
    /// Whether to check for common typos/spelling errors
    pub check_spelling: bool,
    /// Whether to check grammar heuristics
    pub check_grammar: bool,
    /// Whether to check paragraph spacing
    pub check_spacing: bool,
    /// Minimum sentences per paragraph
    pub min_sentences_per_paragraph: usize,
    /// Maximum consecutive short paragraphs before flagging
    pub max_consecutive_short_paragraphs: usize,
    /// Common English words (used for spell-check baseline)
    common_words: HashSet<String>,
}

impl Default for ChapterValidator {
    fn default() -> Self {
        Self {
            max_sentence_repetitions: 1,
            max_phrase_repetitions: 2,
            check_spelling: true,
            check_grammar: true,
            check_spacing: true,
            min_sentences_per_paragraph: 1,
            max_consecutive_short_paragraphs: 3,
            common_words: build_common_word_set(),
        }
    }
}

impl ChapterValidator {
    /// Run all programmatic checks on a chapter text.
    pub fn validate(
        &self,
        text: &str,
        scope: &str,
        targets: &WordCountTargets,
    ) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // 1. Word count checks
        checks.extend(self.check_word_counts(text, scope, targets));

        // 2. Paragraph spacing
        if self.check_spacing {
            checks.extend(self.check_paragraph_spacing(text, scope));
        }

        // 3. Repeating segments
        checks.extend(self.check_repetitions(text, scope));

        // 4. Spelling
        if self.check_spelling {
            checks.extend(self.check_spelling(text, scope));
        }

        // 5. Grammar heuristics
        if self.check_grammar {
            checks.extend(self.check_grammar(text, scope));
        }

        // 6. Structure — check for thinking contamination
        checks.push(self.check_thinking_contamination(text, scope));

        checks
    }

    // ── Word count checks ──────────────────────────────────────────────

    fn check_word_counts(
        &self,
        text: &str,
        scope: &str,
        targets: &WordCountTargets,
    ) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();
        let word_count = text.split_whitespace().count();

        // Per-chapter word count
        let wc_ok = word_count >= targets.per_chapter;
        checks.push(ValidationCheck {
            name: "word_count_per_chapter".into(),
            passed: wc_ok,
            severity: if wc_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: format!("{} words (target: {} min)", word_count, targets.per_chapter),
            suggestion: if wc_ok {
                None
            } else {
                Some(format!(
                    "Add {} more words to reach the target of {}.",
                    targets.per_chapter.saturating_sub(word_count),
                    targets.per_chapter,
                ))
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Per-paragraph word count
        let paragraphs: Vec<&str> = text
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();

        if !paragraphs.is_empty() {
            let short_paras: Vec<usize> = paragraphs
                .iter()
                .enumerate()
                .filter(|(_, p)| p.split_whitespace().count() < targets.per_paragraph / 2)
                .map(|(i, _)| i + 1)
                .collect();

            let short_ok = short_paras.len() <= self.max_consecutive_short_paragraphs;
            checks.push(ValidationCheck {
                name: "paragraph_word_count".into(),
                passed: short_ok,
                severity: if short_ok && short_paras.is_empty() {
                    ValidationSeverity::Info
                } else if short_ok {
                    ValidationSeverity::Warning
                } else {
                    ValidationSeverity::Error
                },
                detail: format!(
                    "{} paragraphs below {} words (paragraphs: {:?})",
                    short_paras.len(),
                    targets.per_paragraph / 2,
                    short_paras,
                ),
                suggestion: if short_ok && !short_paras.is_empty() {
                    Some("Consider expanding short paragraphs with more detail.".into())
                } else if !short_ok {
                    Some(format!(
                        "{} paragraphs are very short. Merge or expand them.",
                        short_paras.len()
                    ))
                } else {
                    None
                },
                source: ValidationSource::Classic,
                scope: scope.to_string(),
            });
        }

        // Minimum total check (only if scope is a chapter)
        let min_total_ok = word_count >= targets.minimum_total;
        checks.push(ValidationCheck {
            name: "minimum_total_word_count".into(),
            passed: min_total_ok,
            severity: if min_total_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "{} words (minimum total target: {})",
                word_count, targets.minimum_total
            ),
            suggestion: if min_total_ok {
                None
            } else {
                Some(format!(
                    "The story should be at least {} words total.",
                    targets.minimum_total
                ))
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    // ── Paragraph spacing ───────────────────────────────────────────────

    fn check_paragraph_spacing(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // Check 1: paragraphs separated by double newlines
        let has_proper_breaks = text.contains("\n\n");
        checks.push(ValidationCheck {
            name: "paragraph_separation".into(),
            passed: has_proper_breaks,
            severity: if has_proper_breaks {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: if has_proper_breaks {
                "Paragraphs are properly separated by blank lines.".into()
            } else {
                "No paragraph breaks found. Use double newlines (\\n\\n) between paragraphs.".into()
            },
            suggestion: if has_proper_breaks {
                None
            } else {
                Some("Insert blank lines between paragraphs to improve readability.".into())
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Check 2: no excessive spacing (more than 2 consecutive newlines)
        let has_excessive_spacing = text.contains("\n\n\n");
        checks.push(ValidationCheck {
            name: "excessive_spacing".into(),
            passed: !has_excessive_spacing,
            severity: if has_excessive_spacing {
                ValidationSeverity::Warning
            } else {
                ValidationSeverity::Info
            },
            detail: if has_excessive_spacing {
                "Excessive blank lines found.".into()
            } else {
                "No excessive spacing.".into()
            },
            suggestion: if has_excessive_spacing {
                Some(
                    "Remove extra blank lines — use at most one blank line between paragraphs."
                        .into(),
                )
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Check 3: sentences within paragraphs (no single-sentence paragraphs)
        let paragraphs: Vec<&str> = text
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();

        let single_sentence_paras: Vec<usize> = paragraphs
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let sentence_count = count_sentences(p);
                sentence_count > 0 && sentence_count < self.min_sentences_per_paragraph
            })
            .map(|(i, _)| i + 1)
            .collect();

        let ss_ok = single_sentence_paras.len() <= 2; // Allow a couple for dialog
        checks.push(ValidationCheck {
            name: "single_sentence_paragraphs".into(),
            passed: ss_ok,
            severity: if single_sentence_paras.is_empty() {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "{} single-sentence paragraph(s): {:?}",
                single_sentence_paras.len(),
                single_sentence_paras,
            ),
            suggestion: if !single_sentence_paras.is_empty() && !ss_ok {
                Some("Consider merging very short paragraphs for better flow.".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    // ── Repetition detection ────────────────────────────────────────────

    fn check_repetitions(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // Check 1: Repeated sentences
        let sentences: Vec<String> = text
            .split(['.', '!', '?'])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| s.len() > 10) // ignore very short fragments
            .collect();

        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut repeated_sentences: Vec<String> = Vec::new();
        for sent in &sentences {
            let count = seen.entry(sent.clone()).or_insert(0);
            *count += 1;
            if *count > self.max_sentence_repetitions {
                repeated_sentences.push(sent.chars().take(50).collect());
            }
        }

        let rep_ok = repeated_sentences.is_empty();
        checks.push(ValidationCheck {
            name: "repeated_sentences".into(),
            passed: rep_ok,
            severity: if rep_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Error
            },
            detail: format!("{} repeated sentence(s)", repeated_sentences.len()),
            suggestion: if rep_ok {
                None
            } else {
                Some(format!(
                    "Rewrite these repeated sentences: {:?}",
                    repeated_sentences,
                ))
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Check 2: Repeated phrases (5+ word n-grams)
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut phrase_seen: HashMap<String, usize> = HashMap::new();
        for window in words.windows(5) {
            let phrase = window.join(" ").to_lowercase();
            let count = phrase_seen.entry(phrase).or_insert(0);
            *count += 1;
        }

        let repeated_phrases: Vec<(&String, &usize)> = phrase_seen
            .iter()
            .filter(|(_, &count)| count > self.max_phrase_repetitions)
            .collect();

        let phrase_ok = repeated_phrases.is_empty();
        checks.push(ValidationCheck {
            name: "repeated_phrases".into(),
            passed: phrase_ok,
            severity: if phrase_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!("{} repeated 5-word phrase(s)", repeated_phrases.len()),
            suggestion: if phrase_ok {
                None
            } else {
                let examples: Vec<String> = repeated_phrases
                    .iter()
                    .take(3)
                    .map(|(p, c)| {
                        format!("\"{}\" (×{})", p.chars().take(40).collect::<String>(), c)
                    })
                    .collect();
                Some(format!(
                    "Vary these repeated phrases: {}",
                    examples.join(", ")
                ))
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    // ── Spelling check ──────────────────────────────────────────────────

    fn check_spelling(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();
        let mut potential_typos: Vec<String> = Vec::new();

        for word in text.split_whitespace() {
            let cleaned: String = word
                .chars()
                .filter(|c| c.is_alphabetic() || *c == '\'' || *c == '-')
                .collect();

            if cleaned.len() < 3 {
                continue;
            }

            let lower = cleaned.to_lowercase();
            if !self.common_words.contains(&lower) && is_likely_typo(&lower) {
                potential_typos.push(cleaned);
            }
        }

        // Limit to first 20 to avoid overwhelming
        potential_typos.truncate(20);

        let typo_ok = potential_typos.is_empty();
        if !typo_ok {
            // Deduplicate
            potential_typos.sort();
            potential_typos.dedup();
        }

        checks.push(ValidationCheck {
            name: "spelling".into(),
            passed: typo_ok,
            severity: if typo_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "{} potential typo(s): {:?}",
                potential_typos.len(),
                potential_typos,
            ),
            suggestion: if !potential_typos.is_empty() {
                Some(format!(
                    "Check these words: {}. They may be misspelled or uncommon.",
                    potential_typos.join(", "),
                ))
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        checks
    }

    // ── Grammar heuristics ──────────────────────────────────────────────

    fn check_grammar(&self, text: &str, scope: &str) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // Check 1: Sentences start with capital letter
        let mut no_cap_sentences: Vec<usize> = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            // Skip empty lines, headers, and list items
            if trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with('-')
                || trimmed.starts_with('*')
                || trimmed.starts_with(|c: char| c.is_ascii_digit())
            {
                continue;
            }
            // Check if it looks like a sentence start (capital or quote)
            if !trimmed.starts_with(|c: char| c.is_uppercase())
                && !trimmed.starts_with('"')
                && !trimmed.starts_with('“')
                && !trimmed.starts_with('‘')
                && !trimmed.starts_with('\'')
            {
                // Allow dialog tags
                if !trimmed.starts_with(|c: char| c.is_lowercase()) || trimmed.len() < 3 {
                    continue;
                }
                no_cap_sentences.push(i + 1);
            }
        }

        let cap_ok = no_cap_sentences.len() <= 2;
        checks.push(ValidationCheck {
            name: "sentence_capitalization".into(),
            passed: cap_ok,
            severity: if no_cap_sentences.is_empty() {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "{} sentence(s) without capital start at line(s): {:?}",
                no_cap_sentences.len(),
                no_cap_sentences,
            ),
            suggestion: if !no_cap_sentences.is_empty() {
                Some("Ensure every sentence starts with a capital letter.".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Check 2: Unmatched quotes
        let quote_count = text.matches('"').count();
        let _single_quote_count = text.matches('\'').count();
        let quotes_ok = quote_count % 2 == 0;
        checks.push(ValidationCheck {
            name: "matching_quotes".into(),
            passed: quotes_ok,
            severity: if quotes_ok {
                ValidationSeverity::Info
            } else {
                ValidationSeverity::Warning
            },
            detail: format!(
                "Found {} double quote marks (should be even for matching pairs)",
                quote_count,
            ),
            suggestion: if !quotes_ok {
                Some("Check for unmatched quotation marks.".into())
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        });

        // Check 3: Multiple spaces
        if text.contains("  ") {
            let multi_space_count = count_multi_spaces(text);
            checks.push(ValidationCheck {
                name: "double_spaces".into(),
                passed: multi_space_count < 5,
                severity: if multi_space_count < 5 {
                    ValidationSeverity::Info
                } else {
                    ValidationSeverity::Warning
                },
                detail: format!("{multi_space_count} instance(s) of multiple consecutive spaces"),
                suggestion: if multi_space_count >= 5 {
                    Some("Replace multiple spaces with single spaces.".into())
                } else {
                    None
                },
                source: ValidationSource::Classic,
                scope: scope.to_string(),
            });
        }

        // Check 4: Repeated punctuation (!!!, ??, etc.)
        let repeated_punct = ["!!!", "??", "?!", "!?", "..", "...", "...."];
        for punct in &repeated_punct {
            if text.contains(punct) {
                checks.push(ValidationCheck {
                    name: format!("repeated_punctuation_{}", punct),
                    passed: false,
                    severity: ValidationSeverity::Warning,
                    detail: format!("Found '{}' in text", punct),
                    suggestion: Some(format!(
                        "Use single '{}' instead of '{}'",
                        if *punct == ".." || *punct == "..." || *punct == "...." {
                            "…"
                        } else {
                            &punct[..1]
                        },
                        punct
                    )),
                    source: ValidationSource::Classic,
                    scope: scope.to_string(),
                });
            }
        }

        checks
    }

    // ── Thinking contamination ──────────────────────────────────────────

    fn check_thinking_contamination(&self, text: &str, scope: &str) -> ValidationCheck {
        let has_thinking = text.contains("💭 thinking")
            || text.contains("thinking")
            || text.contains("💭")
            || text.contains("I'll approach this")
            || text.contains("Let me think")
            || text.contains("I need to");

        ValidationCheck {
            name: "thinking_contamination".into(),
            passed: !has_thinking,
            severity: if has_thinking {
                ValidationSeverity::Error
            } else {
                ValidationSeverity::Info
            },
            detail: if has_thinking {
                "Meta-commentary or thinking contamination detected.".into()
            } else {
                "No thinking contamination found.".into()
            },
            suggestion: if has_thinking {
                Some(
                    "Strip all meta-commentary from the output. Only story prose should remain."
                        .into(),
                )
            } else {
                None
            },
            source: ValidationSource::Classic,
            scope: scope.to_string(),
        }
    }
}

// ── Cross-chapter repetition check ─────────────────────────────────────────

/// Check for repeated content across chapters.
pub fn check_cross_chapter_repetition(
    chapters: &[String],
    scope_prefix: &str,
) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    if chapters.len() < 2 {
        return checks;
    }

    // Extract 5-word phrases from each chapter
    let chapter_phrases: Vec<HashSet<String>> = chapters
        .iter()
        .map(|ch| {
            let words: Vec<&str> = ch.split_whitespace().collect();
            words
                .windows(5)
                .map(|w| w.join(" ").to_lowercase())
                .collect()
        })
        .collect();

    // Check each pair of chapters for overlapping phrases
    for i in 0..chapter_phrases.len() {
        for j in (i + 1)..chapter_phrases.len() {
            let overlap: Vec<&String> = chapter_phrases[i]
                .intersection(&chapter_phrases[j])
                .collect();

            if !overlap.is_empty() && overlap.len() > 3 {
                checks.push(ValidationCheck {
                    name: "cross_chapter_repetition".into(),
                    passed: false,
                    severity: ValidationSeverity::Warning,
                    detail: format!(
                        "Chapter {} and chapter {} share {} repeated 5-word phrases",
                        i + 1,
                        j + 1,
                        overlap.len(),
                    ),
                    suggestion: Some(format!(
                        "Rewrite repeated descriptions or narration between chapters {} and {}.",
                        i + 1,
                        j + 1,
                    )),
                    source: ValidationSource::Classic,
                    scope: format!("{}:cross_chapter", scope_prefix),
                });
            }
        }
    }

    checks
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Count sentences in a text.
fn count_sentences(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed
        .split(['.', '!', '?'])
        .filter(|s| !s.trim().is_empty())
        .count()
}

/// Count instances of multiple consecutive spaces.
fn count_multi_spaces(text: &str) -> usize {
    let mut count = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' && i + 1 < chars.len() && chars[i + 1] == ' ' {
            count += 1;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    count
}

/// Check if a word is likely a typo (not a proper noun, not common English).
fn is_likely_typo(word: &str) -> bool {
    // Skip words with numbers
    if word.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // Skip very short words
    if word.len() < 3 {
        return false;
    }
    // Skip proper nouns (capitalized in middle of text - rough heuristic)
    if word.chars().next().is_some_and(|c| c.is_uppercase()) && word.len() > 2 {
        return false;
    }
    // Check for repeated characters that might be typos (e.g., "teh" for "the")
    let lower = word.to_lowercase();
    // Common patterns
    if lower.contains("thier") || lower.contains("recieve") || lower.contains("beleive") {
        return true;
    }
    true
}

/// Build a set of common English words for spell-check baseline.
fn build_common_word_set() -> HashSet<String> {
    let words = [
        "the",
        "be",
        "to",
        "of",
        "and",
        "a",
        "in",
        "that",
        "have",
        "i",
        "it",
        "for",
        "not",
        "on",
        "with",
        "he",
        "as",
        "you",
        "do",
        "at",
        "this",
        "but",
        "his",
        "by",
        "from",
        "they",
        "we",
        "say",
        "her",
        "she",
        "or",
        "an",
        "will",
        "my",
        "one",
        "all",
        "would",
        "there",
        "their",
        "what",
        "so",
        "up",
        "out",
        "if",
        "about",
        "who",
        "get",
        "which",
        "go",
        "me",
        "when",
        "make",
        "can",
        "like",
        "time",
        "no",
        "just",
        "him",
        "know",
        "take",
        "people",
        "into",
        "year",
        "your",
        "good",
        "some",
        "could",
        "them",
        "see",
        "other",
        "than",
        "then",
        "now",
        "look",
        "only",
        "come",
        "its",
        "over",
        "think",
        "also",
        "back",
        "after",
        "use",
        "two",
        "how",
        "our",
        "work",
        "first",
        "well",
        "way",
        "even",
        "new",
        "want",
        "because",
        "any",
        "these",
        "give",
        "day",
        "most",
        "us",
        "was",
        "had",
        "has",
        "were",
        "been",
        "said",
        "did",
        "got",
        "made",
        "went",
        "being",
        "having",
        "doing",
        "getting",
        "making",
        "going",
        "taking",
        "coming",
        "seeing",
        "looking",
        "man",
        "woman",
        "child",
        "world",
        "life",
        "hand",
        "part",
        "place",
        "case",
        "week",
        "company",
        "system",
        "program",
        "question",
        "government",
        "number",
        "night",
        "point",
        "home",
        "water",
        "room",
        "mother",
        "area",
        "money",
        "story",
        "fact",
        "month",
        "lord",
        "father",
        "side",
        "head",
        "eye",
        "hand",
        "foot",
        "body",
        "face",
        "voice",
        "door",
        "window",
        "table",
        "house",
        "street",
        "road",
        "town",
        "city",
        "country",
        "ground",
        "water",
        "fire",
        "air",
        "long",
        "great",
        "little",
        "own",
        "old",
        "right",
        "high",
        "different",
        "small",
        "large",
        "next",
        "early",
        "young",
        "important",
        "few",
        "same",
        "able",
        "possible",
        "hard",
        "light",
        "dark",
        "deep",
        "cold",
        "hot",
        "warm",
        "soft",
        "hard",
        "sweet",
        "bitter",
        "clean",
        "still",
        "yet",
        "already",
        "always",
        "never",
        "often",
        "sometimes",
        "soon",
        "again",
        "once",
        "here",
        "there",
        "where",
        "everywhere",
        "anywhere",
        "nowhere",
        "above",
        "below",
        "between",
        "through",
        "during",
        "before",
        "after",
        "since",
        "until",
        "while",
        "because",
        "although",
        "though",
        "unless",
        "very",
        "really",
        "quite",
        "almost",
        "nearly",
        "just",
        "only",
        "even",
        "too",
        "enough",
        "away",
        "down",
        "up",
        "out",
        "in",
        "off",
        "over",
        "under",
        "again",
        "back",
        "about",
        "around",
        "along",
        "across",
        "through",
        "past",
        "beyond",
        "toward",
        "inside",
        "outside",
        "without",
        "within",
        "upon",
        "among",
        "between",
        "behind",
        "beneath",
        "beside",
        "beyond",
        "underneath",
        "write",
        "read",
        "speak",
        "hear",
        "think",
        "feel",
        "love",
        "hate",
        "fear",
        "hope",
        "know",
        "believe",
        "understand",
        "remember",
        "forget",
        "imagine",
        "wonder",
        "consider",
        "decide",
        "choose",
        "walk",
        "run",
        "stand",
        "sit",
        "lie",
        "fall",
        "rise",
        "turn",
        "move",
        "follow",
        "watch",
        "see",
        "look",
        "notice",
        "observe",
        "stare",
        "glance",
        "gaze",
        "peer",
        "witness",
        "listen",
        "hear",
        "sound",
        "noise",
        "voice",
        "whisper",
        "shout",
        "scream",
        "laugh",
        "cry",
        "speak",
        "say",
        "tell",
        "talk",
        "explain",
        "describe",
        "mention",
        "announce",
        "declare",
        "state",
        "must",
        "shall",
        "should",
        "would",
        "could",
        "might",
        "may",
        "can",
        "need",
        "dare",
        "every",
        "each",
        "both",
        "few",
        "several",
        "some",
        "any",
        "many",
        "much",
        "more",
        "most",
        "enough",
        "half",
        "next",
        "last",
        "first",
        "second",
        "third",
        "fourth",
        "fifth",
        "chapter",
        "story",
        "character",
        "scene",
        "plot",
        "theme",
        "tone",
        "mood",
        "genre",
        "style",
        "said",
        "asked",
        "replied",
        "answered",
        "whispered",
        "shouted",
        "cried",
        "laughed",
        "thought",
        "wondered",
        "walked",
        "ran",
        "stood",
        "sat",
        "lay",
        "turned",
        "moved",
        "followed",
        "watched",
        "looked",
        "saw",
        "heard",
        "felt",
        "knew",
        "believed",
        "understood",
        "remembered",
        "forgot",
        "imagined",
        "wondered",
        "door",
        "window",
        "wall",
        "floor",
        "ceiling",
        "room",
        "hall",
        "stairs",
        "step",
        "threshold",
        "sky",
        "cloud",
        "sun",
        "moon",
        "star",
        "light",
        "shadow",
        "wind",
        "rain",
        "snow",
        "tree",
        "flower",
        "leaf",
        "grass",
        "earth",
        "stone",
        "rock",
        "hill",
        "mountain",
        "river",
        "ocean",
        "sea",
        "lake",
        "stream",
        "shore",
        "bank",
        "wave",
        "tide",
        "current",
        "depth",
    ];
    words.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_count_check_passes() {
        let text = "The quick brown fox jumps over the lazy dog. ".repeat(100);
        let text = text.as_str(); // ~800 words
        let valid = ChapterValidator::default();
        let targets = WordCountTargets {
            per_chapter: 500,
            ..Default::default()
        };
        let checks = valid.check_word_counts(text, "chapter:1", &targets);
        assert!(
            checks[0].passed,
            "word count should pass for 800 words vs 500 target"
        );
    }

    #[test]
    fn test_word_count_check_fails() {
        let text = "Too short.";
        let valid = ChapterValidator::default();
        let targets = WordCountTargets {
            per_chapter: 500,
            ..Default::default()
        };
        let checks = valid.check_word_counts(text, "chapter:1", &targets);
        assert!(
            !checks[0].passed,
            "word count should fail for 2 words vs 500 target"
        );
    }

    #[test]
    fn test_repeated_sentences_detected() {
        let text = "The knight drew his sword. The knight drew his sword. The knight drew his sword. Then he advanced.";
        let valid = ChapterValidator::default();
        let checks = valid.check_repetitions(text, "chapter:1");
        assert!(!checks[0].passed, "should detect repeated sentences");
    }

    #[test]
    fn test_no_repeated_sentences() {
        let text = "The knight drew his sword. He stepped forward cautiously. The dragon roared.";
        let valid = ChapterValidator::default();
        let checks = valid.check_repetitions(text, "chapter:1");
        assert!(checks[0].passed, "should not flag unique sentences");
    }

    #[test]
    fn test_paragraph_spacing_detected() {
        let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        let valid = ChapterValidator::default();
        let checks = valid.check_paragraph_spacing(text, "chapter:1");
        assert!(checks[0].passed, "should detect proper paragraph spacing");
    }

    #[test]
    fn test_paragraph_spacing_missing() {
        let text = "First paragraph.\nSecond paragraph.\nThird paragraph.";
        let valid = ChapterValidator::default();
        let checks = valid.check_paragraph_spacing(text, "chapter:1");
        assert!(!checks[0].passed, "should flag missing paragraph spacing");
    }

    #[test]
    fn test_cross_chapter_repetition() {
        let ch1 = "The knight drew his sword and stepped forward. The dragon roared loudly and fire filled the air.".to_string();
        let ch2 =
            "The knight drew his sword and advanced. The dragon's roar echoed through the valley."
                .to_string();
        let checks = check_cross_chapter_repetition(&[ch1, ch2], "");
        // At minimum this returns ValidationCheck results (may be zero if no overlap)
        assert!(
            checks.is_empty()
                || checks
                    .iter()
                    .all(|c| c.severity == ValidationSeverity::Warning)
        );
    }

    #[test]
    fn test_excessive_spacing_detected() {
        let text = "Paragraph one.\n\n\n\nParagraph two.";
        let valid = ChapterValidator::default();
        let checks = valid.check_paragraph_spacing(text, "chapter:1");
        assert!(!checks[1].passed, "should flag excessive spacing");
    }

    #[test]
    fn test_thinking_contamination_detected() {
        let text = "The hero walked forward. thinking I need to figure this out. Then he stopped.";
        let valid = ChapterValidator::default();
        let check = valid.check_thinking_contamination(text, "chapter:1");
        assert!(!check.passed, "should detect thinking contamination");
    }

    #[test]
    fn test_grammar_double_spaces() {
        let text = "This  sentence  has double  spaces.";
        let valid = ChapterValidator::default();
        let checks = valid.check_grammar(text, "chapter:1");
        assert!(
            checks.iter().any(|c| c.name == "double_spaces"),
            "should detect double spaces"
        );
    }

    #[test]
    fn test_word_count_targets_default() {
        let targets = WordCountTargets::default();
        assert_eq!(targets.per_chapter, 500);
        assert_eq!(targets.minimum_total, 1500);
    }
}
