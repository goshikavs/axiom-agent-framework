import { NextRequest } from "next/server";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

function target(req: NextRequest, path: string[]): string {
  const base = (
    process.env.GOSHIKQ_STUDIO_RUNTIME_URL ||
    process.env.AXIOM_STUDIO_RUNTIME_URL ||
    process.env.GOSHIKQ_RUNTIME_URL ||
    process.env.AXIOM_RUNTIME_URL ||
    "http://127.0.0.1:7400"
  ).replace(/\/$/, "");
  const suffix = path.join("/");
  const url = new URL(req.url);
  return `${base}/${suffix}${url.search}`;
}

async function proxy(req: NextRequest, path: string[]) {
  const headers = new Headers();
  const pass = [
    "authorization",
    "content-type",
    "accept",
    "last-event-id",
    "x-goshikq-actor",
    "x-axiom-actor",
    "x-api-key",
    "x-goshikq-project-id",
    "x-axiom-project-id",
  ];
  for (const h of pass) {
    const v = req.headers.get(h);
    if (v) headers.set(h, v);
  }
  if (!headers.has("authorization") && !headers.has("x-api-key")) {
    const studioKey = process.env.GOSHIKQ_STUDIO_API_KEY || process.env.AXIOM_STUDIO_API_KEY;
    if (studioKey) headers.set("authorization", `Bearer ${studioKey}`);
  }
  const init: RequestInit = {
    method: req.method,
    headers,
    redirect: "manual",
  };
  if (req.method !== "GET" && req.method !== "HEAD") {
    init.body = await req.arrayBuffer();
  }
  const upstream = await fetch(target(req, path), init);
  const out = new Headers();
  upstream.headers.forEach((v, k) => {
    if (["content-type", "cache-control", "last-event-id"].includes(k.toLowerCase())) out.set(k, v);
  });
  if ((out.get("content-type") || "").includes("text/event-stream")) {
    out.set("cache-control", "no-cache, no-transform");
    out.set("x-accel-buffering", "no");
    out.set("connection", "keep-alive");
  }
  return new Response(upstream.body, { status: upstream.status, headers: out });
}

export async function GET(req: NextRequest, ctx: { params: Promise<{ path: string[] }> }) {
  const { path } = await ctx.params;
  return proxy(req, path);
}
export async function POST(req: NextRequest, ctx: { params: Promise<{ path: string[] }> }) {
  const { path } = await ctx.params;
  return proxy(req, path);
}
export async function PUT(req: NextRequest, ctx: { params: Promise<{ path: string[] }> }) {
  const { path } = await ctx.params;
  return proxy(req, path);
}
export async function PATCH(req: NextRequest, ctx: { params: Promise<{ path: string[] }> }) {
  const { path } = await ctx.params;
  return proxy(req, path);
}
export async function DELETE(req: NextRequest, ctx: { params: Promise<{ path: string[] }> }) {
  const { path } = await ctx.params;
  return proxy(req, path);
}
