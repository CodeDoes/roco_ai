//! Test: can the model emit a <tool_call> as free text (no grammar)?
use roco_core::engine::{CompletionRequest, ModelBackend};
use roco_core::rwkv_backend::RwkvBackend;

#[tokio::main]
async fn main() {
    let backend = RwkvBackend::from_env().expect("backend");
    let system = "You are a tool-using agent. To use a tool, emit exactly:\n\
        <tool_call>\n{\"name\": \"add\", \"arguments\": {\"numbers\": [2, 3]}}\n</tool_call>\n\
        Then you will receive the result. Available tool: add (sums numbers).";
    let prompt = "What is 2 + 3? Use the add tool.";
    let req = CompletionRequest {
        system: system.into(),
        prompt: prompt.into(),
        output_schema: None,
        grammar: None,
        temperature: 0.0,
        max_tokens: 200,
        estimated_prompt_tokens: 0,
        thinking: false,
        preserve_state: false,
        on_token: None,
            session: None,
    };
    let resp = backend.complete(req).await.expect("complete");
    println!("=== RESPONSE ===");
    println!("{}", resp.text);
    println!("=== END ===");
}
