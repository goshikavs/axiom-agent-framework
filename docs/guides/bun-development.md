# Bun development

Axiom uses Bun as the JavaScript package manager and test runner for Studio and the TypeScript SDK. The execution engine is still Rust.

## Commands that work

From the repository root after `bun install`:

```bash
bun install
bun run dev          # Studio (apps/studio)
bun test             # @axiom-ai/sdk tests
bun run typecheck    # SDK tsc --noEmit
bun run build        # SDK typecheck (no separate emit bundle in this alpha)
```

SDK package:

```bash
cd sdk/typescript
bun test
bun run typecheck
```

CLI (after `cargo build -p axiom-cli`):

```bash
axiom init --template typescript
axiom dev                              # bun run dev in the current package
axiom run examples/typescript-refund-agent
axiom studio                           # URL, or starts apps/studio if down
axiom doctor
```

Commands that are not implemented are not listed.

## Node.js

`@axiom-ai/sdk` targets Node 20+ (`fetch`, `AbortController`). MCP stdio uses `node:child_process`. Do not assume `Bun.file` or `Bun.spawn` exist in Node.

## Quickstart

1. Start PostgreSQL and `axiom-server` (see root README).
2. `bun install`
3. `axiom run examples/typescript-refund-agent` or `cd examples/typescript-refund-agent && bun run start`
4. Open the printed Studio URL.
