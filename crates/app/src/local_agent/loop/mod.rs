//! Agent execution loop with retry, rollback detection, and state tracking.
use super::framework::*;

pub struct ExecutionLoop {
    pub max_attempts: u32,
}

impl ExecutionLoop {
    pub fn execute<A: DomainHarness>(&self, agent: &mut A, input: &str, ctx: &Context) -> LoopResult {
        let mut state = State::default();
        let mut history: Vec<State> = Vec::new();
        let mut output = String::new();
        let mut success = false;

        for attempt in 0..self.max_attempts {
            match agent.run(input, ctx) {
                Ok(r) => {
                    output = r.clone();
                    if agent.verify(&output) {
                        success = true;
                        state.attempts = attempt + 1;
                        state.checkpoint = format!("check_{}", attempt);
                        break;
                    } else {
                        // Verification failed: trigger rollback and record state
                        state = agent.rollback(&state);
                        history.push(state.clone());
                    }
                }
                Err(_) => {
                    state = agent.rollback(&state);
                    history.push(state.clone());
                }
            }
        }
        LoopResult {
            output,
            success,
            attempts: state.attempts,
            rollback_count: history.len() as u32,
            final_state: state,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoopResult {
    pub output: String,
    pub success: bool,
    pub attempts: u32,
    pub rollback_count: u32,
    pub final_state: State,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailVerifyAgent;
    impl DomainHarness for FailVerifyAgent {
        fn name(&self) -> &'static str { "fail_verify" }
        fn init(&mut self, _cfg: HarnessConfig) {}
        fn run(&self, input: &str, _ctx: &Context) -> Result<String, HarnessError> {
            Ok(format!("Result: {}", input))
        }
        fn verify(&self, _output: &str) -> bool {
            false // Always fail verification
        }
        fn rollback(&self, state: &State) -> State {
            State {
                checkpoint: format!("{}_failed", state.checkpoint),
                attempts: state.attempts + 1,
            }
        }
    }

    #[test]
    fn test_rollback_on_verification_failure() {
        let mut agent = FailVerifyAgent;
        let loop_runner = ExecutionLoop { max_attempts: 3 };
        let ctx = Context::default();
        let res = loop_runner.execute(&mut agent, "input_test", &ctx);

        assert!(!res.success);
        assert_eq!(res.rollback_count, 3);
        assert_eq!(res.final_state.attempts, 3);
        assert_eq!(res.final_state.checkpoint, "_failed_failed_failed");
    }
}
