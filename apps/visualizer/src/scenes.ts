// RoCo AI — Visualizer scenes (ported from visualizer/app.js to TS).
// Pure functions that return SVG markup strings. No framework deps.

export const COL = {
  cyan: "#38f0ff",
  violet: "#b07bff",
  magenta: "#ff5cc8",
  green: "#46f7a5",
  amber: "#ffce5c",
  red: "#ff6b6b",
  ink: "#dfe7ff",
  muted: "#8a96c4",
};

export interface TraceEvent {
  phase: string;
  actor: string;
  detail: string;
}
export interface Trace {
  events: TraceEvent[];
  memory: string[][];
}

function hexRGB(h: string): [number, number, number] {
  return [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
}
export function rgba(h: string, a: number): string {
  const [r, g, b] = hexRGB(h);
  return `rgba(${r},${g},${b},${a})`;
}

// --- data (ported from roco_core's default_trace fallback) -----------------
export function defaultTrace(): Trace {
  const n = 6;
  const memory: string[][] = [["orchestrator", "decomposed_into", n + " subtasks"]];
  const events: TraceEvent[] = [
    { phase: "decompose", actor: "orchestrator", detail: `split task into ${n} subtasks` },
  ];
  for (let i = 1; i <= n; i++) {
    memory.push(["orchestrator", "spawned", `worker-doc-${i}`]);
    memory.push([`worker-doc-${i}`, "used_backend", "mock-3b"]);
    events.push({ phase: "execute", actor: `worker-doc-${i}`, detail: "model call + verify" });
    events.push({ phase: "verify", actor: "verifier", detail: "gate passed" });
  }
  events.push({ phase: "aggregate", actor: "aggregator", detail: "merged outputs" });
  return { events, memory };
}

export function workerCount(t: Trace): number {
  return Math.max(1, t.memory.filter((m) => m.length === 3 && m[1] === "spawned").length);
}

// --- Scene 1: Stateful Core ------------------------------------------------
function statefulToken(i: number, time: number): string {
  const phase = (time * 0.6 + i * 0.137) % 1.0;
  if (phase > 1.0) return "";
  const target = i % 2 === 0 ? 290.0 : 650.0;
  const x = 460.0 + (target - 460.0) * phase;
  const y = 180.0 + (1.0 - Math.abs(phase - 0.5) * 2.0) * -22.0;
  const c = i % 2 === 0 ? COL.cyan : COL.magenta;
  return `<circle cx="${x.toFixed(2)}" cy="${y.toFixed(2)}" r="3" fill="${c}"/>`;
}
function kvCell(i: number): string {
  const cols = 8;
  const xi = i % cols;
  const yi = Math.floor(i / cols);
  const x = 560.0 + xi * 11.0;
  const y = 200.0 + yi * 11.0;
  const a = (0.45 + 0.55 * ((i + 1) / 60)).toFixed(2);
  return `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="9" height="9" rx="2" fill="rgba(255,92,200,${a})"/>`;
}
export function sceneStateful(time: number): string {
  const tokens = Math.floor(time * 6.0);
  const cells = Math.min(time * 6.0, 60.0);
  let s = `<svg viewBox="0 0 920 360" height="360" preserveAspectRatio="xMidYMid meet">`;
  s += `<text x="290" y="24" fill="${COL.cyan}" font-size="14" text-anchor="middle" font-family="ui-monospace,monospace">RoCo · Stateful (RWKV / SSM)</text>`;
  s += `<text x="650" y="24" fill="${COL.magenta}" font-size="14" text-anchor="middle" font-family="ui-monospace,monospace">Transformer · KV-Cache</text>`;
  for (let i = 0; i < 26; i++) s += statefulToken(i, time);
  const m = 1 + 0.04 * Math.sin(time * 2.0);
  s += `<rect x="240" y="200" width="${(104 * m).toFixed(1)}" height="${(70 * m).toFixed(1)}" rx="10" fill="#122a33" stroke="${COL.cyan}" stroke-width="1.5"/>`;
  s += `<text x="290" y="232" fill="${COL.ink}" font-size="12" text-anchor="middle" font-family="ui-monospace,monospace">STATE</text>`;
  s += `<text x="290" y="250" fill="${COL.cyan}" font-size="12" text-anchor="middle" font-family="ui-monospace,monospace">64 units</text>`;
  for (let i = 0; i < Math.floor(cells); i++) s += kvCell(i);
  s += `<text x="20" y="344" fill="${COL.ink}" font-size="12" font-family="ui-monospace,monospace">tokens streamed: ${tokens}</text>`;
  s += `<text x="20" y="358" fill="${COL.muted}" font-size="11" font-family="ui-monospace,monospace">RoCo state: 64 units (constant)  •  KV-cache: O(N) → blowup</text>`;
  s += `</svg>`;
  return s;
}

// --- Scene 2: Fan-out -----------------------------------------------------
function node(x: number, y: number, r: number, fill: string, label: string, sub: string): string {
  let s = `<circle cx="${x}" cy="${y}" r="${r}" fill="${rgba(fill, 0.35)}" stroke="${fill}" stroke-width="1.6"/>`;
  s += `<text x="${x}" y="${(y - (sub ? 3 : 0)).toFixed(1)}" fill="${COL.ink}" font-size="11" text-anchor="middle" font-family="ui-monospace,monospace">${label}</text>`;
  if (sub) s += `<text x="${x}" y="${(y + 12).toFixed(1)}" fill="${COL.muted}" font-size="9" text-anchor="middle" font-family="ui-monospace,monospace">${sub}</text>`;
  return s;
}
function ring(x: number, y: number, prog: number): string {
  const segs = 24;
  let s = "";
  for (let seg = 0; seg < segs; seg++) {
    if (seg / segs <= prog) {
      const a0 = -Math.PI / 2 + (seg / segs) * Math.PI * 2;
      const a1 = a0 + (Math.PI * 2) / segs;
      s += `<line x1="${(x + 29 * Math.cos(a0)).toFixed(2)}" y1="${(y + 29 * Math.sin(a0)).toFixed(2)}" x2="${(x + 29 * Math.cos(a1)).toFixed(2)}" y2="${(y + 29 * Math.sin(a1)).toFixed(2)}" stroke="${COL.cyan}" stroke-width="3"/>`;
    }
  }
  return s;
}
export function sceneFanout(time: number, workers: number): string {
  const n = Math.max(1, workers);
  const workY: number[] = [];
  for (let i = 0; i < n; i++) workY.push(90 + (n > 1 ? 220 / (n - 1) : 0) * i);
  const dots = workY.map((wy) => {
    const ph = (time * 0.8) % 1.0;
    const x = 130 + (430 - 130) * ph;
    return [wy, x] as [number, number];
  });
  const mid = Math.floor(n / 2);
  let s = `<svg viewBox="0 0 920 420" height="420" preserveAspectRatio="xMidYMid meet">`;
  for (const wy of workY) s += `<line x1="130" y1="210" x2="430" y2="${wy.toFixed(1)}" stroke="rgba(120,160,255,0.5)" stroke-width="1.5"/>`;
  workY.forEach((wy, i) => {
    const col = i === mid ? COL.red : COL.cyan;
    s += `<line x1="460" y1="${wy.toFixed(1)}" x2="760" y2="130" stroke="${col}" stroke-width="1.3"/>`;
  });
  s += `<line x1="770" y1="160" x2="770" y2="260" stroke="${COL.cyan}" stroke-width="1.6"/>`;
  s += `<line x1="800" y1="210" x2="880" y2="210" stroke="${COL.green}" stroke-width="1.8"/>`;
  for (const [wy, x] of dots) s += `<circle cx="${x.toFixed(1)}" cy="${wy.toFixed(1)}" r="2.5" fill="${COL.cyan}"/>`;
  s += node(100, 210, 30, COL.violet, "ORCH", "decompose");
  workY.forEach((wy, i) => {
    s += node(445, wy, 24, COL.cyan, `W${i + 1}`, "");
    s += ring(445, wy, (time * 0.5 + i * 0.2) % 1.0);
    const col = i === mid ? COL.red : COL.green;
    const sym = i === mid ? "✕ verify" : "✓ verify";
    s += `<text x="475" y="${(wy - 22).toFixed(1)}" fill="${col}" font-size="11" font-family="ui-monospace,monospace">${sym}</text>`;
  });
  s += node(770, 130, 28, COL.amber, "VERIFY", "gate");
  s += node(770, 290, 28, COL.cyan, "AGG", "merge");
  s += node(890, 210, 26, COL.green, "OUT", "final");
  s += `</svg>`;
  return s;
}

// --- Scene 3: ContextBudget -----------------------------------------------
export function sceneBudget(_time: number): string {
  const segs: [string, number, string][] = [
    ["system", 700, COL.cyan],
    ["task", 1200, COL.violet],
    ["tools", 800, COL.magenta],
    ["scratch", 700, COL.amber],
    ["generation", 1536, COL.green],
  ];
  const total = segs.reduce((a, s) => a + s[1], 0);
  const maxPrompt = 3000;
  let s = `<svg viewBox="0 0 920 200" height="200" preserveAspectRatio="xMidYMid meet">`;
  let x = 20.0;
  for (const [name, tok, c] of segs) {
    const w = (tok / total) * 880.0;
    s += `<rect x="${x.toFixed(1)}" y="50" width="${w.toFixed(1)}" height="54" fill="${c}"/>`;
    s += `<text x="${(x + w / 2).toFixed(1)}" y="74" fill="#05060c" font-size="11" text-anchor="middle" font-family="ui-monospace,monospace">${name}</text>`;
    s += `<text x="${(x + w / 2).toFixed(1)}" y="90" fill="#05060c" font-size="11" text-anchor="middle" font-family="ui-monospace,monospace">${tok}</text>`;
    x += w;
  }
  const capX = 20.0 + (maxPrompt / total) * 880.0;
  s += `<line x1="${capX.toFixed(1)}" y1="36" x2="${capX.toFixed(1)}" y2="118" stroke="${COL.red}" stroke-width="2"/>`;
  s += `<text x="${capX.toFixed(1)}" y="28" fill="${COL.red}" font-size="11" text-anchor="middle" font-family="ui-monospace,monospace">max prompt = ${maxPrompt}</text>`;
  s += `<text x="20" y="160" fill="${COL.muted}" font-size="12" font-family="ui-monospace,monospace">total = ${total} tokens · prompt hard-capped at ${maxPrompt}</text>`;
  s += `</svg>`;
  return s;
}

// --- Scene 4: CapacityPool ------------------------------------------------
export function sceneCapacity(time: number, speed: number): string {
  const names = ["LocalRwkv7", "Rwkv7Cpu7B", "Rwkv7Cpu13B", "KiloHy3", "Nvidia"];
  const specs = ["gpu 4gb + cache 4gb", "cpu 1 + ram 15gb", "cpu 1 + ram 27gb", "kilo:1 tencent", "nvidia:1 gpu"];
  const colors = [COL.cyan, COL.violet, COL.magenta, COL.amber, COL.green];
  let s = `<svg viewBox="0 0 920 220" height="220" preserveAspectRatio="xMidYMid meet">`;
  let x = 20.0;
  for (let i = 0; i < 5; i++) {
    const cw = (920 - 40 - 14 * 4) / 5;
    const rx = x;
    x += cw + 14;
    const free = (0.5 + 0.5 * Math.sin(time * (0.6 + i * 0.15) * speed)) * 100.0;
    const lit = free < 45.0;
    s += `<rect x="${rx.toFixed(1)}" y="50" width="${cw.toFixed(1)}" height="150" rx="12" fill="${lit ? "#131f33" : "#080b16"}" stroke="${colors[i]}" stroke-width="${lit ? 1.6 : 1}"/>`;
    s += `<text x="${(rx + 10).toFixed(1)}" y="70" fill="${COL.ink}" font-size="12" font-family="ui-monospace,monospace">${names[i]}</text>`;
    s += `<text x="${(rx + 10).toFixed(1)}" y="88" fill="${COL.muted}" font-size="10" font-family="ui-monospace,monospace">${specs[i]}</text>`;
    s += `<rect x="${(rx + 10).toFixed(1)}" y="110" width="${(cw - 20).toFixed(1)}" height="12" rx="6" fill="rgba(255,255,255,0.08)"/>`;
    s += `<rect x="${(rx + 10).toFixed(1)}" y="110" width="${((cw - 20) * free / 100).toFixed(1)}" height="12" rx="6" fill="${colors[i]}"/>`;
    s += `<text x="${(rx + 10).toFixed(1)}" y="140" fill="${COL.muted}" font-size="11" font-family="ui-monospace,monospace">free ${free.toFixed(0)}%</text>`;
    if (lit) s += `<text x="${(rx + cw - 10).toFixed(1)}" y="70" fill="${colors[i]}" font-size="10" text-anchor="end" font-family="ui-monospace,monospace">● active</text>`;
  }
  s += `<text x="20" y="220" fill="${COL.muted}" font-size="12" font-family="ui-monospace,monospace">select(free, order) routes each subtask to the first backend that fits · GPU+CPU concurrent</text>`;
  s += `</svg>`;
  return s;
}
