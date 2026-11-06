import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type HeadConfig } from "vitepress";

const here = dirname(fileURLToPath(import.meta.url));
const vueRoot = resolve(here, "../node_modules/vue");

const SITE_ORIGIN = "https://goshikavs.github.io";
const SITE_BASE = "/goshikq-agent-framework/";
const SITE_URL = `${SITE_ORIGIN}${SITE_BASE}`;
const SITE_DESCRIPTION =
  "Open-source AI agent framework with a Rust runtime for durable execution, multi-agent orchestration, and verified outcomes.";

function canonicalPath(relativePath: string): string {
  const trimmed = relativePath.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, "");
  if (!trimmed) {
    return SITE_BASE;
  }
  return `${SITE_BASE}${trimmed}.html`;
}

export default defineConfig({
  lang: "en-US",
  title: "Goshikq Docs",
  titleTemplate: ":title | Goshikq",
  description: SITE_DESCRIPTION,
  base: SITE_BASE,
  srcDir: "..",
  srcExclude: ["site/**", "audits/**"],
  lastUpdated: true,
  ignoreDeadLinks: [/^https?:\/\/127\.0\.0\.1/, /^https?:\/\/localhost/],
  // Hostname must end with `/` so relative sitemap paths keep `/goshikq-agent-framework/`.
  sitemap: {
    hostname: SITE_URL,
    transformItems(items) {
      return items.map((item) => ({
        ...item,
        url: item.url.replace(/^https?:\/\/[^/]+/, "").replace(/^\//, ""),
      }));
    },
  },
  head: [
    ["meta", { name: "theme-color", content: "#111827" }],
    ["meta", { name: "robots", content: "index,follow" }],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:site_name", content: "Goshikq Docs" }],
    ["meta", { name: "twitter:card", content: "summary" }],
  ],
  transformPageData(pageData) {
    const path = canonicalPath(pageData.relativePath);
    const canonical = `${SITE_ORIGIN}${path}`;
    const title = pageData.title ? `${pageData.title} | Goshikq` : "Goshikq Docs";
    const description = pageData.description || SITE_DESCRIPTION;
    const extra: HeadConfig[] = [
      ["link", { rel: "canonical", href: canonical }],
      ["meta", { property: "og:url", content: canonical }],
      ["meta", { property: "og:title", content: title }],
      ["meta", { property: "og:description", content: description }],
      ["meta", { name: "description", content: description }],
    ];
    pageData.frontmatter.head = [...(pageData.frontmatter.head ?? []), ...extra];
  },
  vite: {
    publicDir: resolve(here, "public"),
    resolve: {
      alias: {
        "vue/server-renderer": resolve(vueRoot, "server-renderer/index.mjs"),
        vue: vueRoot,
      },
    },
  },
  themeConfig: {
    siteTitle: "Goshikq",
    search: { provider: "local" },
    socialLinks: [{ icon: "github", link: "https://github.com/goshikavs/goshikq-agent-framework" }],
    editLink: {
      pattern: "https://github.com/goshikavs/goshikq-agent-framework/edit/main/docs/:path",
      text: "Edit this page on GitHub",
    },
    nav: [
      { text: "Introduction", link: "/ARCHITECTURE.md" },
      { text: "Quickstart", link: "/getting-started/quickstart.md" },
      { text: "Status", link: "/IMPLEMENTATION_STATUS.md" },
      { text: "GitHub", link: "https://github.com/goshikavs/goshikq-agent-framework" },
    ],
    sidebar: [
      { text: "Architecture", link: "/ARCHITECTURE.md" },
      { text: "Quickstart", link: "/getting-started/quickstart.md" },
      { text: "Installation", link: "/guides/installation.md" },
      { text: "CLI", link: "/guides/cli.md" },
      { text: "Python SDK", link: "/getting-started/quickstart.md" },
      { text: "TypeScript SDK", link: "/guides/typescript-sdk.md" },
      { text: "Goals and contracts", link: "/architecture/outcome-verification.md" },
      { text: "Tool registration", link: "/architecture/action-safety-gates.md" },
      { text: "Evidence", link: "/guides/evidence-aware-agents.md" },
      { text: "Recovery", link: "/guides/crash-recovery.md" },
      { text: "Multi-agent", link: "/architecture/multi-agent.md" },
      { text: "Shared context", link: "/architecture/shared-context.md" },
      { text: "MCP", link: "/integrations/mcp.md" },
      { text: "A2A", link: "/integrations/a2a.md" },
      { text: "OpenTelemetry", link: "/integrations/opentelemetry.md" },
      { text: "LangGraph", link: "/integrations/langgraph.md" },
      { text: "OpenAI Agents SDK", link: "/integrations/openai-agents.md" },
      { text: "Studio", link: "/studio/overview.md" },
      { text: "Studio screenshots", link: "/guides/studio-screenshots.md" },
      { text: "Workflow designer", link: "/guides/workflow-builder.md" },
      { text: "Workflow execution", link: "/architecture/workflow-execution.md" },
      { text: "API", link: "/api/README.md" },
      { text: "Deployment", link: "/guides/installation.md" },
      { text: "Security", link: "/architecture/security.md" },
      { text: "Troubleshooting", link: "/troubleshooting.md" },
      { text: "Packaging", link: "/packaging.md" },
      { text: "Search Console", link: "/guides/search-console.md" },
      { text: "Contributing", link: "/contributing.md" },
      { text: "Roadmap", link: "/REMAINING_ROADMAP.md" },
      { text: "ADRs", link: "/decisions/ADR-001-runtime-boundaries.md" },
    ],
    footer: {
      message: "Apache-2.0. Alpha software; APIs will change.",
      copyright: `Source: ${SITE_URL}`,
    },
  },
});
