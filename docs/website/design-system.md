# Goshikq website design system

Tokens: `apps/website/src/styles/tokens.css`.

## Color

| Token | Dark | Use |
| --- | --- | --- |
| `--gk-ink` | `#0c1116` | Page background |
| `--gk-text` | `#e8eef4` | Body text |
| `--gk-verify` | `#3ddc97` | Accent / verified |
| `--gk-run` | `#e8b84a` | Execution / caution |
| `--gk-fail` | `#e8614a` | Failure |

Light theme inverts ink/paper. Accent stays green, not purple. No particle effects.

## Typography

System UI stack (`Segoe UI`, `ui-sans-serif`) and system monospace. No webfont download, to keep LCP small.

## Components

Buttons, cards, badges (`available` / `experimental` / `planned` / `alpha`), screenshot figures, architecture SVG, approval/lightbox `<dialog>`, and the labeled verification simulation.

## Motion

`prefers-reduced-motion: reduce` disables animation and transitions. Theme toggle stores `starlight-theme` so docs and marketing stay in sync.
