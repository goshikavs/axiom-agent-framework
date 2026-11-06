import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

test.describe("Goshikq website", () => {
  test("homepage has Goshikq branding and no Axiom chrome", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: /Build AI Agents/i })).toBeVisible();
    await expect(page.getByRole("link", { name: "Get started" }).first()).toBeVisible();
    const body = await page.locator("body").innerText();
    expect(body).not.toMatch(/Axiom Studio/i);
    expect(body).toMatch(/Goshikq/);
  });

  test("quickstart and download are honest about unpublished packages", async ({ page }) => {
    await page.goto("/download/");
    await expect(page.getByText(/Packaged releases are not yet published/i).first()).toBeVisible();
    await expect(page.getByRole("link", { name: /main\.zip/i })).toHaveAttribute(
      "href",
      "https://github.com/goshikavs/goshikq-agent-framework/archive/refs/heads/main.zip",
    );
  });

  test("docs landing and search UI exist", async ({ page }) => {
    await page.goto("/docs/");
    await expect(page.getByRole("heading", { name: /Goshikq documentation/i })).toBeVisible();
    await expect(page.locator("body")).toContainText("Getting started");
  });

  test("studio gallery screenshots load", async ({ page }) => {
    await page.goto("/studio/");
    const img = page.locator("img[src*='goshikq-studio-dashboard']").first();
    await expect(img).toBeVisible();
    const res = await page.request.get("/media/studio/goshikq-studio-dashboard.webp");
    expect(res.ok()).toBeTruthy();
  });

  test("keyboard can reach skip link and primary nav", async ({ page }) => {
    await page.goto("/");
    await page.keyboard.press("Tab");
    await expect(page.getByRole("link", { name: "Skip to content" })).toBeFocused();
  });

  test("mobile nav toggle appears at 375px", async ({ page }, testInfo) => {
    test.skip(testInfo.project.name !== "mobile");
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Menu" })).toBeVisible();
  });
});

test.describe("build artifacts", () => {
  test("robots.txt allows indexing", async ({ request }) => {
    const res = await request.get("/robots.txt");
    expect(res.ok()).toBeTruthy();
    const text = await res.text();
    expect(text).toMatch(/Allow:\s*\//);
    expect(text).not.toMatch(/Disallow:\s*\//);
  });

  test("sitemap exists", async ({ request }) => {
    const res = await request.get("/sitemap-index.xml");
    expect(res.ok()).toBeTruthy();
    expect(await res.text()).toMatch(/sitemap/i);
  });
});

test("download clone command matches the public repo", async () => {
  const site = readFileSync(resolve("src/lib/site.ts"), "utf8");
  expect(site).toContain("https://github.com/goshikavs/goshikq-agent-framework.git");
  expect(site).not.toContain("pypi.org/project");
});
