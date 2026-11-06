# Website development

From `apps/website`:

```bash
bun run dev      # http://127.0.0.1:4321
bun run build
bun run preview
bun run test:e2e # requires a prior build
```

Canonical technical documentation is in the repository `docs/` tree. `scripts/prepare.mjs` copies a curated set into Starlight at build time.
