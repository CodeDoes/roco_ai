import { HarnessConfig, Context, State } from './framework.js';
import { CodingAgent } from './agents/coding.js';
import { Sandbox } from './sandbox.js';
import { Verifier } from './verifier.js';

export interface StackResult {
  output: string;
  success: boolean;
  attempts: number;
  rollbackHistory: State[];
}

export class StackRunner {
  static async runAll(input: string): Promise<StackResult> {
    const cfg: HarnessConfig = {
      modelPath: 'rwkv_mock',
      workspaceDir: '/tmp/mock',
      maxRetries: 3,
      strictGrammar: true,
    };

    const agent = new CodingAgent();
    agent.init(cfg);

    const ctx: Context = {
      sessionId: 'full_stack_01',
      memory: [input],
      toolResults: new Map<string, string>(),
    };

    let state: State = { checkpoint: '', attempts: 0 };
    const history: State[] = [];
    let output = '';
    let success = false;

    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        const r = await agent.run(input, ctx);
        output = r;
        if (agent.verify(output)) {
          success = true;
          state.attempts = attempt + 1;
          break;
        }
      } catch (err) {
        // Ignored
      }
      state = agent.rollback(state);
      history.push({ ...state });
    }

    return {
      output,
      success,
      attempts: state.attempts,
      rollbackHistory: history,
    };
  }

  static async runWithSandboxAndVerifier(input: string): Promise<[string, boolean, number]> {
    const sb = new Sandbox('/tmp/mock_workspace');
    const v = new Verifier();
    const cfg: HarnessConfig = {
      modelPath: 'rwkv_mock',
      workspaceDir: '/tmp/mock_workspace',
      maxRetries: 3,
      strictGrammar: true,
    };

    const agent = new CodingAgent();
    agent.init(cfg);

    const ctx: Context = {
      sessionId: `stack_${input.length}`,
      memory: [input],
      toolResults: new Map<string, string>(),
    };

    let attempts = 0;
    let out = '';
    let ok = false;

    for (let i = 0; i < 3; i++) {
      attempts++;
      try {
        const r = await agent.run(input, ctx);
        out = r;
        if (agent.verify(out) && v.verify(out) && sb.allowed(input)) {
          ok = true;
          break;
        }
      } catch (err) {
        // Ignored
      }
    }

    return [out, ok, attempts];
  }
}
