import * as vscode from 'vscode';

function apiBase(): string {
    const config = vscode.workspace.getConfiguration('roco');
    return config.get<string>('apiUrl', 'http://localhost:8080');
}

async function apiRequest(path: string, options?: RequestInit): Promise<any> {
    const response = await fetch(`${apiBase()}${path}`, {
        headers: { 'Content-Type': 'application/json' },
        ...options,
    });

    if (!response.ok) {
        throw new Error(`API error: ${response.status}`);
    }

    return response.json();
}

export function activate(context: vscode.ExtensionContext) {
    // Generate Chapter
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.generateChapter', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active editor');
                return;
            }

            vscode.window.showInformationMessage('Generating chapter...');

            try {
                const result = await apiRequest('/chapters/generate', {
                    method: 'POST',
                    body: JSON.stringify({
                        content: editor.document.getText(),
                    }),
                });

                editor.edit((editBuilder) => {
                    editBuilder.replace(editor.selection, result.content);
                });

                vscode.window.showInformationMessage('Chapter generated!');
            } catch (error) {
                vscode.window.showErrorMessage('Failed to generate chapter');
                console.error(error);
            }
        })
    );

    // Continue Writing
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.continueWriting', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active editor');
                return;
            }

            const position = editor.selection.active;
            const line = editor.document.lineAt(position.line).text;

            vscode.window.showInformationMessage('Continuing...');

            try {
                const result = await apiRequest('/continue', {
                    method: 'POST',
                    body: JSON.stringify({
                        text: line,
                        position: { line: position.line, character: position.character },
                    }),
                });

                editor.edit((editBuilder) => {
                    editBuilder.insert(position, result.text);
                });

                vscode.window.showInformationMessage('Continued!');
            } catch (error) {
                vscode.window.showErrorMessage('Failed to continue');
                console.error(error);
            }
        })
    );

    // Get Suggestions
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.getSuggestions', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active editor');
                return;
            }

            const content = editor.document.getText();

            try {
                const result = await apiRequest('/suggestions', {
                    method: 'POST',
                    body: JSON.stringify({ text: content }),
                });

                // Show suggestions in quick pick
                const items = result.suggestions.map((s: any) => ({
                    label: s.type,
                    description: s.text.substring(0, 100),
                    suggestion: s,
                }));

                const selected = await vscode.window.showQuickPick(items, {
                    placeHolder: 'Select a suggestion to apply',
                });

                if (selected) {
                    editor.edit((editBuilder) => {
                        editBuilder.insert(editor.selection.active, selected.suggestion.text);
                    });
                }
            } catch (error) {
                vscode.window.showErrorMessage('Failed to get suggestions');
                console.error(error);
            }
        })
    );

    // Revise Selection
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.reviseSelection', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active editor');
                return;
            }

            const selection = editor.document.getText(editor.selection);
            if (!selection) {
                vscode.window.showWarningMessage('No text selected');
                return;
            }

            const feedback = await vscode.window.showInputBox({
                prompt: 'Enter feedback for revision',
                placeHolder: 'e.g., make it darker, add more dialogue',
            });

            if (!feedback) return;

            vscode.window.showInformationMessage('Revising...');

            try {
                const result = await apiRequest('/revise', {
                    method: 'POST',
                    body: JSON.stringify({ text: selection, feedback }),
                });

                editor.edit((editBuilder) => {
                    editBuilder.replace(editor.selection, result.text);
                });

                vscode.window.showInformationMessage('Revised!');
            } catch (error) {
                vscode.window.showErrorMessage('Failed to revise');
                console.error(error);
            }
        })
    );

    // Add Comment
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.addComment', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active editor');
                return;
            }

            const selection = editor.document.getText(editor.selection);
            if (!selection) {
                vscode.window.showWarningMessage('No text selected');
                return;
            }

            const comment = await vscode.window.showInputBox({
                prompt: 'Enter comment',
                placeHolder: 'e.g., needs more detail, good pacing',
            });

            if (!comment) return;

            editor.edit((editBuilder) => {
                const position = editor.selection.end;
                editBuilder.insert(position, `\n<!-- [RoCo AI]: ${comment} -->`);
            });

            vscode.window.showInformationMessage('Comment added');
        })
    );

    // Show Plot State
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.showPlotState', async () => {
            try {
                const plotState = await apiRequest('/plot-state');

                const panel = vscode.window.createWebviewPanel(
                    'rocoPlotState',
                    'RoCo Plot State',
                    vscode.ViewColumn.Beside,
                    {}
                );

                panel.webview.html = `
                    <!DOCTYPE html>
                    <html>
                    <head>
                        <style>
                            body { font-family: sans-serif; padding: 16px; }
                            h2 { color: #e94560; }
                            .section { margin-bottom: 16px; }
                            .label { font-weight: bold; color: #666; }
                        </style>
                    </head>
                    <body>
                        <h2>Plot State</h2>
                        <div class="section">
                            <div class="label">Characters</div>
                            <div>${plotState.characters.join(', ') || 'None'}</div>
                        </div>
                        <div class="section">
                            <div class="label">Locations</div>
                            <div>${plotState.locations.join(', ') || 'None'}</div>
                        </div>
                        <div class="section">
                            <div class="label">Conflicts</div>
                            <div>${plotState.conflicts.join(', ') || 'None'}</div>
                        </div>
                    </body>
                    </html>
                `;
            } catch (error) {
                vscode.window.showErrorMessage('Failed to get plot state');
                console.error(error);
            }
        })
    );
}

export function deactivate() {}
