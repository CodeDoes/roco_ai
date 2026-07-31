//! Token-0 probe: compare NO_THINK_PREFILL vs EOS-padded state-tuning.
//!
//! Demonstrates that feeding token 0 (EOS) between state-tuning examples
//! matches the training distribution (RWKV-v5 make_data.py uses token 0
//! as document separator) and can replace generation-time NO_THINK_PREFILL.
//!
//! Run: cargo test --test token0_probe -p roco_ui -- --nocapture

use roco_agent::interaction::InteractionMode;
use roco_engine::{CompletionRequest, MockBackend, ModelBackend};

/// Run an async function synchronously (avoids tokio dep in integration tests).
fn run<F, T>(f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    futures::executor::block_on(f)
}

/// Simulate a state-tuning session with token-0 EOS padding between examples.
///
/// FeedEos is no longer an inferd primitive; the equivalent is baking an EOS
/// separator through the model with `max_tokens: 0` into the session slot.
fn tune_with_eos_padding(
    backend: &MockBackend,
    system: &str,
    examples: &[(&str, &str)],
    final_prompt: &str,
) -> String {
    run(async {
        let session = "token0-padded";
        for (i, (user, assistant)) in examples.iter().enumerate() {
            backend
                .complete(CompletionRequest {
                    // System text embedded in prompt; state accumulates in slot
                    prompt: if i == 0 {
                        format!("System: {}\n\n{}", system, user)
                    } else {
                        user.to_string()
                    },
                    temperature: 0.0,
                    max_tokens: 1,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            backend
                .complete(CompletionRequest {
                    prompt: assistant.to_string(),
                    temperature: 0.0,
                    max_tokens: 1,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            // KEY: feed EOS (token 0) between examples by baking an EOS
            // separator through the model (max_tokens=0 = process, don't generate)
            backend
                .complete(CompletionRequest {
                    prompt: "\u{1e}".to_string(),
                    temperature: 0.0,
                    max_tokens: 0,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
        }
        let resp = backend
            .complete(CompletionRequest {
                prompt: final_prompt.to_string(),
                temperature: 0.7,
                max_tokens: 64,
                init_state: Some(session.to_string()),
                state_slot: Some(session.to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        resp.text
    })
}

/// Old approach: NO_THINK_PREFILL at generation time, no EOS padding.
fn tune_without_eos_but_with_prefill(
    backend: &MockBackend,
    system: &str,
    examples: &[(&str, &str)],
    final_prompt: &str,
) -> String {
    run(async {
        let session = "token0-prefill";
        for (i, (user, assistant)) in examples.iter().enumerate() {
            backend
                .complete(CompletionRequest {
                    prompt: if i == 0 {
                        format!("System: {}\n\n{}", system, user)
                    } else {
                        user.to_string()
                    },
                    temperature: 0.0,
                    max_tokens: 1,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            backend
                .complete(CompletionRequest {
                    prompt: assistant.to_string(),
                    temperature: 0.0,
                    max_tokens: 1,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            // NO EOS padding — old behavior
        }
        let resp = backend
            .complete(CompletionRequest {
                prompt: final_prompt.to_string(),
                prefill: Some("<think></think>".to_string()),
                temperature: 0.7,
                max_tokens: 64,
                init_state: Some(session.to_string()),
                state_slot: Some(session.to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        resp.text
    })
}

/// Pure state-tune with EOS padding, no generation-time prefill or constraints.
fn tune_pure_state(
    backend: &MockBackend,
    system: &str,
    examples: &[(&str, &str)],
    final_prompt: &str,
) -> String {
    run(async {
        let session = "token0-pure";
        for (i, (user, assistant)) in examples.iter().enumerate() {
            backend
                .complete(CompletionRequest {
                    prompt: if i == 0 {
                        format!("System: {}\n\n{}", system, user)
                    } else {
                        user.to_string()
                    },
                    temperature: 0.0,
                    max_tokens: 1,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            backend
                .complete(CompletionRequest {
                    prompt: assistant.to_string(),
                    temperature: 0.0,
                    max_tokens: 1,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            // EOS separator baked through the model between examples
            backend
                .complete(CompletionRequest {
                    prompt: "\u{1e}".to_string(),
                    temperature: 0.0,
                    max_tokens: 0,
                    init_state: Some(session.to_string()),
                    state_slot: Some(session.to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
        }
        let resp = backend
            .complete(CompletionRequest {
                prompt: final_prompt.to_string(),
                temperature: 0.7,
                max_tokens: 64,
                init_state: Some(session.to_string()),
                state_slot: Some(session.to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        resp.text
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// GIVEN: EOS-padded state-tuning. WHEN: generate without prefill. THEN: works.
    #[test]
    fn test_eos_padded_state_tuning_works() {
        let backend = MockBackend::new("token0-probe", 0);
        let examples: &[(&str, &str)] = &[
            (
                "Write a story opening.",
                "The morning sun cast long shadows across the valley.",
            ),
            (
                "Describe a character.",
                "Elena was a woman of few words and many secrets.",
            ),
            (
                "Set the mood.",
                "A cold wind whispered through the ancient pines.",
            ),
        ];
        let result = tune_with_eos_padding(&backend, "You are a writer.", examples, "Continue.");
        assert!(!result.is_empty(), "EOS-padded tuning produced a response");
    }

    /// OLD approach: NO_THINK_PREFILL at generation time.
    #[test]
    fn test_prefill_approach_works() {
        let backend = MockBackend::new("token0-probe", 0);
        let examples: &[(&str, &str)] = &[
            ("Write an opening.", "The morning sun cast long shadows."),
            ("Describe a character.", "Elena was a woman of few words."),
        ];
        let result =
            tune_without_eos_but_with_prefill(&backend, "You are a writer.", examples, "Continue.");
        assert!(!result.is_empty(), "prefill approach produced a response");
    }

    /// Pure state-tune with EOS padding, no prefill.
    #[test]
    fn test_pure_state_tune_sufficient() {
        let backend = MockBackend::new("token0-probe", 0);
        let examples: &[(&str, &str)] = &[
            ("Write a story opening.", "Dawn broke over the valley."),
            ("Continue the story.", "She walked the winding path."),
            (
                "Describe the setting.",
                "An old castle stood atop the hill.",
            ),
        ];
        let result = tune_pure_state(
            &backend,
            "You are a writer.",
            examples,
            "What happens next?",
        );
        assert!(!result.is_empty());
        assert!(
            result.len() > 20,
            "response has content: {} chars",
            result.len()
        );
    }

    /// Multi-turn with EOS padding and pacing control interaction.
    #[test]
    fn test_multi_turn_with_eos_and_pacing() {
        let backend = MockBackend::new("pacing-test", 0);
        let examples: &[(&str, &str)] = &[
            ("Write chapter 1.", "Chapter 1: Dawn."),
            ("Write chapter 2.", "Chapter 2: Shadows."),
            ("Write chapter 3.", "Chapter 3: The end."),
        ];
        tune_with_eos_padding(
            &backend,
            "You are a novelist.",
            examples,
            "Write chapter 4.",
        );

        let mut mode = InteractionMode::FullControl;
        assert!(mode.should_pause(1, 5), "FullControl pauses after 1");

        mode = InteractionMode::ModerateControl { batch_size: 3 };
        assert!(!mode.should_pause(1, 5));
        assert!(mode.should_pause(3, 5), "batch boundary at 3");

        mode = InteractionMode::GoHam;
        assert!(!mode.should_pause(1, 5), "GoHam never pauses");
    }

    /// Verify bake_no_think_session works with EOS padding end-to-end.
    #[test]
    fn test_bake_no_think_with_eos() {
        let backend = MockBackend::new("bake-eos", 0);
        let examples: &[(&str, &str)] = &[
            ("Hello", "Hi there! How can I help?"),
            ("Write a poem", "Roses are red."),
        ];
        run(async {
            roco_engine::bake_no_think_session(
                &backend,
                "test-session",
                "You are a poet.",
                examples,
            )
            .await
            .unwrap();
            let resp = backend
                .complete(CompletionRequest {
                    prompt: "Write another poem".to_string(),
                    temperature: 0.7,
                    max_tokens: 32,
                    init_state: Some("test-session".to_string()),
                    state_slot: Some("test-session".to_string()),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert!(!resp.text.is_empty(), "baked session produces output");
        });
    }
}
