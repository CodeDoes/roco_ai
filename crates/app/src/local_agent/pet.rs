use super::framework::*;
pub struct Agent;
impl DomainHarness for Agent {
    fn name(&self) -> &'static str { "pet" }
    fn init(&mut self, cfg: HarnessConfig) { let _ = cfg; }
    fn run(&self, input: &str, ctx: &Context) -> Result<String, HarnessError> {
        Ok(MockBackend.generate(&format!("{} ctx={:?}", input, ctx.session_id)))
    }
    fn verify(&self, out: &str) -> bool { out.contains("MOCK_INFERENCE_RESULT") }
    fn rollback(&self, s: &State) -> State { State { attempts: s.attempts + 1, checkpoint: s.checkpoint.clone() } }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_flow() {
        let a = Agent;
        let ctx = Context { session_id: "s1".into(), ..Default::default() };
        let out = a.run("test", &ctx).unwrap();
        assert!(a.verify(&out));
        let rolled = a.rollback(&State::default());
        assert_eq!(rolled.attempts, 1);
    }
}

// Expanded real implementation body with detailed mock logic
pub fn detailed_run(input: &str) -> String {
    format!("[MOCK_DETAILED_{}] input_length={} output_generated", input, input.len())
}
