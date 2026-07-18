// Minimal vscode mock for testing

export enum ViewColumn {
    Active = -1,
    Beside = -2,
    One = 1,
    Two = 2,
    Three = 3,
}

export class Position {
    constructor(
        public line: number,
        public character: number,
    ) {}
}

export class Range {
    constructor(
        public start: Position,
        public end: Position,
    ) {}
}

export class Selection {
    constructor(
        public start: Position,
        public end: Position,
        public active: Position,
        public anchor: Position,
    ) {}
}

export class TextEditor {
    public document: TextDocument;
    public selection: Selection;
    public edits: Array<{ range: Range; text: string }> = [];

    constructor(content: string) {
        this.document = new TextDocument(content);
        this.selection = new Selection(
            new Position(0, 0),
            new Position(0, content.length),
            new Position(0, content.length),
            new Position(0, 0),
        );
    }
}

export class TextDocument {
    private lines: string[];

    constructor(public content: string) {
        this.lines = content.split('\n');
    }

    getText(): string {
        return this.content;
    }

    lineAt(line: number): { text: string } {
        return { text: this.lines[line] || '' };
    }
}

export class ExtensionContext {
    subscriptions: Array<{ dispose(): void }> = [];
}

export class TextEdit {
    constructor(
        public range: Range,
        public text: string,
    ) {}
}

export let activeTextEditor: TextEditor | undefined;

export function setActiveTextEditor(editor: TextEditor | undefined) {
    activeTextEditor = editor;
}

export namespace window {
    export function showInformationMessage(message: string, ...items: string[]): Thenable<string | undefined> {
        return Promise.resolve(undefined);
    }

    export function showWarningMessage(message: string, ...items: string[]): Thenable<string | undefined> {
        return Promise.resolve(undefined);
    }

    export function showErrorMessage(message: string, ...items: string[]): Thenable<string | undefined> {
        return Promise.resolve(undefined);
    }

    export function showInputBox(options?: { prompt?: string; placeHolder?: string }): Thenable<string | undefined> {
        return Promise.resolve(undefined);
    }

    export function showQuickPick(
        items: Array<{ label: string; description?: string }>,
        options?: { placeHolder?: string },
    ): Thenable<{ label: string; description?: string } | undefined> {
        return Promise.resolve(undefined);
    }

    export function createWebviewPanel(
        viewType: string,
        title: string,
        showOptions: ViewColumn | { viewColumn: ViewColumn; preserveFocus?: boolean },
        options?: { enableScripts?: boolean },
    ): WebviewPanel {
        return new WebviewPanel();
    }
}

export class WebviewPanel {
    webview = new Webview();
    dispose() {}
}

export class Webview {
    html = '';
}

export namespace workspace {
    export function getConfiguration(section: string): {
        get<T>(key: string, defaultValue?: T): T | undefined;
    } {
        return {
            get<T>(key: string, defaultValue?: T): T | undefined {
                if (section === 'roco' && key === 'apiUrl') {
                    return (process.env.ROCO_API_URL || 'http://localhost:8080') as unknown as T;
                }
                return defaultValue;
            },
        };
    }
}

export class Disposable {
    static from(...disposables: Array<{ dispose(): void }>): { dispose(): void } {
        return { dispose: () => disposables.forEach(d => d.dispose()) };
    }
}

export namespace commands {
    export function registerCommand(
        command: string,
        callback: (...args: any[]) => any,
    ): { dispose(): void } {
        return { dispose: () => {} };
    }
}
