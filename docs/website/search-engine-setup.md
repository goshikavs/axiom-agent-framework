# Search engine setup (website)

A sitemap does not force indexing. This page is for the **Astro website**, not a claim that Google has indexed anything.

Current live documentation host: `https://goshikavs.github.io/goshikq-agent-framework/` (VitePress). The Astro site is not public until Cloudflare/GitHub Pages cutover is approved.

## After a public origin exists

1. Set `PUBLIC_SITE_URL` to that origin and rebuild.
2. Confirm `https://<origin>/sitemap-index.xml` and `/robots.txt` return 200.
3. Google Search Console → URL prefix property for that origin.
4. HTML-tag verification: put `google-site-verification` in `apps/website/astro.config.mjs` `head` (or a file in `apps/website/public/`). Do not commit a token until Search Console issues one.
5. Sitemap submission: paste `https://<origin>/sitemap-index.xml`.
6. URL Inspection → request indexing for `/` and `/docs/`. Requests are not a ranking guarantee.
7. Bing Webmaster Tools → import from Search Console or add the same prefix and sitemap.

## GitHub repository

Keep the repository URL-prefix property as documented in [guides/search-console.md](../guides/search-console.md).

## Recommended GitHub About (manual)

GitHub repository settings are not changed by this branch unless the owner updates them.

- Description: `Open-source AI agent framework with a Rust runtime for durable execution, multi-agent orchestration, and verified outcomes.`
- Website: the approved public origin (Cloudflare Pages or custom domain)
- Topics: `ai-agents`, `rust`, `python`, `typescript`, `durable-execution`, `multi-agent`, `open-source`

Do not rename the repository or change visibility.
