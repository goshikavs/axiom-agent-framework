/**
 * Capture Goshikq Studio documentation screenshots from a running local app.
 *
 * Prerequisites: goshikq-server on :7400, Studio on :3000, seeded fixtures.
 * Does not start those services and does not invent UI.
 *
 * Usage (repo root): bun run screenshots:docs
 */
import { mkdir, unlink, writeFile } from "node:fs/promises";
import { existsSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { chromium, type Page } from "@playwright/test";

const STUDIO = (process.env.PLAYWRIGHT_BASE_URL || "http://127.0.0.1:3000").replace(/\/$/, "");
const RUNTIME = (process.env.GOSHIKQ_RUNTIME_URL || "http://127.0.0.1:7400").replace(/\/$/, "");
const REPO = resolve(__dirname, "../../..");
const OUT = resolve(REPO, "docs/assets/studio");
const VIEWPORT = { width: 1600, height: 1000 };
const DEVICE_SCALE = 2;
const WEBP_QUALITY = 90;
const MANIFEST = join(OUT, "manifest.json");

const shots: {
  name: string;
  path: string;
  ok: boolean;
  pngBytes: number;
  webpBytes: number;
  axiomVisible: boolean;
}[] = [];

async function requireOk(url: string, label: string): Promise<void> {
  const resp = await fetch(url);
  if (!resp.ok) {
    throw new Error(`${label} not ready: GET ${url} -> ${resp.status} ${await resp.text()}`);
  }
}

async function hideChrome(page: Page): Promise<void> {
  await page.addStyleTag({
    content: [
      "nextjs-portal, [data-next-badge-root] { display: none !important; }",
      ".react-flow__minimap, .react-flow__controls { display: none !important; }",
    ].join("\n"),
  });
}

async function settleLiveBadges(page: Page): Promise<void> {
  await page.getByText("reconnecting", { exact: false }).first().waitFor({ state: "hidden", timeout: 8_000 }).catch(() => undefined);
  await page.getByText("Loading…").first().waitFor({ state: "hidden", timeout: 8_000 }).catch(() => undefined);
}

async function ready(page: Page, heading: string): Promise<void> {
  await hideChrome(page);
  await page.getByRole("link", { name: "Goshikq Studio" }).first().waitFor({ timeout: 20_000 });
  await page.getByRole("heading", { name: heading }).first().waitFor({ timeout: 20_000 });
  await page.getByText("Loading metrics from the runtime").first().waitFor({ state: "hidden", timeout: 15_000 }).catch(() => undefined);
  await settleLiveBadges(page);
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
  await page.waitForTimeout(400);
  const brand = await page.locator("body").innerText();
  if (/\bAxiom Studio\b/i.test(brand)) {
    throw new Error(`visible Axiom Studio chrome on ${page.url()}`);
  }
}

function convertWebp(pngPath: string): number {
  const py = `
from pathlib import Path
from PIL import Image
png = Path(r"""${pngPath}""")
webp = png.with_suffix(".webp")
im = Image.open(png).convert("RGBA")
im.save(webp, "WEBP", quality=${WEBP_QUALITY}, method=6)
print(webp.stat().st_size)
`;
  const result = spawnSync("python", ["-c", py], { encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(`webp conversion failed for ${pngPath}: ${result.stderr || result.stdout}`);
  }
  const bytes = Number.parseInt((result.stdout || "").trim(), 10);
  if (!Number.isFinite(bytes) || bytes <= 0) {
    throw new Error(`webp conversion produced no size for ${pngPath}: ${result.stdout}`);
  }
  return bytes;
}

async function snap(page: Page, stem: string, extra?: () => Promise<void>): Promise<void> {
  await hideChrome(page);
  await settleLiveBadges(page);
  if (extra) await extra();
  await page.mouse.move(0, 0);
  const pngPath = join(OUT, `${stem}.png`);
  const webpPath = join(OUT, `${stem}.webp`);
  await mkdir(dirname(pngPath), { recursive: true });
  await page.screenshot({ path: pngPath, fullPage: false, animations: "disabled" });
  const pngBytes = statSync(pngPath).size;
  const webpBytes = convertWebp(pngPath);
  await unlink(pngPath);
  const text = await page.locator("body").innerText();
  shots.push({
    name: `${stem}.webp`,
    path: relative(REPO, webpPath).replaceAll("\\", "/"),
    ok: true,
    pngBytes,
    webpBytes,
    axiomVisible: /\bAxiom Studio\b/i.test(text),
  });
  console.log(`wrote ${stem}.webp (${webpBytes} bytes, png was ${pngBytes})`);
}

async function latestRun(agent: string): Promise<string> {
  const url = `${RUNTIME}/api/v1/runs?agent=${encodeURIComponent(agent)}&limit=1`;
  const resp = await fetch(url);
  if (!resp.ok) throw new Error(`runs?agent=${agent} -> ${resp.status}`);
  const body = (await resp.json()) as { items?: { id: string }[] };
  const id = body.items?.[0]?.id;
  if (!id) throw new Error(`no persisted run for agent ${agent}`);
  return id;
}

async function maybeRun(agent: string): Promise<string | null> {
  try {
    return await latestRun(agent);
  } catch {
    return null;
  }
}

async function main(): Promise<void> {
  await requireOk(`${RUNTIME}/health`, "goshikq-server");
  await requireOk(STUDIO, "Goshikq Studio");
  await mkdir(OUT, { recursive: true });

  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: VIEWPORT,
    deviceScaleFactor: DEVICE_SCALE,
    baseURL: STUDIO,
  });
  const page = await context.newPage();

  const hello = await maybeRun("hello-agent");
  const supervisor = await maybeRun("refund-supervisor");
  const claims = await maybeRun("claims_agent");

  await page.goto("/");
  await ready(page, "Overview");
  await snap(page, "goshikq-studio-dashboard");

  await page.goto("/executions?agent=hello-agent");
  await ready(page, "Executions");
  await page.getByTestId("executions-table").or(page.getByText("No runs")).waitFor({ timeout: 20_000 });
  await snap(page, "goshikq-execution-explorer");

  await page.goto("/executions?framework=langgraph");
  await ready(page, "Executions");
  await snap(page, "goshikq-execution-explorer-langgraph");

  if (hello) {
    await page.goto(`/executions/${hello}`);
    await page.getByText("Execution graph").waitFor({ timeout: 20_000 });
    await page.getByTestId("timeline").waitFor();
    const row = page.locator("ol button").first();
    if (await row.count()) {
      await row.click();
      await page.getByTestId("inspector").getByText("Loading…").waitFor({ state: "hidden", timeout: 20_000 }).catch(() => undefined);
      await page.waitForTimeout(600);
    }
    await snap(page, "goshikq-execution-detail");

    await page.goto(`/executions/${hello}/verification`);
    await ready(page, "Goal verification");
    await snap(page, "goshikq-verification-report");

    await page.goto(`/executions/${hello}/evidence`);
    await ready(page, "Evidence intelligence");
    await snap(page, "goshikq-evidence-inspector");
  }

  if (supervisor) {
    await page.goto(`/executions/${supervisor}`);
    await page.getByTestId("team-panel").waitFor({ timeout: 20_000 });
    await snap(page, "goshikq-multi-agent-hierarchy");
  }

  await page.goto("/workflows");
  await ready(page, "Workflows");
  await page.getByTestId("workflow-canvas").waitFor({ timeout: 20_000 });
  const saved = page.locator("[data-testid^='wf-saved-pw-docs-workflow']").first();
  if (await saved.count()) await saved.click();
  await page.waitForTimeout(800);
  await snap(page, "goshikq-workflow-builder");

  const wfRuns = await fetch(`${RUNTIME}/api/v1/runs?search=pw-docs-workflow&limit=1`);
  if (wfRuns.ok) {
    const body = (await wfRuns.json()) as { items?: { id: string }[] };
    const id = body.items?.[0]?.id;
    if (id) {
      await page.goto(`/executions/${id}`);
      await page.getByTestId("workflow-nodes").or(page.getByText("Execution graph")).waitFor({ timeout: 20_000 });
      await snap(page, "goshikq-workflow-run");
    }
  }

  await page.goto("/approvals");
  await ready(page, "Approvals");
  await snap(page, "goshikq-approval-queue");

  await page.goto("/replay");
  await ready(page, "Historical replay");
  const replay = page.getByTestId("replay-select");
  const helloOpt = replay.locator("option").filter({ hasText: "hello-agent" }).first();
  if (await helloOpt.count()) {
    const value = await helloOpt.getAttribute("value");
    if (value) await replay.selectOption(value);
    await page.getByTestId("replay-forward").click();
    await page.getByText("Loading…").waitFor({ state: "hidden", timeout: 15_000 }).catch(() => undefined);
  }
  await snap(page, "goshikq-execution-replay");

  await page.goto("/executions/compare");
  await ready(page, "Compare runs");
  const a = page.getByTestId("compare-a");
  const b = page.getByTestId("compare-b");
  const helloA = a.locator("option").filter({ hasText: "hello-agent" }).first();
  const superB = b.locator("option").filter({ hasText: "refund-supervisor" }).first();
  if ((await helloA.count()) && (await superB.count())) {
    const va = await helloA.getAttribute("value");
    const vb = await superB.getAttribute("value");
    if (va) await a.selectOption(va);
    if (vb) await b.selectOption(vb);
    await page.getByTestId("compare-summaries").waitFor({ timeout: 10_000 }).catch(() => undefined);
  }
  await snap(page, "goshikq-run-comparison");

  await page.goto("/errors");
  await ready(page, "Failure analysis");
  const fail = page.getByTestId("failure-run").first();
  if (await fail.count()) {
    await fail.click();
    await page.getByTestId("failure-panel").waitFor({ timeout: 10_000 }).catch(() => undefined);
  }
  await snap(page, "goshikq-failure-analysis");

  await page.goto("/integrations");
  await ready(page, "Integrations");
  await snap(page, "goshikq-integrations");

  await page.goto("/models");
  await ready(page, "Models");
  await snap(page, "goshikq-models");

  await page.goto("/settings");
  await ready(page, "Settings");
  await snap(page, "goshikq-settings");

  await page.goto("/agents");
  await ready(page, "Agents");
  await snap(page, "goshikq-agents");

  if (claims) {
    await page.goto(`/executions/${claims}`);
    await page.getByText("Execution graph").or(page.getByText("Recovery")).waitFor({ timeout: 20_000 });
    await snap(page, "goshikq-claims-execution");
  }

  await browser.close();
  await writeFile(
    MANIFEST,
    JSON.stringify(
      {
        captured_at: new Date().toISOString(),
        studio: STUDIO,
        runtime: RUNTIME,
        viewport: VIEWPORT,
        deviceScaleFactor: DEVICE_SCALE,
        webpQuality: WEBP_QUALITY,
        hello,
        supervisor,
        claims,
        shots,
      },
      null,
      2,
    ),
  );
  const branded = shots.filter((s) => s.axiomVisible);
  if (branded.length) {
    throw new Error(`Axiom Studio still visible in ${branded.map((s) => s.name).join(", ")}`);
  }
  if (!existsSync(join(OUT, "goshikq-studio-dashboard.webp"))) {
    throw new Error("dashboard screenshot missing after capture");
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
