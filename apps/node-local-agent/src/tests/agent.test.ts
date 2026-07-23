import test from 'node:test';
import assert from 'node:assert';
import * as fs from 'fs';
import * as path from 'path';
import { Sandbox } from '../sandbox.js';
import { Verifier } from '../verifier.js';
import { ExecutionLoop } from '../loop.js';
import { Context, State } from '../framework.js';
import { CodingAgent } from '../agents/coding.js';
import { WritingAgent } from '../agents/writing.js';
import { HtmlAgent } from '../agents/html.js';
import { ChatAgent } from '../agents/chat.js';
import { OrganizationAgent } from '../agents/organization.js';
import { PetAgent } from '../agents/pet.js';
import { DebugAgent } from '../agents/debug.js';
import { EmailAgent } from '../agents/email.js';
import { ResearchAgent } from '../agents/research.js';
import { AggregateAgent } from '../agents/aggregate.js';
import { BrowserAgent } from '../agents/browser.js';
import { StackRunner } from '../full_stack.js';
import { USE_CASES, TOTAL_USE_CASES_COUNT } from '../use_cases.js';
import { RocoServer } from '../server.js';
import { RocoGateway } from '../gateway.js';
import { RocoInferClient } from '../infer_client.js';
import { runCli } from '../cli.js';

// Setup temporary directory for sandbox tests
const TEST_DIR = path.resolve('./temp_sandbox_test');

test('Sandbox Unit Tests', async (t) => {
  if (fs.existsSync(TEST_DIR)) {
    fs.rmSync(TEST_DIR, { recursive: true, force: true });
  }
  const sandbox = new Sandbox(TEST_DIR);

  await t.test('write and read file successfully', () => {
    sandbox.write('test.txt', 'hello world');
    assert.strictEqual(sandbox.read('test.txt'), 'hello world');
    assert.strictEqual(sandbox.exists('test.txt'), true);
  });

  await t.test('allowed extensions check', () => {
    assert.strictEqual(sandbox.allowed('code.rs'), true);
    assert.strictEqual(sandbox.allowed('script.py'), true);
    assert.strictEqual(sandbox.allowed('doc.pdf'), false);
  });

  await t.test('list files in sandbox root', () => {
    const files = sandbox.listFiles();
    assert.deepStrictEqual(files, ['test.txt']);
  });

  await t.test('prevent path escape traversal via relative traversal', () => {
    assert.throws(() => {
      sandbox.read('../secret.txt');
    }, /path escape blocked/);

    assert.throws(() => {
      sandbox.write('../secret.txt', 'compromised');
    }, /path escape blocked/);
  });

  await t.test('prevent path escape traversal via sibling directory starting with same prefix', () => {
    assert.throws(() => {
      sandbox.read('../temp_sandbox_test_sibling/secret.txt');
    }, /path escape blocked/);
  });

  await t.test('size limit check', () => {
    assert.strictEqual(sandbox.sizeLimitCheck('test.txt', 100), true);
    assert.strictEqual(sandbox.sizeLimitCheck('test.txt', 2), false);
  });

  await t.test('delete file and escape check', () => {
    sandbox.delete('test.txt');
    assert.strictEqual(sandbox.exists('test.txt'), false);

    assert.throws(() => {
      sandbox.delete('../secret.txt');
    }, /escape/);
  });

  // Cleanup
  if (fs.existsSync(TEST_DIR)) {
    fs.rmSync(TEST_DIR, { recursive: true, force: true });
  }
});

test('Verifier Unit Tests', () => {
  const v = new Verifier();

  assert.strictEqual(v.verify('MOCK_INFERENCE_RESULT: code'), true);
  assert.strictEqual(v.verify('short'), false); // too short (< 10)
  assert.strictEqual(v.verify('some other long output without mock'), false); // missing pattern

  assert.ok(v.score('MOCK_INFERENCE_RESULT: code') > 0.5);
  assert.ok(v.explain('MOCK_INFERENCE_RESULT: code').includes('PASS'));
  assert.ok(v.explain('short').includes('FAIL'));
});

test('Execution Loop Unit Tests', async () => {
  const loop = new ExecutionLoop(3);
  const agent = new CodingAgent();
  const ctx: Context = {
    sessionId: 'session_test',
    memory: [],
    toolResults: new Map(),
  };

  const res = await loop.execute(agent, 'generate sorting algorithm', ctx);
  assert.strictEqual(res.success, true);
  assert.strictEqual(res.attempts, 1);
  assert.strictEqual(res.rollbackCount, 0);
  assert.ok(res.output.includes('MOCK_INFERENCE_RESULT'));
});

