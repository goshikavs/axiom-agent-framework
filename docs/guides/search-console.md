---
title: Google Search Console
description: How to verify the Goshikq documentation site and GitHub repository in Google Search Console, and how to request indexing without assuming the site is already indexed.
---

# Verify Goshikq in Google Search Console

Google does not index a GitHub repository or GitHub Pages site on a fixed schedule. A `site:` search with no hits means the URL is **not confirmed indexed**. This page records how to request crawling; it does not claim that indexing has happened.

## Properties to add

Add both of these as **URL prefix** properties:

1. Documentation site: `https://goshikavs.github.io/goshikq-agent-framework/`
2. GitHub repository: `https://github.com/goshikavs/goshikq-agent-framework/`

The old path `https://github.com/goshikavs/axiom-agent-framework/` redirects after the repository rename. Prefer the current name as the Search Console property.

## Verify the GitHub Pages site

After the docs workflow has published Pages:

1. Open [Google Search Console](https://search.google.com/search-console).
2. Add property → **URL prefix** → `https://goshikavs.github.io/goshikq-agent-framework/`.
3. Choose **HTML tag** verification.
4. Copy the `google-site-verification` meta tag content.
5. Put that value in `docs/site/.vitepress/config.ts` under `head`, for example:

   ```ts
   ["meta", { name: "google-site-verification", content: "PASTE_TOKEN_HERE" }]
   ```

6. Merge to `main` so GitHub Pages redeploys, then click **Verify**.

HTML-file verification also works: place the Google-supplied file in `docs/site/.vitepress/public/` (copied to the site root via Vite `publicDir`).

Do not commit a verification token until you have generated it in Search Console.

## Verify the GitHub repository

GitHub HTML cannot host an arbitrary Search Console meta tag. For `https://github.com/goshikavs/goshikq-agent-framework/`:

- Use **URL prefix** verification if Google offers a method you control, or
- Rely on the Pages property for documentation queries, and treat GitHub as a second listing once Google crawls it.

GitHub’s own `robots.txt` and crawl budget affect `github.com` URLs. A new or renamed public repository often stays unindexed for days or weeks.

## After verification

1. **URL inspection** → enter `https://goshikavs.github.io/goshikq-agent-framework/` → **Request indexing**.
2. Submit `https://goshikavs.github.io/goshikq-agent-framework/sitemap.xml` under Sitemaps.
3. Repeat inspection for `https://github.com/goshikavs/goshikq-agent-framework/` if that property verifies.
4. Wait. Recheck with:

   ```text
   site:github.com/goshikavs/goshikq-agent-framework
   site:goshikavs.github.io/goshikq-agent-framework
   ```

   Empty results still mean **not confirmed indexed**.

## What this repository does not do

- It does not ping Google automatically.
- It does not purchase ads or Search Console API credentials.
- It does not treat GitHub Actions success as a ranking signal.
