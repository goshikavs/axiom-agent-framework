/**
 * Measure TypeScript worker round-trip against a live axiom-server.
 * Does not claim Bun is faster than Node or Python.
 */
import { Agent, Goal, MockModel, Runtime, StateSnapshot, defineTool } from "../src/index.ts";
import { z } from "zod";

const base = (process.env.AXIOM_RUNTIME_URL || "http://127.0.0.1:7400").replace(/\/$/, "");

async function ready(): Promise<boolean> {
  try {
    const r = await fetch(`${base}/health`);
    return r.ok;
  } catch {
    return false;
  }
}

const ping = defineTool({
  name: "ping",
  schema: z.object({ n: z.number() }),
  jsonSchema: { type: "object", properties: { n: { type: "number" } }, required: ["n"] },
  effect: "READ_ONLY",
  execute: async ({ n }) => ({ n, t: Date.now() }),
});

function agentFor(calls: number) {
  const script = [
    ...Array.from({ length: calls }, (_, i) => ({ tool_calls: [["ping", { n: i }] as [string, Record<string, unknown>]] })),
    { content: "done" },
  ];
  return new Agent({
    name: "ts_bench_ping",
    model: new MockModel({ script }),
    tools: [ping],
  });
}

async function runOnce(calls: number) {
  const t0 = performance.now();
  const rt = new Runtime(agentFor(calls), {
    stateProvider: { snapshot: () => new StateSnapshot({ state: { ok: true }, version: "1", source: "bench" }) },
  });
  const result = await rt.run(new Goal({ description: `bench ${calls}`, success: () => true, maxSteps: calls + 3 }));
  const ms = performance.now() - t0;
  return { ms, runId: result.runId, status: result.status, steps: result.stepCount };
}

if (!(await ready())) {
  console.log(JSON.stringify({ skipped: true, reason: "axiom-server not reachable" }));
  process.exit(0);
}

const results = [];
for (const n of [1, 5]) {
  const r = await runOnce(n);
  results.push({ tool_calls: n, ...r, ms_per_call: r.ms / n });
}
console.log(JSON.stringify({ runtime: base, results }, null, 2));
