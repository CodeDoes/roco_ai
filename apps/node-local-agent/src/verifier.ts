export class Verifier {
  private forbiddenWords: Set<string>;
  private requiredPatterns: string[];
  private minLength: number;

  constructor() {
    this.forbiddenWords = new Set<string>();
    this.requiredPatterns = ['MOCK_INFERENCE_RESULT'];
    this.minLength = 10;
  }

  verify(output: string): boolean {
    if (output.length < this.minLength) {
      return false;
    }
    for (const word of this.forbiddenWords) {
      if (output.includes(word)) {
        return false;
      }
    }
    for (const pat of this.requiredPatterns) {
      if (!output.includes(pat)) {
        return false;
      }
    }
    return true;
  }

  score(output: string): number {
    let score = 1.0;
    if (output.length < this.minLength) {
      score *= 0.5;
    }
    for (const pat of this.requiredPatterns) {
      if (output.includes(pat)) {
        score *= 1.2;
      } else {
        score *= 0.3;
      }
    }
    return Math.min(score, 1.0);
  }

  explain(output: string): string {
    if (this.verify(output)) {
      return `PASS: verified (len=${output.length}, required_patterns_matches)`;
    } else {
      return `FAIL: min_length=${this.minLength}, required=${this.requiredPatterns.length}, forbidden_check`;
    }
  }
}
