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
    // Generate Chapter at cursor
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.generateChapter', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active editor');
                return;
            }

            vscode.window.showInformationMessage('Generating chapter...');

            try {
                // Story server expects chapter number in path: /chapters/:num/generate
                const chapterNum = await vscode.window.showInputBox({
                    prompt: 'Chapter number to generate',
                    value: '1',
                });
                if (!chapterNum) return;

                const result = await apiRequest(`/chapters/${chapterNum}/generate`, {
                    method: 'POST',
                    body: JSON.stringify({
                        direction: editor.document.getText(),
                    }),
                });

                editor.edit((editBuilder) => {
                    const fullText = `Chapter ${chapterNum}: ${result.title || ''}\n\n${result.content || ''}`;
                    editBuilder.replace(editor.selection, fullText);
                });

                vscode.window.showInformationMessage(`Chapter ${chapterNum} generated!`);
            } catch (error) {
                vscode.window.showErrorMessage('Failed to generate chapter — is roco-server running with --story?');
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
                    }),
                });

                editor.edit((editBuilder) => {
                    editBuilder.insert(position, result.text || '');
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

            const chapterNum = await vscode.window.showInputBox({
                prompt: 'Chapter number',
                value: '1',
            });
            if (!chapterNum) return;

            vscode.window.showInformationMessage('Revising...');

            try {
                const result = await apiRequest(`/chapters/${chapterNum}/revise`, {
                    method: 'POST',
                    body: JSON.stringify({ feedback }),
                });

                editor.edit((editBuilder) => {
                    editBuilder.replace(editor.selection, result.content || result.text || '');
                });

                vscode.window.showInformationMessage('Revised!');
            } catch (error) {
                vscode.window.showErrorMessage('Failed to revise — is roco-server running with --story?');
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

    // Check Quality
    context.subscriptions.push(
        vscode.commands.registerCommand('roco.checkQuality', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('No active editor');
                return;
            }

            const chapterNum = await vscode.window.showInputBox({
                prompt: 'Chapter number to evaluate',
                value: '1',
            });
            if (!chapterNum) return;

            vscode.window.showInformationMessage('Evaluating quality...');

            try {
                const result = await apiRequest(`/chapters/${chapterNum}/quality`);

                const panel = vscode.window.createWebviewPanel(
                    'rocoQuality',
                    `RoCo Quality — Chapter ${chapterNum}`,
                    vscode.ViewColumn.Beside,
                    {}
                );

                const issues = (result.issues || []).map((i: any) =>
                    `<li><strong>${i.category}</strong> [${i.severity}]: ${i.description}</li>`
                ).join('');
                const strengths = (result.strengths || []).map((s: string) =>
                    `<li>${s}</li>`
                ).join('');
                const suggestions = (result.suggestions || []).map((s: string) =>
                    `<li>${s}</li>`
                ).join('');

                panel.webview.html = `
                    <!DOCTYPE html>
                    <html><head><style>
                        body { font-family: sans-serif; padding: 16px; }
                        h2 { color: #e94560; }
                        .score { font-size: 24px; font-weight: bold; }
                        .section { margin-bottom: 16px; }
                        .good { color: green; } .warn { color: orange; } .bad { color: red; }
                    </style></head><body>
                        <h2>Quality Report — Chapter ${chapterNum}</h2>
                        <div class="section">
                            <p>Overall: <span class="score ${result.overall >= 7 ? 'good' : result.overall >= 5 ? 'warn' : 'bad'}">${result.overall || '?'}/10</span></p>
                            <p>Pacing: ${result.pacing || '?'} | Show-don't-tell: ${result.show_dont_tell || '?'} | Character voice: ${result.character_voice || '?'}</p>
                            <p>Engagement: ${result.engagement || '?'} | Coherence: ${result.plot_coherence || '?'} | Prose: ${result.prose_quality || '?'}</p>
                        </div>
                        ${issues ? `<div class="section"><h3>Issues</h3><ul>${issues}</ul></div>` : ''}
                        ${strengths ? `<div class="section"><h3>Strengths</h3><ul>${strengths}</ul></div>` : ''}
                        ${suggestions ? `<div class="section"><h3>Suggestions</h3><ul>${suggestions}</ul></div>` : ''}
                    </body></html>
                `;
            } catch (error) {
                vscode.window.showErrorMessage('Failed to check quality — is roco-server running with --story?');
                console.error(error);
            }
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
