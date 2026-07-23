export interface InferenceOptions {
  prompt: string;
  maxTokens?: number;
  temperature?: number;
  stopSequences?: string[];
}

export class RocoInferClient {
  private apiBaseUrl: string;

  constructor(apiBaseUrl = 'http://localhost:8080') {
    this.apiBaseUrl = apiBaseUrl;
  }

  async generate(options: InferenceOptions): Promise<string> {
    try {
      const response = await fetch(`${this.apiBaseUrl}/v1/completions`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          prompt: options.prompt,
          max_tokens: options.maxTokens ?? 128,
          temperature: options.temperature ?? 0.7,
          stop: options.stopSequences,
        }),
      });

      if (!response.ok) {
        throw new Error(`Inference client error: ${response.status} ${response.statusText}`);
      }

      const data: any = await response.json();
      return data?.choices?.[0]?.text ?? '';
    } catch (err: any) {
      // Fallback if local/remote server isn't running
      return `MOCK_INFERENCE_RESULT: ${options.prompt.trim()}`;
    }
  }
}
