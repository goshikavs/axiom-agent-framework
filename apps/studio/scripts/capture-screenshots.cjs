const { chromium } = require("@playwright/test");
const path = require("path");
const out = path.resolve(__dirname, "../../../docs/studio/screenshots");

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  async function snap(url, name) {
    await page.goto(url, { waitUntil: "networkidle", timeout: 30000 });
    await page.waitForTimeout(800);
    await page.screenshot({ path: path.join(out, name), fullPage: true });
  }
  await snap("http://127.0.0.1:3000/", "overview.png");
  await snap("http://127.0.0.1:3000/executions", "executions.png");
  await snap("http://127.0.0.1:3000/executions?framework=langgraph", "executions-langgraph.png");
  await page.goto("http://127.0.0.1:3000/executions", { waitUntil: "networkidle" });
  await page.getByTestId("execution-link").first().click();
  await page.waitForTimeout(1500);
  await page.screenshot({ path: path.join(out, "execution-detail.png"), fullPage: true });
  await snap("http://127.0.0.1:3000/replay", "replay.png");
  await snap("http://127.0.0.1:3000/executions/compare", "compare.png");
  await snap("http://127.0.0.1:3000/integrations", "integrations.png");
  await browser.close();
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
