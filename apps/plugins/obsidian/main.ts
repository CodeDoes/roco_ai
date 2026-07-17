import {
  App,
  Plugin,
  PluginSettingTab,
  Setting,
  MarkdownView,
  Notice,
  TFile,
} from 'obsidian';

interface RoCoSettings {
  apiUrl: string;
  autoSuggest: boolean;
  showPlotState: boolean;
}

const DEFAULT_SETTINGS: RoCoSettings = {
  apiUrl: 'http://localhost:3000',
  autoSuggest: true,
  showPlotState: true,
};

export default class RoCoPlugin extends Plugin {
  settings: RoCoSettings;

  async onload() {
    await this.loadSettings();

    // Add ribbon icon
    this.addRibbonIcon('sparkles', 'RoCo AI', () => {
      this.openSidebar();
    });

    // Add command: Generate chapter
    this.addCommand({
      id: 'generate-chapter',
      name: 'Generate Chapter',
      callback: () => this.generateChapter(),
    });

    // Add command: Continue writing
    this.addCommand({
      id: 'continue-writing',
      name: 'Continue Writing',
      callback: () => this.continueWriting(),
    });

    // Add command: Get suggestions
    this.addCommand({
      id: 'get-suggestions',
      name: 'Get Suggestions',
      callback: () => this.getSuggestions(),
    });

    // Add command: Revise selection
    this.addCommand({
      id: 'revise-selection',
      name: 'Revise Selection',
      editorCallback: (editor) => {
        const selection = editor.getSelection();
        if (selection) {
          this.reviseSelection(selection);
        }
      },
    });

    // Add command: Add comment
    this.addCommand({
      id: 'add-comment',
      name: 'Add Comment',
      editorCallback: (editor) => {
        const selection = editor.getSelection();
        if (selection) {
          this.addComment(selection);
        }
      },
    });

    // Add settings tab
    this.addSettingTab(new RoCoSettingTab(this.app, this));

    // Register editor extension for inline suggestions
    this.registerEditorExtension([]);
  }

  onunload() {}

  async loadSettings() {
    this.settings = Object.assign({}, DEFAULT_SETTINGS, await this.loadData());
  }

  async saveSettings() {
    await this.saveData(this.settings);
  }

  // API methods
  private async apiRequest(path: string, options?: RequestInit): Promise<any> {
    const response = await fetch(`${this.settings.apiUrl}${path}`, {
      headers: { 'Content-Type': 'application/json' },
      ...options,
    });

    if (!response.ok) {
      throw new Error(`API error: ${response.status}`);
    }

    return response.json();
  }

  // Open sidebar
  openSidebar() {
    // TODO: Open RoCo sidebar with plot state, suggestions, etc.
    new Notice('RoCo AI sidebar opened');
  }

  // Generate chapter
  async generateChapter() {
    const view = this.app.workspace.getActiveViewOfType(MarkdownView);
    if (!view) {
      new Notice('No active markdown view');
      return;
    }

    new Notice('Generating chapter...');

    try {
      const result = await this.apiRequest('/chapters/generate', {
        method: 'POST',
        body: JSON.stringify({
          content: view.editor.getValue(),
        }),
      });

      view.editor.replaceSelection(result.content);
      new Notice('Chapter generated!');
    } catch (error) {
      new Notice('Failed to generate chapter');
      console.error(error);
    }
  }

  // Continue writing
  async continueWriting() {
    const view = this.app.workspace.getActiveViewOfType(MarkdownView);
    if (!view) {
      new Notice('No active markdown view');
      return;
    }

    const cursor = view.editor.getCursor();
    const line = view.editor.getLine(cursor.line);

    new Notice('Continuing...');

    try {
      const result = await this.apiRequest('/continue', {
        method: 'POST',
        body: JSON.stringify({
          text: line,
          position: cursor,
        }),
      });

      view.editor.replaceRange(result.text, cursor);
      new Notice('Continued!');
    } catch (error) {
      new Notice('Failed to continue');
      console.error(error);
    }
  }

  // Get suggestions
  async getSuggestions() {
    const view = this.app.workspace.getActiveViewOfType(MarkdownView);
    if (!view) {
      new Notice('No active markdown view');
      return;
    }

    const content = view.editor.getValue();

    try {
      const result = await this.apiRequest('/suggestions', {
        method: 'POST',
        body: JSON.stringify({ text: content }),
      });

      // Show suggestions in a notice or modal
      new Notice(`Got ${result.suggestions.length} suggestions`);
    } catch (error) {
      new Notice('Failed to get suggestions');
      console.error(error);
    }
  }

  // Revise selection
  async reviseSelection(text: string) {
    const feedback = await this.promptForFeedback();
    if (!feedback) return;

    new Notice('Revising...');

    try {
      const result = await this.apiRequest('/revise', {
        method: 'POST',
        body: JSON.stringify({ text, feedback }),
      });

      const view = this.app.workspace.getActiveViewOfType(MarkdownView);
      if (view) {
        view.editor.replaceSelection(result.text);
      }

      new Notice('Revised!');
    } catch (error) {
      new Notice('Failed to revise');
      console.error(error);
    }
  }

  // Add comment
  async addComment(text: string) {
    const comment = await this.promptForComment();
    if (!comment) return;

    const view = this.app.workspace.getActiveViewOfType(MarkdownView);
    if (!view) return;

    // Add comment as HTML comment in markdown
    const commentHtml = `<!-- [RoCo AI]: ${comment} -->`;
    view.editor.replaceSelection(`${text}\n${commentHtml}`);

    new Notice('Comment added');
  }

  // Prompt for feedback
  private async promptForFeedback(): Promise<string | null> {
    // TODO: Show a modal for feedback input
    return prompt('Enter feedback:');
  }

  // Prompt for comment
  private async promptForComment(): Promise<string | null> {
    // TODO: Show a modal for comment input
    return prompt('Enter comment:');
  }
}

class RoCoSettingTab extends PluginSettingTab {
  plugin: RoCoPlugin;

  constructor(app: App, plugin: RoCoPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    const { containerEl } = this;
    containerEl.empty();

    containerEl.createEl('h2', { text: 'RoCo AI Settings' });

    new Setting(containerEl)
      .setName('API URL')
      .setDesc('URL of the RoCo API server')
      .addText((text) =>
        text
          .setPlaceholder('http://localhost:3000')
          .setValue(this.plugin.settings.apiUrl)
          .onChange(async (value) => {
            this.plugin.settings.apiUrl = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName('Auto-suggest')
      .setDesc('Automatically show suggestions while typing')
      .addToggle((toggle) =>
        toggle
          .setValue(this.plugin.settings.autoSuggest)
          .onChange(async (value) => {
            this.plugin.settings.autoSuggest = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName('Show Plot State')
      .setDesc('Show plot state in sidebar')
      .addToggle((toggle) =>
        toggle
          .setValue(this.plugin.settings.showPlotState)
          .onChange(async (value) => {
            this.plugin.settings.showPlotState = value;
            await this.plugin.saveSettings();
          })
      );
  }
}
