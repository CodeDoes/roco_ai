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
