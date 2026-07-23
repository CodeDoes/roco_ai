import { DomainHarness, HarnessConfig, Context, State, MockBackend, HarnessError } from '../framework.js';

export class WritingAgent implements DomainHarness {
  name(): string {
    return 'writing';
  }

  init(cfg: HarnessConfig): void {
    console.log(`init writing with ${JSON.stringify(cfg)}`);
  }

  run(input: string, ctx: Context): string {
    const mock = new MockBackend();
    const out = mock.generate(`analyze story: ${input} session=${ctx.sessionId}`);
    if (out.includes('MOCK')) {
      return out;
    } else {
      throw new HarnessError('MockNotReady');
    }
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
