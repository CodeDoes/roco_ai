/**
 * REAL framework — no stubs. Mock backend included.
 */

export interface HarnessConfig {
  modelPath: string;
  workspaceDir: string;
  maxRetries: number;
  strictGrammar: boolean;
}

export interface Context {
  sessionId: string;
  memory: string[];
  toolResults: Map<string, string>;
}

export interface State {
  checkpoint: string;
  attempts: number;
}

export class HarnessError extends Error {
  constructor(public type: 'MockNotReady' | 'VerificationFailed' | 'RollbackError', message?: string) {
    super(message || type);
    this.name = 'HarnessError';
  }
}

export interface DomainHarness {
  name(): string;
  init(cfg: HarnessConfig): void;
  run(input: string, ctx: Context): Promise<string> | string;
  verify(output: string): boolean;
  rollback(state: State): State;
}

export class MockBackend {
  generate(prompt: string): string {
    return `MOCK_INFERENCE_RESULT: ${prompt.trim()}`;
  }
}
