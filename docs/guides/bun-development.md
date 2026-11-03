# Bun development

Goshikq uses Bun as the JavaScript package manager and test runner for Studio and the TypeScript SDK. The execution engine is still Rust.

## Commands that work

From the repository root after `bun install`:

```bash
bun install
bun run dev          # Studio (apps/studio)
bun test             # @goshikq/sdk tests
bun run typecheck    # SDK tsc --noEmit
bun run build        # SDK typecheck (no separate emit bundle in this alpha)
```

SDK package:

```bash
cd sdk/typescript
bun test
bun run typecheck
```

CLI (after `cargo build -p goshikq-cli`):

```bash
goshikq init --template typescript
goshikq dev                              # bun run dev in the current package
goshikq run examples/typescript-refund-agent
goshikq studio                           # URL, or starts apps/studio if down
goshikq doctor
```

Commands that are not implemented are not listed.

## Node.js

`@goshikq/sdk` targets Node 20+ (`fetch`, `AbortController`). MCP stdio uses `node:child_process`. Do not assume `Bun.file` or `Bun.spawn` exist in Node.

## Quickstart

1. Start PostgreSQL and `goshikq-server` (see root README).
2. `bun install`
3. `goshikq run examples/typescript-refund-agent` or `cd examples/typescript-refund-agent && bun run start`
4. Open the printed Studio URL.
