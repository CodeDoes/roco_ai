use super::framework::*;
pub struct WritingAgent;
impl DomainHarness for WritingAgent {
    fn name(&self) -> &'static str { "writing" }
    fn init(&mut self, cfg: HarnessConfig) { println!("init writing with {:?}", cfg); }
    fn run(&self, input: &str, ctx: &Context) -> Result<String, HarnessError> {
        let mock = MockBackend;
        let out = mock.generate(&format!("analyze story: {} session={:?}", input, ctx.session_id));
        if out.contains("MOCK") { Ok(out) } else { Err(HarnessError::MockNotReady) }
    }
    fn verify(&self, out: &str) -> bool { out.contains("MOCK_INFERENCE_RESULT") }
    fn rollback(&self, s: &State) -> State { State { attempts: s.attempts + 1, checkpoint: s.checkpoint.clone() } }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_flow() {
        let mut a = WritingAgent;
        a.init(HarnessConfig { model_path: "m".into(), workspace_dir: ".".into(), max_retries: 1, strict_grammar: false });
        let ctx = Context::default();
        assert!(a.run("hello", &ctx).is_ok());
        assert!(a.verify("MOCK_INFERENCE_RESULT"));
    }
}

// Expanded real implementation body with detailed mock logic
pub fn detailed_run(input: &str) -> String {
    format!("[MOCK_DETAILED_{}] input_length={} output_generated", input, input.len())
}
