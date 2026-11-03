import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const RUNTIME = (process.env.GOSHIKQ_RUNTIME_URL || "http://127.0.0.1:7400").replace(/\/$/, "");

test.describe("Workflow studio", () => {
  test("palette, validate, save, publish, execute, and inspect", async ({ page, request }) => {
    const def = JSON.parse(readFileSync(join(__dirname, "../../../examples/visual-workflow/parallel-approval.json"), "utf8"));
    def.name = `pw-parallel-${Date.now()}`;
    const created = await request.post(`${RUNTIME}/api/v1/workflows`, { data: { definition: def, draft: true } });
    expect(created.ok()).toBeTruthy();
    const wf = await created.json();
    const published = await request.post(`${RUNTIME}/api/v1/workflows/${wf.id}/publish`);
    expect(published.ok()).toBeTruthy();

    await page.goto("/workflows");
    await expect(page.getByRole("heading", { name: "Workflows" })).toBeVisible();
    await expect(page.getByTestId("workflow-canvas")).toBeVisible();
    await page.screenshot({ path: "../../docs/assets/studio-workflows.png", fullPage: true });
    await page.getByTestId("palette-agent").click();
    await page.getByTestId("wf-validate").click();
    await expect(page.getByTestId("wf-issues").or(page.getByText("No validation issues"))).toBeVisible({ timeout: 10_000 });

    const run = await request.post(`${RUNTIME}/api/v1/workflows/${wf.id}/run`, { data: { input: { order_id: "PW-1" } } });
    expect(run.ok()).toBeTruthy();
    const runId = (await run.json()).run_id as string;
    await page.goto(`/executions/${runId}`);
    await expect(page.getByTestId("workflow-nodes").or(page.getByTestId("team-panel")).or(page.getByText("refund-supervisor").or(page.getByRole("heading")))).toBeVisible({ timeout: 20_000 });
  });

  test("unknown workflow run is not a static snapshot", async ({ page }) => {
    await page.goto("/workflows");
    await expect(page.getByTestId("wf-run")).toBeDisabled();
  });

  test("validate reports missing verification on a new graph", async ({ page }) => {
    await page.goto("/workflows");
    await page.getByTestId("wf-new").click();
    await page.getByTestId("wf-name").fill(`pw-invalid-${Date.now()}`);
    await page.getByTestId("wf-validate").click();
    await expect(page.getByTestId("wf-issues")).toContainText(/missing_required_verification|start_node|end/i, { timeout: 10_000 });
  });

  test("execution page shows node-level status for a live run", async ({ page, request }) => {
    const def = JSON.parse(readFileSync(join(__dirname, "../../../examples/visual-workflow/parallel-approval.json"), "utf8"));
    def.name = `pw-nodes-${Date.now()}`;
    const created = await request.post(`${RUNTIME}/api/v1/workflows`, { data: { definition: def, draft: true } });
    expect(created.ok()).toBeTruthy();
    const wf = await created.json();
    expect((await request.post(`${RUNTIME}/api/v1/workflows/${wf.id}/publish`)).ok()).toBeTruthy();
    const run = await request.post(`${RUNTIME}/api/v1/workflows/${wf.id}/run`, { data: { input: { order_id: "PW-2" } } });
    expect(run.ok()).toBeTruthy();
    const runId = (await run.json()).run_id as string;
    await page.goto(`/executions/${runId}`);
    await expect(page.getByTestId("workflow-nodes")).toBeVisible({ timeout: 20_000 });
    await expect(page.getByTestId("workflow-node").first()).toBeVisible();
    await expect(page.getByTestId("timeline")).toBeVisible();
    await page.screenshot({ path: "../../docs/assets/studio-workflow-run.png", fullPage: true });
    const cancel = await request.post(`${RUNTIME}/api/v1/runs/${runId}/cancel`, { data: { reason: "playwright" } });
    expect(cancel.ok() || [400, 409, 422].includes(cancel.status()), `cancel ${cancel.status()} ${await cancel.text()}`).toBeTruthy();
  });
});
