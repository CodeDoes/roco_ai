import { DomainHarness, HarnessConfig, Context, State, MockBackend } from '../framework.js';

export class EmailAgent implements DomainHarness {
  name(): string {
    return 'email';
  }

  init(cfg: HarnessConfig): void {
    // no-op
  }

  run(input: string, ctx: Context): string {
    const mock = new MockBackend();
    return mock.generate(`${input} ctx=${ctx.sessionId}`);
  }

  verify(output: string): boolean {
    return output.includes('MOCK_INFERENCE_RESULT');
  }

  rollback(state: State): State {
    return {
      attempts: state.attempts + 1,
      checkpoint: state.checkpoint,
    };
  }
}

export function detailedRun(input: string): string {
  return `[MOCK_DETAILED_${input}] input_length=${input.length} output_generated`;
}
