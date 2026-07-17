/**
 * RoCo API Client
 *
 * Connects the ProseMirror editor to the Rust backend.
 */

const API_BASE = 'http://localhost:3000';

export interface PlotState {
  characters: string[];
  locations: string[];
  conflicts: string[];
}

export interface Suggestion {
  type: string;
  text: string;
  reasoning?: string;
}

export interface Chapter {
  number: number;
  title: string;
  content: string;
}

export interface Outline {
  chapters: Chapter[];
}

export interface QualityScore {
  overall: number;
  pacing: number;
  engagement: number;
  plot_coherence: number;
  issues: string[];
  suggestions: string[];
}

class RoCoAPI {
  private baseUrl: string;

  constructor(baseUrl: string = API_BASE) {
    this.baseUrl = baseUrl;
  }

  private async request<T>(path: string, options?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      headers: {
        'Content-Type': 'application/json',
      },
      ...options,
    });

    if (!response.ok) {
      throw new Error(`API error: ${response.status}`);
    }

    return response.json();
  }

  // Outline
  async getOutline(): Promise<Outline> {
    return this.request('/outline');
  }

  async updateOutline(outline: Outline): Promise<Outline> {
    return this.request('/outline', {
      method: 'PUT',
      body: JSON.stringify(outline),
    });
  }

  // Chapters
  async getChapter(num: number): Promise<Chapter> {
    return this.request(`/chapters/${num}`);
  }

  async saveChapter(num: number, content: string): Promise<void> {
    await this.request(`/chapters/${num}`, {
      method: 'PUT',
      body: JSON.stringify({ content }),
    });
  }

  async generateChapter(num: number, direction?: string): Promise<Chapter> {
    return this.request(`/chapters/${num}/generate`, {
      method: 'POST',
      body: JSON.stringify({ direction }),
    });
  }

  async reviseChapter(num: number, feedback: string): Promise<Chapter> {
    return this.request(`/chapters/${num}/revise`, {
      method: 'POST',
      body: JSON.stringify({ feedback }),
    });
  }

  // Suggestions
  async getSuggestions(chapterNum: number, text: string): Promise<Suggestion[]> {
    return this.request('/suggestions', {
      method: 'POST',
      body: JSON.stringify({ chapterNum, text }),
    });
  }

  async applySuggestion(suggestion: Suggestion): Promise<string> {
    return this.request('/suggestions/apply', {
      method: 'POST',
      body: JSON.stringify(suggestion),
    });
  }

  // Quality
  async evaluateQuality(chapterNum: number): Promise<QualityScore> {
    return this.request(`/chapters/${chapterNum}/quality`);
  }

  // Plot State
  async getPlotState(): Promise<PlotState> {
    return this.request('/plot-state');
  }

  // Feedback
  async sendFeedback(feedback: string): Promise<{ intent: string; response: string }> {
    return this.request('/feedback', {
      method: 'POST',
      body: JSON.stringify({ feedback }),
    });
  }

  // Story
  async publish(): Promise<{ path: string }> {
    return this.request('/publish', { method: 'POST' });
  }

  async getStatus(): Promise<{
    chapters: number;
    words: number;
    quality: number;
  }> {
    return this.request('/status');
  }
}

export const api = new RoCoAPI();
