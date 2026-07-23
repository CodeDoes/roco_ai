import * as http from 'http';
import { WritingAgent } from './agents/writing.js';
import { Context } from './framework.js';

export class RocoServer {
  private server: http.Server;
  private port: number;

  constructor(port = 8080) {
    this.port = port;
    this.server = http.createServer((req, res) => this.handleRequest(req, res));
  }

  start(): Promise<void> {
    return new Promise((resolve) => {
      this.server.listen(this.port, () => {
        resolve();
      });
    });
  }

  stop(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server.close((err) => {
        if (err) reject(err);
        else resolve();
      });
    });
  }

  private handleRequest(req: http.IncomingMessage, res: http.ServerResponse) {
    // Add CORS headers
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

    if (req.method === 'OPTIONS') {
      res.writeHead(200);
      res.end();
      return;
    }

    const parsedUrl = new URL(req.url || '', `http://${req.headers.host || 'localhost'}`);
    const pathname = parsedUrl.pathname;

    let body = '';
    req.on('data', (chunk) => {
      body += chunk;
    });

    req.on('end', async () => {
      try {
        let jsonBody: any = {};
        if (body) {
          try {
            jsonBody = JSON.parse(body);
          } catch {
            // ignore JSON parse errors
          }
        }

        // 1. OpenAI-compatible completions
        if (pathname === '/v1/completions' && req.method === 'POST') {
          const prompt = jsonBody.prompt || '';
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            id: 'cmpl-roco-ts',
            object: 'text_completion',
            created: Date.now(),
            model: 'rwkv-7-mock',
            choices: [{
              text: `MOCK_INFERENCE_RESULT: ${prompt}`,
              index: 0,
              finish_reason: 'stop'
            }]
          }));
          return;
        }

        // 2. OpenAI-compatible chat completions
        if (pathname === '/v1/chat/completions' && req.method === 'POST') {
          const messages = jsonBody.messages || [];
          const lastMsg = messages[messages.length - 1]?.content || '';
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            id: 'chatcmpl-roco-ts',
            object: 'chat.completion',
            created: Date.now(),
            model: 'rwkv-7-mock',
            choices: [{
              message: {
                role: 'assistant',
                content: `MOCK_INFERENCE_RESULT: ${lastMsg}`
              },
              index: 0,
              finish_reason: 'stop'
            }]
          }));
          return;
        }

        // 3. Chapters generate: /chapters/:num/generate
        const generateMatch = pathname.match(/^\/chapters\/(\d+)\/generate$/);
        if (generateMatch && req.method === 'POST') {
          const chNum = generateMatch[1];
          const direction = jsonBody.direction || '';
          const agent = new WritingAgent();
          const ctx: Context = { sessionId: 'server_gen', memory: [], toolResults: new Map() };
          const out = await agent.run(direction, ctx);

          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            title: `Chapter ${chNum}: Generated Story Outline`,
            content: `${out}\nDirection was: ${direction}`
          }));
          return;
        }

        // 4. Continue writing: /continue
        if (pathname === '/continue' && req.method === 'POST') {
          const text = jsonBody.text || '';
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            text: ` MOCK_INFERENCE_RESULT: Continued prose from ${text}`
          }));
          return;
        }

        // 5. Suggestions: /suggestions
        if (pathname === '/suggestions' && req.method === 'POST') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            suggestions: [
              { type: 'Prose suggestion', text: 'Add more description about the local scenery and fog.' },
              { type: 'Pacing suggestion', text: 'Speed up the transition into the main action.' }
            ]
          }));
          return;
        }

        // 6. Revise selection: /chapters/:num/revise
        const reviseMatch = pathname.match(/^\/chapters\/(\d+)\/revise$/);
        if (reviseMatch && req.method === 'POST') {
          const chNum = reviseMatch[1];
          const feedback = jsonBody.feedback || '';
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            content: `Revised chapter ${chNum} content incorporating feedback: ${feedback}`
          }));
          return;
        }

        // 7. Chapter quality: /chapters/:num/quality
        const qualityMatch = pathname.match(/^\/chapters\/(\d+)\/quality$/);
        if (qualityMatch && req.method === 'GET') {
          const chNum = qualityMatch[1];
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            overall: 8.5,
            pacing: 'Balanced',
            show_dont_tell: 'High quality',
            character_voice: 'Distinct',
            engagement: 'Engaging',
            plot_coherence: 'Excellent',
            prose_quality: 'Superb',
            issues: [
              { category: 'Pacing', severity: 'low', description: 'Initial scene starts slightly slow.' }
            ],
            strengths: ['Great character interactions', 'Vivid descriptive detail'],
            suggestions: [`Consider shortening the preamble in chapter ${chNum}`]
          }));
          return;
        }

        // 8. Plot state: /plot-state
        if (pathname === '/plot-state' && req.method === 'GET') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            characters: ['Silas', 'Elara'],
            locations: ['The Cliffside Lighthouse', 'The Coastal Inn'],
            conflicts: ['Silas discovers a coded letter', 'Elara hides her true identity']
          }));
          return;
        }

        // Not Found
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('Not Found');
      } catch (err: any) {
        res.writeHead(500, { 'Content-Type': 'text/plain' });
        res.end(`Internal Server Error: ${err?.message || err}`);
      }
    });
  }
}
