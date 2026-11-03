import { expect, test } from "@playwright/test";

test.describe("Goshikq Studio v0.5", () => {
  test("execution list loads", async ({ page }) => {
    await page.goto("/executions");
    await expect(page.getByRole("heading", { name: "Executions" })).toBeVisible();
    await expect(page.getByTestId("executions-table").or(page.getByText("No runs"))).toBeVisible({ timeout: 20_000 });
  });

  test("filters update the URL", async ({ page }) => {
    await page.goto("/executions");
    await page.getByTestId("filter-framework").fill("langgraph");
    await expect(page).toHaveURL(/framework=langgraph/);
  });

  test("detail page loads persisted events and inspector payload", async ({ page }) => {
    await page.goto("/executions");
    const link = page.getByTestId("execution-link").first();
    await expect(link).toBeVisible({ timeout: 20_000 });
    await link.click();
    await expect(page.getByText("Execution graph")).toBeVisible();
    await expect(page.getByTestId("timeline")).toBeVisible();
    const row = page.locator("ol button").first();
    if (await row.count()) {
      await row.click();
      await expect(page.getByText("Inspector", { exact: true })).toBeVisible();
      await expect(page.getByText("Copy JSON")).toBeVisible();
    }
  });

  test("graph and timeline share selection", async ({ page }) => {
    await page.goto("/executions");
    await page.getByTestId("execution-link").first().click();
    const row = page.locator("ol button").first();
    await expect(row).toBeVisible({ timeout: 20_000 });
    await row.click();
    await expect(row).toHaveClass(/border-accent/);
  });

  test("external langgraph run is UNVERIFIED", async ({ page }) => {
    await page.goto("/executions?framework=langgraph");
    const row = page.getByTestId("execution-row").first();
    if (await row.count()) {
      await expect(row).toContainText("UNVERIFIED");
      await expect(row).not.toContainText("VERIFIED");
    }
  });

  test("missing telemetry is labeled on an otel run", async ({ page }) => {
    await page.goto("/executions?framework=otel&search=pw-missing-telemetry");
    const row = page.getByTestId("execution-row").filter({ hasText: "pw-missing-telemetry" }).first();
    await expect(row).toBeVisible({ timeout: 20_000 });
    await row.getByTestId("execution-link").click();
    const buttons = page.locator("ol button");
    await expect(buttons.first()).toBeVisible({ timeout: 20_000 });
    const n = await buttons.count();
    for (let i = 0; i < n; i++) {
      await buttons.nth(i).click();
      const missing = page.getByText("Missing attributes");
      if (await missing.count()) {
        await expect(missing).toBeVisible();
        await expect(page.getByText(/unknown, not fabricated/i)).toBeVisible();
        return;
      }
    }
    throw new Error("seeded pw-missing-telemetry run had no Missing attributes label");
  });

  test("historical replay is GET-only", async ({ page }) => {
    const posts: string[] = [];
    page.on("request", (req) => {
      if (req.method() !== "GET" && req.method() !== "HEAD" && req.method() !== "OPTIONS") posts.push(`${req.method()} ${req.url()}`);
    });
    await page.goto("/replay");
    const select = page.getByTestId("replay-select");
    await expect(select).toBeVisible();
    const options = await select.locator("option").count();
    if (options > 1) {
      await select.selectOption({ index: 1 });
      await page.getByTestId("replay-forward").click();
    }
    expect(posts.filter((p) => !p.includes("query"))).toEqual([]);
  });

  test("run comparison renders summaries", async ({ page }) => {
    await page.goto("/executions/compare");
    const a = page.getByTestId("compare-a");
    const opts = await a.locator("option").count();
    if (opts > 2) {
      await a.selectOption({ index: 1 });
      await page.getByTestId("compare-b").selectOption({ index: 2 });
      await expect(page.getByTestId("compare-summaries")).toBeVisible();
    }
  });

  test("evidence page uses persisted records", async ({ page }) => {
    await page.goto("/executions");
    await page.getByTestId("execution-link").first().click();
    await page.getByTestId("run-evidence-link").click();
    await expect(page.getByRole("heading", { name: "Evidence intelligence" })).toBeVisible({ timeout: 20_000 });
  });

  test("unknown run is not accessible", async ({ page }) => {
    await page.goto("/executions/00000000-0000-0000-0000-000000000099");
    await expect(page.getByText(/Runtime unreachable|not found|error/i)).toBeVisible({ timeout: 15_000 });
  });

  test("multi-agent panel shows persisted parent/child", async ({ page, request }) => {
    const runtime = (process.env.GOSHIKQ_RUNTIME_URL || "http://127.0.0.1:7400").replace(/\/$/, "");
    const parent = await request.post(`${runtime}/api/v1/runs`, {
      data: {
        agent: { name: "pw-supervisor", model: "mock/deterministic", tools: [], execution_limits: { max_steps: 2, deadline_seconds: 30 } },
        goal: { description: "playwright multi-agent fixture", verification_policy: "none" },
        defer_start: true,
        metadata: { example: "playwright-multi-agent" },
      },
    });
    expect(parent.ok()).toBeTruthy();
    const pid = (await parent.json()).run_id as string;
    const child = await request.post(`${runtime}/api/v1/runs/${pid}/tasks`, {
      data: {
        agent: { name: "pw-worker", model: "mock/deterministic", tools: [], execution_limits: { max_steps: 2, deadline_seconds: 30 } },
        goal: { description: "playwright child", verification_policy: "none" },
        defer_start: true,
        role: "worker",
      },
    });
    expect(child.ok()).toBeTruthy();
    await page.goto(`/executions/${pid}`);
    await expect(page.getByTestId("team-panel")).toBeVisible({ timeout: 20_000 });
    await expect(page.getByTestId("delegation-graph")).toBeVisible();
    await expect(page.getByTestId("child-agent")).toContainText("pw-worker");
    const childBtn = page.getByTestId("handoff-events").locator("button").first();
    if (await childBtn.count()) {
      await childBtn.click();
      await expect(page.getByTestId("inspector")).toBeVisible();
    }
  });

  test("live SSE shows a connection state", async ({ page }) => {
    await page.goto("/executions");
    await page.getByTestId("execution-link").first().click();
    await expect(page.getByTestId("sse-conn")).toBeVisible({ timeout: 20_000 });
    await page.route("**/events/stream**", (route) => route.abort());
    await expect(page.getByTestId("sse-conn")).toContainText(/connected|reconnecting|completed|disconnected/);
  });
});
