import { useEffect, useRef, useState } from "react";
import {
  defaultTrace,
  workerCount,
  sceneStateful,
  sceneFanout,
  sceneBudget,
  sceneCapacity,
} from "./scenes";

const TABS = [
  {
    name: "Stateful Core",
    title: "① Stateful Core — O(1) state vs KV-cache",
    sub: "RWKV / SSM collapse history into a fixed-size hidden state every step, so memory stays constant. A transformer must persist every key/value pair — memory grows with context.",
  },
  {
    name: "Fan-out",
    title: "② Orchestrator → Worker fan-out + verification",
    sub: "A task is decomposed into N 4K-chunk subtasks, fanned out to parallel workers (each runs its own tool-calling loop), vetted by a verification gate, then aggregated. Failed gates trigger a retry / escalation cascade.",
  },
  {
    name: "ContextBudget",
    title: "③ ContextBudget — hard 4K allocation",
    sub: "The window is split deterministically; the combined prompt is capped at 3000 tokens. The red rule is the hard limit — fits_prompt() rejects anything over it.",
  },
  {
    name: "CapacityPool",
    title: "④ CapacityPool — backend routing by capacity",
    sub: "Each backend advertises a Capacity; select(free, order) picks the first that fits. GPU and CPU pools are independent, so they run concurrently — full utilization.",
  },
];

type Stat = [string, string];

function statsFor(tab: number, time: number, speed: number, workers: number): Stat[] {
  if (tab === 0) {
    const tokens = Math.floor(time * 6.0);
    const cells = Math.min(time * 6.0, 60.0);
    return [
      ["tokens streamed", String(tokens)],
      ["RoCo state", "64 units"],
      ["KV-cache", `${Math.floor(cells)} cells`],
      ["state growth", "O(1)"],
    ];
  }
  if (tab === 1)
    return [
      ["workers", String(workers)],
      ["verify gates", String(workers)],
      ["retries", "0"],
      ["escalations", "0"],
    ];
  if (tab === 2) {
    const total = 700 + 1200 + 800 + 700 + 1536;
    return [
      ["total window", `${total} tok`],
      ["max prompt", "3000 tok"],
      ["fits_prompt()", "true"],
    ];
  }
  let active = 0;
  for (let i = 0; i < 5; i++) {
    const free = (0.5 + 0.5 * Math.sin(time * (0.6 + i * 0.15) * speed)) * 100.0;
    if (free < 45.0) active++;
  }
  return [
    ["backends", "5"],
    ["concurrent pools", "2"],
    ["active now", String(active)],
  ];
}

export default function App() {
  const [tab, setTab] = useState(0);
  const [playing, setPlaying] = useState(true);
  const [speed, setSpeed] = useState(1);
  const [time, setTime] = useState(0);

  const trace = useRef(defaultTrace()).current;
  const workers = useRef(workerCount(trace)).current;

  useEffect(() => {
    let raf = 0;
    const loop = () => {
      if (playing) setTime((t) => t + 0.016 * speed);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [playing, speed]);

  const svg =
    tab === 0
      ? sceneStateful(time)
      : tab === 1
        ? sceneFanout(time, workers)
        : tab === 2
          ? sceneBudget(time)
          : sceneCapacity(time, speed);

  const stats = statsFor(tab, time, speed, workers);
  const sub = TABS[tab].sub.replace("N 4K-chunk", `${workers} 4K-chunk`);

  return (
    <div id="main">
      <div className="hero">
        <div className="brand">
          <div className="logo" />
          <div>
            <h1>
              RoCo <span className="grad">AI</span> — Visualizer
            </h1>
            <div className="tag">RNN · RWKV · SSM agents — React (pnpm) web UI</div>
          </div>
        </div>
        <div className="controls">
          <button className="btn primary" onClick={() => setPlaying((p) => !p)}>
            {playing ? "❚❚ Pause" : "▶ Play"}
          </button>
          <span>speed</span>
          <input
            type="range"
            min={0.3}
            max={2.5}
            step={0.1}
            value={speed}
            onChange={(e) => setSpeed(parseFloat(e.target.value) || 1)}
          />
          <span>{speed.toFixed(1)}×</span>
        </div>
      </div>

      <div className="tabs">
        {TABS.map((t, i) => (
          <div
            key={t.name}
            className={"tab" + (i === tab ? " active" : "")}
            onClick={() => setTab(i)}
          >
            {t.name}
          </div>
        ))}
      </div>

      <div className="stage">
        <h2>{TABS[tab].title}</h2>
        <p className="sub">{sub}</p>
        <div id="scene" dangerouslySetInnerHTML={{ __html: svg }} />
        <div className="stats">
          {stats.map(([k, v]) => (
            <div className="stat" key={k}>
              <div className="k">{k}</div>
              <div className="v">{v}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="events">
        {trace.events
          .slice()
          .reverse()
          .slice(0, 40)
          .map((e, i) => (
            <div className="e" key={i}>
              <span className="p">● {e.phase}</span>
              <span>
                {e.actor}: {e.detail}
              </span>
            </div>
          ))}
      </div>

      <div className="footer">
        React + Vite + pnpm port of the RoCo visualizer. Data: ported{" "}
        <code>default_trace()</code> from <code>roco_core</code>.
      </div>
    </div>
  );
}
