import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const website = resolve(here, "..");
const root = resolve(website, "../..");

const DOCS = [
  { from: "docs/guides/installation.md", to: "docs/installation.md" },
  { from: "docs/ARCHITECTURE.md", to: "docs/core-concepts.md" },
  { from: "docs/architecture/outcome-verification.md", to: "docs/verification.md" },
  { from: "docs/guides/evidence-aware-agents.md", to: "docs/evidence.md" },
  { from: "docs/architecture/durable-recovery.md", to: "docs/durable-execution.md" },
  { from: "docs/architecture/multi-agent.md", to: "docs/multi-agent.md" },
  { from: "docs/architecture/workflow-execution.md", to: "docs/workflows.md" },
  { from: "docs/guides/workflow-builder.md", to: "docs/workflow-builder.md" },
  { from: "docs/guides/pending-external-operations.md", to: "docs/human-approval.md" },
  { from: "docs/studio/overview.md", to: "docs/studio.md" },
  { from: "docs/studio/execution-explorer.md", to: "docs/studio-explorer.md" },
  { from: "docs/studio/execution-debugging.md", to: "docs/studio-debugging.md" },
  { from: "docs/studio/replay.md", to: "docs/studio-replay.md" },
  { from: "docs/studio/run-comparison.md", to: "docs/studio-compare.md" },
  { from: "docs/integrations/mcp.md", to: "docs/integrations/mcp.md" },
  { from: "docs/integrations/a2a.md", to: "docs/integrations/a2a.md" },
  { from: "docs/integrations/opentelemetry.md", to: "docs/integrations/opentelemetry.md" },
  { from: "docs/integrations/langgraph.md", to: "docs/integrations/langgraph.md" },
  { from: "docs/integrations/openai-agents.md", to: "docs/integrations/openai-agents.md" },
  { from: "docs/architecture/security.md", to: "docs/security.md" },
  { from: "docs/security/authentication.md", to: "docs/security-authentication.md" },
  { from: "docs/operations/monitoring.md", to: "docs/operations.md" },
  { from: "docs/api/README.md", to: "docs/api-reference.md" },
  { from: "docs/guides/typescript-sdk.md", to: "docs/typescript-sdk.md" },
  { from: "docs/IMPLEMENTATION_STATUS.md", to: "docs/status.md" },
  { from: "docs/FEATURE_MATRIX.md", to: "docs/feature-matrix.md" },
  { from: "docs/troubleshooting.md", to: "docs/troubleshooting.md" },
  { from: "docs/contributing.md", to: "docs/contributing.md" },
  { from: "CHANGELOG.md", to: "docs/changelog.md" },
  { from: "docs/REMAINING_ROADMAP.md", to: "docs/roadmap.md" },
  { from: "docs/packaging.md", to: "docs/packaging.md" },
];

const GH = "https://github.com/goshikavs/goshikq-agent-framework/blob/main/";

function parseFrontmatter(raw) {
  if (!raw.startsWith("---\n")) return { fm: {}, body: raw };
  const end = raw.indexOf("\n---\n", 4);
  if (end < 0) return { fm: {}, body: raw };
  const yaml = raw.slice(4, end);
  const body = raw.slice(end + 5);
  const fm = {};
  for (const line of yaml.split("\n")) {
    const m = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!m) continue;
    fm[m[1]] = m[2].replace(/^["']|["']$/g, "");
  }
  return { fm, body };
}

function headingTitle(body) {
  const m = body.match(/^#\s+(.+)$/m);
  return m ? m[1].replace(/[*`]/g, "") : "Goshikq";
}

function rewrite(body, fromAbs) {
  return body.replace(/\]\(([^)]+)\)/g, (full, href) => {
    if (/^(https?:|mailto:|#|\/)/.test(href)) return full;
    const [pathPart, hash] = href.split("#");
    const suffix = hash ? `#${hash}` : "";
    if (/assets\/studio\//.test(pathPart) || /goshikq-.*\.(webp|png)$/.test(pathPart)) {
      const name = pathPart.split("/").pop();
      return `](/media/studio/${name}${suffix})`;
    }
    const resolved = resolve(dirname(fromAbs), pathPart);
    const rel = relative(root, resolved).replaceAll("\\", "/");
    const mapped = DOCS.find((d) => d.from === rel);
    if (mapped) {
      const slug = mapped.to.replace(/\.mdx?$/, "").replace(/\/index$/, "");
      return `](/${slug}/${suffix})`;
    }
    if (rel.startsWith("docs/") || rel.endsWith(".md")) return `](${GH}${rel}${suffix})`;
    return full;
  });
}

async function copyDocs() {
  const destRoot = join(website, "src/content/docs");
  for (const item of DOCS) {
    const fromAbs = join(root, item.from);
    const toAbs = join(destRoot, item.to);
    const raw = await readFile(fromAbs, "utf8");
    const { fm, body } = parseFrontmatter(raw);
    const title = fm.title || headingTitle(body);
    const description = fm.description || `${title} — Goshikq documentation.`;
    const rewritten = rewrite(body, fromAbs);
    const source = relative(root, fromAbs).replaceAll("\\", "/");
    const banner =
      `\n\n:::note[Canonical source]\nThis page is generated from [\`${source}\`](${GH}${source}) at build time. Edit the source file, not this copy.\n:::\n`;
    const out = `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n${rewritten.trim()}${banner}\n`;
    await mkdir(dirname(toAbs), { recursive: true });
    await writeFile(toAbs, out);
  }
}

async function copyStudioMedia() {
  const srcDir = join(root, "docs/assets/studio");
  const destDir = join(website, "public/media/studio");
  await mkdir(destDir, { recursive: true });
  const names = [
    "goshikq-studio-dashboard.webp",
    "goshikq-execution-explorer.webp",
    "goshikq-execution-explorer-langgraph.webp",
    "goshikq-execution-detail.webp",
    "goshikq-verification-report.webp",
    "goshikq-evidence-inspector.webp",
    "goshikq-multi-agent-hierarchy.webp",
    "goshikq-workflow-builder.webp",
    "goshikq-workflow-run.webp",
    "goshikq-approval-queue.webp",
    "goshikq-execution-replay.webp",
    "goshikq-run-comparison.webp",
    "goshikq-failure-analysis.webp",
    "goshikq-integrations.webp",
    "goshikq-models.webp",
    "goshikq-settings.webp",
    "goshikq-agents.webp",
    "goshikq-claims-execution.webp",
  ];
  for (const name of names) {
    await copyFile(join(srcDir, name), join(destDir, name));
  }
}

async function main() {
  await rm(join(website, "src/content/docs/guides"), { recursive: true, force: true });
  await rm(join(website, "src/content/docs/reference"), { recursive: true, force: true });
  await copyDocs();
  await copyStudioMedia();
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
