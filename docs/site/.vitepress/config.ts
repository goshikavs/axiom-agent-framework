import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitepress";

const here = dirname(fileURLToPath(import.meta.url));
const vueRoot = resolve(here, "../node_modules/vue");

export default defineConfig({
  title: "Goshikq",
  description: "Outcome-driven AI agent framework",
  srcDir: "..",
  srcExclude: ["site/**"],
  ignoreDeadLinks: true,
  vite: {
    resolve: {
      alias: {
        "vue/server-renderer": resolve(vueRoot, "server-renderer/index.mjs"),
        vue: vueRoot,
      },
    },
  },
  themeConfig: {
    nav: [
      { text: "Introduction", link: "/ARCHITECTURE.md" },
      { text: "Quickstart", link: "/getting-started/quickstart.md" },
      { text: "Status", link: "/IMPLEMENTATION_STATUS.md" },
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
      { text: "Workflow designer", link: "/guides/workflow-builder.md" },
      { text: "Workflow execution", link: "/architecture/workflow-execution.md" },
      { text: "API", link: "/api/README.md" },
      { text: "Deployment", link: "/guides/installation.md" },
      { text: "Security", link: "/architecture/security.md" },
      { text: "Troubleshooting", link: "/troubleshooting.md" },
      { text: "Packaging", link: "/packaging.md" },
      { text: "Contributing", link: "/contributing.md" },
      { text: "Roadmap", link: "/REMAINING_ROADMAP.md" },
      { text: "ADRs", link: "/decisions/ADR-001-runtime-boundaries.md" },
    ],
  },
});
