"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { NAV } from "@/lib/nav";
import { cn } from "@/lib/utils";

export function Shell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const [collapsed, setCollapsed] = useState(false);
  const [theme, setTheme] = useState<"dark" | "light">("dark");

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.classList.toggle("light", theme === "light");
    document.documentElement.style.colorScheme = theme;
    if (theme === "light") {
      document.body.classList.add("bg-slate-100", "text-slate-900");
      document.body.classList.remove("bg-canvas", "text-ink");
    } else {
      document.body.classList.remove("bg-slate-100", "text-slate-900");
      document.body.classList.add("bg-canvas", "text-ink");
    }
  }, [theme]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "[" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setCollapsed((c) => !c);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="flex min-h-screen">
      <aside
        className={cn(
          "sticky top-0 flex h-screen shrink-0 flex-col border-r border-line bg-canvas-raised transition-[width]",
          collapsed ? "w-14" : "w-60",
        )}
      >
        <div className="flex items-center justify-between gap-2 border-b border-line px-3 py-3">
          {!collapsed ? (
            <Link href="/" className="font-semibold tracking-tight">
              Goshikq Studio
            </Link>
          ) : (
            <Link href="/" className="font-mono text-sm" aria-label="Goshikq Studio">
              GQ
            </Link>
          )}
          <button
            type="button"
            onClick={() => setCollapsed((c) => !c)}
            className="rounded p-1 text-ink-faint hover:bg-canvas-overlay hover:text-ink"
            aria-label="Collapse sidebar"
            title="Ctrl+["
          >
            {collapsed ? "»" : "«"}
          </button>
        </div>
        <nav className="flex-1 overflow-y-auto px-2 py-3 scrollbar-thin">
          {NAV.map((section) => (
            <div key={section.title || "root"} className="mb-4">
              {section.title && !collapsed ? (
                <p className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-ink-faint">{section.title}</p>
              ) : null}
              <ul className="space-y-0.5">
                {section.items.map((item) => {
                  const active = item.href === "/" ? pathname === "/" : pathname === item.href || pathname.startsWith(`${item.href}/`);
                  return (
                    <li key={item.href}>
                      <Link
                        href={item.href}
                        title={item.label}
                        className={cn(
                          "flex items-center justify-between rounded px-2 py-1.5 text-[13px]",
                          active ? "bg-accent/15 text-ink" : "text-ink-muted hover:bg-canvas-overlay hover:text-ink",
                        )}
                      >
                        <span className="truncate">{collapsed ? item.label.slice(0, 2) : item.label}</span>
                        {!collapsed && item.status !== "available" ? (
                          <span className="text-[9px] uppercase tracking-wide text-ink-faint">{item.status === "planned" ? "soon" : "exp"}</span>
                        ) : null}
                      </Link>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </nav>
        <div className="border-t border-line p-2">
          <button
            type="button"
            onClick={() => setTheme((t) => (t === "dark" ? "light" : "dark"))}
            className="w-full rounded px-2 py-1.5 text-left text-xs text-ink-muted hover:bg-canvas-overlay"
          >
            {collapsed ? (theme === "dark" ? "☀" : "☾") : theme === "dark" ? "Light mode" : "Dark mode"}
          </button>
        </div>
      </aside>
      <main className="min-w-0 flex-1">
        <div className={pathname.startsWith("/executions/") && pathname.split("/").length >= 3 ? "h-full px-4 py-3" : "mx-auto max-w-[1600px] px-6 py-5"}>{children}</div>
      </main>
    </div>
  );
}
