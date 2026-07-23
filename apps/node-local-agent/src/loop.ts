import { DomainHarness, Context, State } from './framework.js';

export interface LoopResult {
  output: string;
  success: boolean;
  attempts: number;
  rollbackCount: number;
  finalState: State;
}

export class ExecutionLoop {
  constructor(public maxAttempts: number) {}

  async execute(agent: DomainHarness, input: string, ctx: Context): Promise<LoopResult> {
    let state: State = { checkpoint: '', attempts: 0 };
    const history: State[] = [];
    let output = '';
    let success = false;

    for (let attempt = 0; attempt < this.maxAttempts; attempt++) {
      try {
        const result = await agent.run(input, ctx);
        output = result;
        if (agent.verify(output)) {
          success = true;
          state.attempts = attempt + 1;
          state.checkpoint = `check_${attempt}`;
          break;
        } else {
          state = agent.rollback(state);
          history.push({ ...state });
        }
      } catch (err) {
        state = agent.rollback(state);
        history.push({ ...state });
      }
    }

    return {
      output,
      success,
      attempts: state.attempts,
      rollbackCount: history.length,
      finalState: state,
    };
  }
}
