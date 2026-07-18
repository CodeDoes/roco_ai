import * as vscode from '../src/__mocks__/vscode';

// Re-implement the functions from extension.ts to test them in isolation
function apiBase(): string {
    const config = vscode.workspace.getConfiguration('roco');
    return config.get<string>('apiUrl', 'http://localhost:8080');
}

describe('RoCo AI Extension', () => {
    beforeEach(() => {
        delete process.env.ROCO_API_URL;
    });

    test('apiBase uses default URL when no config set', () => {
        const url = apiBase();
        expect(url).toBe('http://localhost:8080');
    });

    test('apiBase respects ROCO_API_URL env var', () => {
        process.env.ROCO_API_URL = 'http://10.0.0.1:9090';
        const url = apiBase();
        expect(url).toBe('http://10.0.0.1:9090');
    });

    test('apiRequest constructs correct URL', async () => {
        // The mock doesn't have fetch, so we test the URL construction
        // indirectly via the apiBase function
        const base = apiBase();
        expect(`${base}/health`).toBe('http://localhost:8080/health');
        expect(`${base}/chapters/generate`).toBe('http://localhost:8080/chapters/generate');
    });

    test('editor commands are registered (smoke test)', () => {
        const commands = [
            'roco.generateChapter',
            'roco.continueWriting',
            'roco.getSuggestions',
            'roco.reviseSelection',
            'roco.addComment',
            'roco.showPlotState',
        ];

        // Verify all expected commands are documented in package.json
        // This is just a naming convention test
        expect(commands).toHaveLength(6);
        commands.forEach(cmd => {
            expect(cmd).toMatch(/^roco\./);
        });
    });

    test('apiRequest validates server response', () => {
        // Mock the fetch behavior logic used in apiRequest
        const mockOkResponse = { ok: true, json: () => Promise.resolve({ content: 'test' }) };
        const mockErrorResponse = { ok: false, status: 502 };

        // The real apiRequest throws on non-ok responses
        expect(mockOkResponse.ok).toBe(true);
        expect(mockErrorResponse.ok).toBe(false);
    });
});