test('All 11 Domain Agents Unit Tests', () => {
  const agents = [
    new CodingAgent(),
    new WritingAgent(),
    new HtmlAgent(),
    new ChatAgent(),
    new OrganizationAgent(),
    new PetAgent(),
    new DebugAgent(),
    new EmailAgent(),
    new ResearchAgent(),
    new AggregateAgent(),
    new BrowserAgent(),
  ];

  for (const agent of agents) {
    assert.ok(agent.name().length > 0);
    const ctx: Context = { sessionId: 'test', memory: [], toolResults: new Map() };
    agent.init({ modelPath: 'test', workspaceDir: 'test', maxRetries: 1, strictGrammar: false });
    const output = agent.run('hello', ctx);
    assert.ok(agent.verify(output));

    const initial: State = { checkpoint: 'init', attempts: 0 };
    const rolled = agent.rollback(initial);
    assert.strictEqual(rolled.attempts, 1);
  }
});

test('Full Stack Runner Unit Tests', async () => {
  const result = await StackRunner.runAll('test input');
  assert.strictEqual(result.success, true);
  assert.strictEqual(result.attempts, 1);
  assert.ok(result.output.includes('MOCK_INFERENCE_RESULT'));

  const [out, ok, attempts] = await StackRunner.runWithSandboxAndVerifier('test.rs');
  assert.strictEqual(ok, true);
  assert.strictEqual(attempts, 1);
  assert.ok(out.includes('MOCK_INFERENCE_RESULT'));
});

test('70 Mapped Use Cases Unit Tests', () => {
  assert.strictEqual(TOTAL_USE_CASES_COUNT, 70);
  assert.strictEqual(Object.keys(USE_CASES).length, 14);
  assert.strictEqual(USE_CASES.privacy_security.count, 7);
  assert.strictEqual(USE_CASES.niche_edge.count, 8);
});

test('RocoServer and RocoGateway Integration Tests', async () => {
  const srv = new RocoServer(9091);
  const gtw = new RocoGateway(9092, 9091);

  await srv.start();
  await gtw.start();

  try {
    // 1. Test completions via server directly
    const res1 = await fetch('http://localhost:9091/v1/completions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ prompt: 'test prompt' }),
    });
    assert.strictEqual(res1.status, 200);
    const json1: any = await res1.json();
    assert.ok(json1.choices[0].text.includes('test prompt'));

    // 2. Test chat completions via gateway proxy
    const res2 = await fetch('http://localhost:9092/v1/chat/completions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ messages: [{ role: 'user', content: 'hello chat' }] }),
    });
    assert.strictEqual(res2.status, 200);
    assert.strictEqual(res2.headers.get('X-RateLimit-Limit'), '100');
    const json2: any = await res2.json();
    assert.ok(json2.choices[0].message.content.includes('hello chat'));

    // 3. Test generate chapter endpoint
    const resCh = await fetch('http://localhost:9091/chapters/1/generate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ direction: 'into the forest' }),
    });
    assert.strictEqual(resCh.status, 200);
    const jsonCh: any = await resCh.json();
    assert.ok(jsonCh.title.includes('Chapter 1'));
    assert.ok(jsonCh.content.includes('into the forest'));

    // 4. Test continue endpoint
    const resCont = await fetch('http://localhost:9091/continue', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text: 'once upon a time' }),
    });
    assert.strictEqual(resCont.status, 200);
    const jsonCont: any = await resCont.json();
    assert.ok(jsonCont.text.includes('once upon a time'));

    // 5. Test suggestions endpoint
    const resSugg = await fetch('http://localhost:9091/suggestions', {
      method: 'POST',
    });
    assert.strictEqual(resSugg.status, 200);
    const jsonSugg: any = await resSugg.json();
    assert.strictEqual(jsonSugg.suggestions.length, 2);

    // 6. Test revise chapter endpoint
    const resRev = await fetch('http://localhost:9091/chapters/2/revise', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ feedback: 'make it darker' }),
    });
    assert.strictEqual(resRev.status, 200);
    const jsonRev: any = await resRev.json();
    assert.ok(jsonRev.content.includes('make it darker'));

    // 7. Test quality chapter endpoint
    const resQual = await fetch('http://localhost:9091/chapters/3/quality');
    assert.strictEqual(resQual.status, 200);
    const jsonQual: any = await resQual.json();
    assert.strictEqual(jsonQual.overall, 8.5);

    // 8. Test plot state endpoint
    const resPlot = await fetch('http://localhost:9091/plot-state');
    assert.strictEqual(resPlot.status, 200);
    const jsonPlot: any = await resPlot.json();
    assert.deepStrictEqual(jsonPlot.characters, ['Silas', 'Elara']);

  } finally {
    await gtw.stop();
    await srv.stop();
  }
});

test('RocoInferClient Unit Tests', async () => {
  const client = new RocoInferClient('http://localhost:9091');
  const res = await client.generate({ prompt: 'hello world' });
  assert.ok(res.includes('hello world'));
});

test('CLI command parser tests', async () => {
  // Test GUI mockup log
  await runCli(['gui']);
  // Test invalid command help log
  await runCli(['invalid']);
});
