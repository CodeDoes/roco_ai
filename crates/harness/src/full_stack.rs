//! FULL STACK (mocked) — integrated loop: agent -> framework -> mock backend -> verify -> rollback.
use super::*;

pub struct StackRunner;
impl StackRunner {
    pub fn run_all(input: &str) -> StackResult {
        let cfg = HarnessConfig {
            model_path: "rwkv_mock".into(),
            workspace_dir: "/tmp/mock".into(),
            max_retries: 3,
            strict_grammar: true,
        };
        let mut agent = coding::Agent;
        agent.init(cfg);
        let ctx = Context {
            session_id: "full_stack_01".into(),
            memory: vec![input.into()],
            tool_results: std::collections::HashMap::new(),
        };
        let mut state = State::default();
        let mut history = vec![];
        let mut output = String::new();
        let mut success = false;

        for attempt in 0..3 {
            match agent.run(input, &ctx) {
                Ok(r) => {
                    output = r.clone();
                    if agent.verify(&output) {
                        success = true;
                        state.attempts = attempt + 1;
                        break;
                    }
                }
                Err(_) => {}
            }
            state = agent.rollback(&state);
            history.push(state.clone());
        }
        StackResult {
            output,
            success,
            attempts: state.attempts,
            rollback_history: history,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StackResult {
    pub output: String,
    pub success: bool,
    pub attempts: u32,
    pub rollback_history: Vec<State>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_stack_works_mocked() {
        let res = StackRunner::run_all("build function");
        assert!(res.success);
        assert!(res.output.contains("MOCK_INFERENCE_RESULT"));
        assert_eq!(res.attempts, 1);
    }
}

// Expanded full stack with sandbox integration and verifier
pub fn run_with_sandbox_and_verifier(input: &str) -> (String, bool, u32) {
    let sb = Sandbox::new("/tmp/mock_workspace");
    let v = Verifier::new();
    let cfg = HarnessConfig {
        model_path: "rwkv_mock".into(),
        workspace_dir: "/tmp/mock_workspace".into(),
        max_retries: 3,
        strict_grammar: true,
    };
    let mut agent = coding::Agent;
    agent.init(cfg.clone());
    let ctx = Context {
        session_id: format!("stack_{}", input.len()),
        memory: vec![input.into()],
        tool_results: std::collections::HashMap::new(),
    };
    let mut attempts = 0u32;
    let mut out = String::new();
    let mut ok = false;
    for _i in 0..3 {
        attempts += 1;
        match agent.run(input, &ctx) {
            Ok(r) => {
                out = r.clone();
                if agent.verify(&out) && v.verify(&out) && sb.allowed(input) {
                    ok = true;
                    break;
                }
            }
            Err(_) => {}
        }
    }
    (out, ok, attempts)
}
