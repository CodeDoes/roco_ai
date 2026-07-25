//! State-tuning example: snapshot, restore, and blend recurrent model states.
//!
//! Demonstrates RWKV-7's unique capabilities for recurrent state hotswapping,
//! prefilling, and element-wise state blending.

use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing to see backend execution details
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("============================================================");
    println!("  RWKV-7 Recurrent State Tuning & Hotswapping Example");
    println!("============================================================\n");

    println!("1. Initializing RWKV backend...");
    let backend = RwkvBackend::from_env()?;
    println!("Backend loaded: {}", backend.name());

    // Step A: Save the initial base state
    println!("\n2. Saving initial model state (checkpoint 0)...");
    let initial_state = backend.save_state().await?;
    println!("Initial state saved ({} bytes)", initial_state.len());

    // Step B: Prompt and generate first completion
    let prompt1 = "It was a dark and stormy night, and";
    println!("\n3. Generating completion with Prompt 1: '{}'...", prompt1);

    let req1 = CompletionRequest {
        prompt: prompt1.into(),
        temperature: 0.1,
        max_tokens: 20,
        preserve_state: true, // Keep the state updated in the active session
        ..Default::default()
    };
    let resp1 = backend.complete(req1).await?;
    println!("Completion output: '{}'", resp1.text);

    // Save state after generating from prompt 1
    let state_prompt1 = backend.save_state().await?;
    println!("State saved at Checkpoint 1 (Prompt 1 Context)");

    // Step C: Restore the initial state and prompt with something completely different
    println!("\n4. Restoring initial model state (Checkpoint 0) to clear Prompt 1 context...");
    backend.load_state(initial_state.clone()).await?;
    println!("Successfully restored initial blank state.");

    let prompt2 = "The scientist adjusted the lens of the microscope and saw";
    println!("Generating completion with Prompt 2: '{}'...", prompt2);
    let req2 = CompletionRequest {
        prompt: prompt2.into(),
        temperature: 0.1,
        max_tokens: 20,
        preserve_state: true,
        ..Default::default()
    };
    let resp2 = backend.complete(req2).await?;
    println!("Completion output: '{}'", resp2.text);

    // Step D: Restore Checkpoint 1 and verify the model resumes prompt 1's context
    println!("\n5. Hotswapping back to Checkpoint 1 (Prompt 1 context)...");
    backend.load_state(state_prompt1).await?;
    println!("Successfully hotswapped back to Prompt 1 state.");

    let resume_prompt = " suddenly there was a";
    println!(
        "Continuing generation with prompt suffix: '{}'...",
        resume_prompt
    );
    let req3 = CompletionRequest {
        prompt: resume_prompt.into(),
        temperature: 0.1,
        max_tokens: 15,
        ..Default::default()
    };
    let resp3 = backend.complete(req3).await?;
    println!("Continued output: '{}'", resp3.text);

    // Step E: Demontrating blending states (if session IDs are utilized in actor session pool)
    println!("\n6. Demonstrating Session-Based State Blending...");
    println!("Saving states to sessions 'session_a' and 'session_b'...");

    // Create state in session_a
    let _ = backend
        .complete(CompletionRequest {
            prompt: "A beautiful garden filled with roses".into(),
            session: Some("session_a".into()),
            max_tokens: 1,
            preserve_state: true,
            ..Default::default()
        })
        .await?;

    // Create state in session_b
    let _ = backend
        .complete(CompletionRequest {
            prompt: "A futuristic cyber city with neon lights".into(),
            session: Some("session_b".into()),
            max_tokens: 1,
            preserve_state: true,
            ..Default::default()
        })
        .await?;

    // Blend session_a and session_b with 50% blend factor into 'session_blended'
    println!("Blending session_a and session_b (alpha = 0.5) -> 'session_blended'...");
    if let Err(e) = backend.blend_states("session_a", "session_b", 0.5, "session_blended") {
        println!("Blending returned a backend result: {:?}", e);
    } else {
        println!("State blending complete! The session 'session_blended' can now be used for hybrid generation.");
    }

    println!("\nState tuning demo completed successfully!");
    Ok(())
}
