import { RocoServer } from './server.js';
import { RocoGateway } from './gateway.js';
import { WritingAgent } from './agents/writing.js';
import { Context } from './framework.js';
import * as readline from 'readline';

export async function runCli(args: string[]): Promise<void> {
  const subcommand = args[0];

  switch (subcommand) {
    case 'server': {
      const port = args[1] ? parseInt(args[1], 10) : 8080;
      const srv = new RocoServer(port);
      console.log(`Starting Roco Server on port ${port}...`);
      await srv.start();
      console.log('Roco Server is running. Press Ctrl+C to stop.');
      break;
    }
    case 'gateway': {
      const port = args[1] ? parseInt(args[1], 10) : 8000;
      const targetPort = args[2] ? parseInt(args[2], 10) : 8080;
      const gtw = new RocoGateway(port, targetPort);
      console.log(`Starting Roco Gateway on port ${port} proxying to ${targetPort}...`);
      await gtw.start();
      console.log('Roco Gateway is running. Press Ctrl+C to stop.');
      break;
    }
    case 'start': {
      const premise = args.slice(1).join(' ') || 'A mysterious lighthouse keeper finds a bottle';
      console.log(`Generating story based on premise: "${premise}"...`);
      const agent = new WritingAgent();
      const ctx: Context = { sessionId: 'cli_start', memory: [premise], toolResults: new Map() };
      const output = await agent.run(premise, ctx);
      console.log('\n--- Story Outline Result ---');
      console.log(output);
      console.log('----------------------------\n');
      break;
    }
    case 'interact': {
      console.log('Starting interactive session. Type "exit" or "quit" to stop.');
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
      });

      const agent = new WritingAgent();
      const ctx: Context = { sessionId: 'cli_interact', memory: [], toolResults: new Map() };

      const askQuestion = () => {
        rl.question('\nYou > ', async (input) => {
          if (input.trim() === 'exit' || input.trim() === 'quit') {
            rl.close();
            return;
          }
          try {
            const out = await agent.run(input, ctx);
            console.log(`AI  > ${out}`);
            ctx.memory.push(`User: ${input}`, `AI: ${out}`);
          } catch (err: any) {
            console.log(`Error: ${err.message}`);
          }
          askQuestion();
        });
      };
      askQuestion();
      break;
    }
    case 'gui': {
      console.log('Starting Desktop GUI mockup...');
      console.log('UI Widgets loaded: Chat, Pacing, Editor, Wiki, Session Browser.');
      console.log('GUI running successfully.');
      break;
    }
    default: {
      console.log('RoCo AI Local Agent Framework CLI (Node TypeScript Port)\n');
      console.log('Usage:');
      console.log('  roco start <premise>   Generate a new story based on a premise');
      console.log('  roco server [port]     Start HTTP API server');
      console.log('  roco gateway [port]    Start gateway proxy with rate limiting');
      console.log('  roco interact          Start interactive terminal chat session');
      console.log('  roco gui               Start desktop GUI widget mockup');
      break;
    }
  }
}

// If run directly
if (process.argv[1]?.endsWith('cli.js') || process.argv[1]?.endsWith('cli.ts')) {
  runCli(process.argv.slice(2)).catch(err => {
    console.error('CLI Execution Error:', err);
    process.exit(1);
  });
}
