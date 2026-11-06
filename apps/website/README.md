# Goshikq public website

Astro + Starlight static site for marketing pages and documentation. It does not start `goshikq-server`, PostgreSQL, or Studio.

```bash
cd apps/website
bun install
bun run build
bun run preview   # http://127.0.0.1:4321
```

Set `PUBLIC_SITE_URL` to the public origin before a production build (sitemap and canonical URLs).

Canonical technical docs remain in `/docs` at the repository root. `bun run build` copies a curated set into Starlight.
