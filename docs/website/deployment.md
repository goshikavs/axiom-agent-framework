# Website deployment

Do not purchase a domain, change DNS, or connect production hosting until the owner approves.

## Intended host: Cloudflare Pages (free tier)

1. Cloudflare Dashboard → Workers & Pages → Create → Pages.
2. Connect GitHub repository `goshikavs/goshikq-agent-framework`.
3. Project name: `goshikq` (this becomes `goshikq.pages.dev` if available).
4. Build settings:

| Setting | Value |
| --- | --- |
| Root directory | `apps/website` |
| Build command | `bun install && bun run build` |
| Build output | `dist` |
| Node / Bun | Enable Bun, or use the Cloudflare Bun build image |
| Environment | `PUBLIC_SITE_URL=https://<project>.pages.dev` (later the custom domain) |

5. Preview deployments: enable for pull requests if the Cloudflare GitHub app is connected.
6. Custom domain (later): add `www` or apex in Cloudflare DNS as CNAME/ALIAS to the pages.dev target. HTTPS is automatic.

Rollback: Cloudflare Pages → Deployments → previous successful deploy → Retry / Rollback.

## GitHub Pages

The existing `docs.yml` workflow still publishes **VitePress** from `docs/site`. This website does not replace that workflow until cutover is approved. Running both on the same hostname would duplicate documentation.

## Local production build

```bash
cd apps/website
PUBLIC_SITE_URL=http://127.0.0.1:4321 bun run build
bun run preview
```
