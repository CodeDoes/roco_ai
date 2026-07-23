LINE-BY-LINE EXECUTION FLOW

1. full_stack.rs creates StackRunner
2. StackRunner::run_all creates HarnessConfig (model_path="rwkv_mock")
3. Creates coding::Agent (struct Agent)
4. agent.init(cfg) — does nothing but accepts config (line: let _ = cfg;)
5. Creates Context with session_id="full_stack_01", memory=[input], empty HashMap
6. Creates State::default() (checkpoint="", attempts=0)
7. Creates history vec![], output="", success=false
8. For attempt in 0..3:
   a. agent.run(input, &ctx) calls MockBackend.generate(format!("{} ctx={:?}", input, ctx.session_id))
   b. MockBackend.generate returns format!("MOCK_INFERENCE_RESULT: {}", prompt.trim())
   c. run returns Ok(string containing mock result)
   d. output = r.clone()
   e. agent.verify(&output) checks output.contains("MOCK_INFERENCE_RESULT") → true
   f. If true: success = true, state.attempts = attempt + 1, break loop
   g. If false: state = agent.rollback(&state) → attempts += 1, checkpoint cloned
   h. history.push(state.clone())
9. After loop: StackResult returned with output, success=true, attempts=1, rollback_history (empty if success first try)
10. Test asserts: res.success == true, res.output.contains("MOCK_INFERENCE_RESULT"), res.attempts == 1
