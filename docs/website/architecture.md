# Public website architecture

The public site lives in `apps/website`. It is a static Astro 7 + Starlight 0.42 app.

## Why Astro / Starlight instead of extending VitePress

`docs/site` VitePress remains the **currently deployed** GitHub Pages host (`https://goshikavs.github.io/goshikq-agent-framework/`). Replacing it in production is an owner decision.

Astro is used for the public product website because:

1. Marketing routes (`/`, `/studio`, `/download`, `/features`) need a custom design system, not a docs-theme splash page.
2. Starlight provides docs search (Pagefind), sidebar, tabs, and asides with almost no client JavaScript.
3. The output is static files suitable for Cloudflare Pages free tier. No runtime, CMS, database, or paid search.

Do not run Goshikq Runtime, PostgreSQL, or Studio to serve this site.

## Content

| Kind | Location | Source of truth |
| --- | --- | --- |
| Marketing pages | `apps/website/src/pages` | Written for the site; claims taken from README / implementation status |
| Hand-written docs | `apps/website/src/content/docs/docs/{index,getting-started,python-sdk}.mdx` | Short pages that point at repo docs |
| Generated docs | `scripts/prepare.mjs` | Copies curated files from `docs/` and `CHANGELOG.md` |
| Studio images | `public/media/studio` at build | Copied from `docs/assets/studio/*.webp` |

Generated files are gitignored. Edit the repository `docs/` files.

## Independence

The website package does not import Studio or SDK runtime code. CI can build it with Bun only.
