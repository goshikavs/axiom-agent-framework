// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

const site = process.env.PUBLIC_SITE_URL || "http://localhost:4321";
const base = process.env.PUBLIC_BASE_PATH || "/";

export default defineConfig({
  site,
  base,
  trailingSlash: "always",
  integrations: [
    starlight({
      title: "Goshikq",
      description:
        "Open-source AI agent framework with a Rust runtime for durable execution, multi-agent orchestration, and verified outcomes.",
      favicon: "/favicon.svg",
      logo: { src: "./src/assets/mark.svg", alt: "Goshikq" },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/goshikavs/goshikq-agent-framework",
        },
      ],
      lastUpdated: false,
      pagination: true,
      pagefind: true,
      customCss: ["./src/styles/starlight.css"],
      head: [
        { tag: "meta", attrs: { name: "theme-color", content: "#0c1116" } },
        { tag: "meta", attrs: { name: "color-scheme", content: "dark light" } },
        { tag: "meta", attrs: { property: "og:site_name", content: "Goshikq" } },
        { tag: "meta", attrs: { name: "twitter:card", content: "summary_large_image" } },
        {
          tag: "meta",
          attrs: { property: "og:image", content: new URL("media/studio/goshikq-studio-dashboard.webp", site).href },
        },
      ],
      sidebar: [
        { label: "Start", items: [
          { label: "Documentation home", slug: "docs" },
          { label: "Getting started", slug: "docs/getting-started" },
          { label: "Installation", slug: "docs/installation" },
          { label: "Implementation status", slug: "docs/status" },
        ]},
        { label: "Core", items: [
          { label: "Architecture", slug: "docs/core-concepts" },
          { label: "Verification", slug: "docs/verification" },
          { label: "Evidence", slug: "docs/evidence" },
          { label: "Durable execution", slug: "docs/durable-execution" },
          { label: "Human approval", slug: "docs/human-approval" },
          { label: "Multi-agent", slug: "docs/multi-agent" },
          { label: "Workflows", slug: "docs/workflows" },
          { label: "Workflow builder", slug: "docs/workflow-builder" },
        ]},
        { label: "Studio", items: [
          { label: "Overview", slug: "docs/studio" },
          { label: "Execution explorer", slug: "docs/studio-explorer" },
          { label: "Debugging", slug: "docs/studio-debugging" },
          { label: "Replay", slug: "docs/studio-replay" },
          { label: "Run comparison", slug: "docs/studio-compare" },
        ]},
        { label: "SDKs and APIs", items: [
          { label: "Python SDK", slug: "docs/python-sdk" },
          { label: "TypeScript SDK", slug: "docs/typescript-sdk" },
          { label: "HTTP API", slug: "docs/api-reference" },
        ]},
        { label: "Integrations", items: [
          { label: "MCP", slug: "docs/integrations/mcp" },
          { label: "A2A", slug: "docs/integrations/a2a" },
          { label: "OpenTelemetry", slug: "docs/integrations/opentelemetry" },
          { label: "LangGraph", slug: "docs/integrations/langgraph" },
          { label: "OpenAI Agents SDK", slug: "docs/integrations/openai-agents" },
        ]},
        { label: "Operate", items: [
          { label: "Security", slug: "docs/security" },
          { label: "Authentication", slug: "docs/security-authentication" },
          { label: "Operations", slug: "docs/operations" },
          { label: "Packaging", slug: "docs/packaging" },
          { label: "Troubleshooting", slug: "docs/troubleshooting" },
        ]},
        { label: "Project", items: [
          { label: "Feature matrix", slug: "docs/feature-matrix" },
          { label: "Changelog", slug: "docs/changelog" },
          { label: "Roadmap", slug: "docs/roadmap" },
          { label: "Contributing", slug: "docs/contributing" },
        ]},
      ],
    }),
  ],
});
