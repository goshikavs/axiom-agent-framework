export type NavItem = {
  href: string;
  label: string;
  status: "available" | "experimental" | "planned";
};

export type NavSection = {
  title: string;
  items: NavItem[];
};

export const NAV: NavSection[] = [
  {
    title: "",
    items: [{ href: "/", label: "Overview", status: "available" }],
  },
  {
    title: "Build",
    items: [
      { href: "/agents", label: "Agents", status: "available" },
      { href: "/workflows", label: "Workflows", status: "available" },
      { href: "/tools", label: "Tools", status: "planned" },
      { href: "/goals", label: "Goals", status: "planned" },
    ],
  },
  {
    title: "Observe",
    items: [
      { href: "/executions", label: "Executions", status: "available" },
      { href: "/traces", label: "Traces", status: "available" },
      { href: "/events", label: "Events", status: "available" },
      { href: "/performance", label: "Performance", status: "available" },
    ],
  },
  {
    title: "Debug",
    items: [
      { href: "/replay", label: "Execution Replay", status: "available" },
      { href: "/executions/compare", label: "Compare", status: "available" },
      { href: "/state", label: "State Inspector", status: "available" },
      { href: "/verification", label: "Verification", status: "available" },
      { href: "/evidence", label: "Evidence", status: "available" },
      { href: "/errors", label: "Errors", status: "available" },
    ],
  },
  {
    title: "Test",
    items: [
      { href: "/evaluations", label: "Evaluations", status: "planned" },
      { href: "/regression", label: "Regression Tests", status: "planned" },
      { href: "/benchmarks", label: "Benchmarks", status: "planned" },
    ],
  },
  {
    title: "Platform",
    items: [
      { href: "/approvals", label: "Approvals", status: "available" },
      { href: "/integrations", label: "Integrations", status: "available" },
      { href: "/models", label: "Models", status: "available" },
      { href: "/settings", label: "Settings", status: "available" },
    ],
  },
];
