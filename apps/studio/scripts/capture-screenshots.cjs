#!/usr/bin/env node
/**
 * Deprecated entrypoint. Docs screenshots now live under docs/assets/studio/
 * and are captured by `bun run screenshots:docs`.
 */
const { spawnSync } = require("node:child_process");
const path = require("node:path");

console.error("capture-screenshots.cjs is deprecated. Forwarding to bun run screenshots:docs");
const result = spawnSync("bun", ["scripts/capture-docs-screenshots.ts"], {
  cwd: path.resolve(__dirname, ".."),
  stdio: "inherit",
  shell: process.platform === "win32",
});
process.exit(result.status === null ? 1 : result.status);
