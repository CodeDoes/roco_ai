//! Stress-test all 4 prompt styles.

use std::io::Write;

use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

#[derive(Clone, Copy)]
enum PromptStyle {
    StateOnly,
    Interleaved,
    HistoryFirst,
    RepeatedSystem,
}

impl PromptStyle {
    fn label(&self) -> &'static str {
        match self {
            PromptStyle::StateOnly => "state-only",
            PromptStyle::Interleaved => "interleaved",
            PromptStyle::HistoryFirst => "history-first",
            PromptStyle::RepeatedSystem => "repeated-system",
        }
    }
    fn all() -> [PromptStyle; 4] {
        [
            PromptStyle::StateOnly,
            PromptStyle::Interleaved,
            PromptStyle::HistoryFirst,
            PromptStyle::RepeatedSystem,
        ]
    }
}

struct Turn {
    user: String,
    assistant: String,
}

fn coherence(text: &str) -> f64 {
    let total = text.chars().count().max(1);
    let alpha = text
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace())
        .count();
    alpha as f64 / total as f64
}

fn repetition(text: &str) -> f64 {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 6 {
        return 0.0;
    }
    let mut counts = std::collections::HashMap::new();
    for w in words.windows(3) {
        *counts.entry(w.join(" ")).or_insert(0u32) += 1;
    }
    counts.values().filter(|&&c| c > 1).sum::<u32>() as f64 / (words.len() - 2) as f64
}

fn build_prompt(style: PromptStyle, turns: &[Turn], input: &str, system: &str) -> String {
    match style {
        PromptStyle::StateOnly => input.to_string(),
        PromptStyle::Interleaved => {
            let mut s = format!("System: {system}\n\n");
            for t in turns {
                s.push_str(&format!(
                    "User: {}\n\nAssistant: {}\n\n",
                    t.user, t.assistant
                ));
            }
            s.push_str(&format!("User: {input}"));
            s
        }
        PromptStyle::HistoryFirst => {
            let mut s = String::new();
            for t in turns {
                s.push_str(&format!(
                    "User: {}\n\nAssistant: {}\n\n",
                    t.user, t.assistant
                ));
            }
            s.push_str(&format!("System: {system}\n\nUser: {input}"));
            s
        }
        PromptStyle::RepeatedSystem => {
            let mut s = format!("System: {system}\n\n");
            for t in turns {
                s.push_str(&format!(
                    "User: {}\n\nAssistant: {}\n\nSystem: {system}\n\n",
                    t.user, t.assistant
                ));
            }
            s.push_str(&format!("User: {input}"));
            s
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .init();
    let turns_per_style = 20;
    let system = "You are a helpful assistant.";
    let inputs: Vec<String> = (0..turns_per_style)
        .map(|i| {
            let qs = [
                "What is photosynthesis?",
                "Explain gravity to a child.",
                "How does a microwave work?",
                "Why is the sky blue?",
                "What causes earthquakes?",
                "How do batteries store energy?",
                "Why do leaves change color?",
                "How does GPS work?",
                "What is dark matter?",
                "How does the immune system work?",
                "Explain the internet simply.",
                "What causes ocean tides?",
                "How does nuclear fusion work?",
                "Why do we dream?",
                "How does a computer CPU work?",
                "What is CRISPR gene editing?",
                "How does WiFi transmit data?",
                "Why do magnets attract iron?",
                "Explain how vaccines work.",
                "What is quantum entanglement?",
            ];
            qs[i % qs.len()].to_string()
        })
        .collect();

    eprintln!("\n  Prompt Style Stress Test\n");
    let backend = RwkvBackend::from_env()?;
    eprintln!("Backend: {} — ready.\n", backend.name());

    for &style in &PromptStyle::all() {
        eprintln!("═══ Style: {} ═══", style.label());
        let mut turns: Vec<Turn> = Vec::new();
        let mut total_prompt = 0;
        let mut total_completion = 0;
        let mut scores: Vec<(f64, f64)> = Vec::new();
        for (i, input) in inputs.iter().enumerate() {
            eprint!("  {:>2}/{}: ", i + 1, turns_per_style);
            let _ = std::io::stderr().flush();
            let streamed = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let cloned = streamed.clone();
            let prompt = build_prompt(style, &turns, input, system);
            let resp = backend
                .complete(CompletionRequest {
                    system: String::new(),
                    prompt,
                    output_schema: None,
                    grammar: None,
                    temperature: 0.5,
                    max_tokens: 48,
                    estimated_prompt_tokens: 0,
                    thinking: false,
                    preserve_state: false,
                    on_token: Some(Box::new(move |t: &str| cloned.lock().unwrap().push_str(t))),
                    session: None,
                    ..Default::default()
                })
                .await?;

            let text = streamed.lock().unwrap().clone();
            let (c, r) = (coherence(&text), repetition(&text));
            scores.push((c, r));
            total_prompt += resp.usage.prompt_tokens;
            total_completion += resp.usage.completion_tokens;
            eprintln!(
                "{} tok  coherence={:.2}  repetition={:.2}",
                resp.usage.completion_tokens, c, r
            );
            if c < 0.9 || r > 0.3 {
                eprintln!(
                    "    ⚠ \"{}\"",
                    text.trim().chars().take(80).collect::<String>()
                );
            }
            turns.push(Turn {
                user: inputs[i].clone(),
                assistant: text,
            });
        }
        let avg_c = scores.iter().map(|(c, _)| c).sum::<f64>() / scores.len() as f64;
        let avg_r = scores.iter().map(|(_, r)| r).sum::<f64>() / scores.len() as f64;
        let first_c: f64 = scores[..10].iter().map(|(c, _)| c).sum::<f64>() / 10.0;
        let second_c: f64 = scores[10..].iter().map(|(c, _)| c).sum::<f64>() / 10.0;
        eprintln!(
            "  Avg coherence={avg_c:.3}  repetition={avg_r:.3}  trend drop={:.3}",
            first_c - second_c
        );
        eprintln!("  Prompt={total_prompt}  Completion={total_completion}\n");
    }
    Ok(())
}
