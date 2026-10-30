import { expect, test } from "@playwright/test";

test.describe("Studio production auth boundaries", () => {
  test("runtime proxy does not leak API keys in page HTML", async ({ page }) => {
    await page.goto("/executions");
    const html = await page.content();
    expect(html).not.toMatch(/axk_[0-9a-f]{32}/i);
    expect(html.toLowerCase()).not.toContain("authorization: bearer");
  });

  test("inspector redacts secret-like fields", async ({ page }) => {
    await page.goto("/executions?search=pw-missing-telemetry");
    const link = page.getByTestId("execution-link").first();
    await expect(link).toBeVisible({ timeout: 20_000 });
    await link.click();
    const inspector = page.getByTestId("inspector").or(page.getByText("Inspector"));
    await expect(inspector).toBeVisible({ timeout: 20_000 });
    const row = page.locator("ol button").first();
    if (await row.count()) {
      await row.click();
    }
    const text = await page.locator("body").innerText();
    expect(text).not.toMatch(/sk-[A-Za-z0-9]{20,}/);
    expect(text).not.toMatch(/axk_[0-9a-f]{32}/i);
  });

  test("role-restricted UI still loads empty/error states", async ({ page }) => {
    await page.goto("/executions?search=does-not-exist-v09");
    await expect(page.getByRole("heading", { name: "Executions" })).toBeVisible();
    await expect(page.getByTestId("executions-table").or(page.getByText(/No runs|no executions/i))).toBeVisible({
      timeout: 20_000,
    });
  });
});
