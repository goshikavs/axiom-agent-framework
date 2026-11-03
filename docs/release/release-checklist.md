# Release checklist

Owner: **goshikavs**. Do not add Cursor or AI assistants as authors, co-authors, contributors, or maintainers.

Nothing in this list has been performed unless the owner authorized it.

## Before first public tag

- [ ] Owner confirms git identity (`user.name` / `user.email`) — do not change git config from tooling
- [ ] Review `git status` for secrets (`.env`, `*.pem`, local DBs, logs)
- [ ] Initial commit authorized by owner
- [x] GitHub repository renamed to `goshikavs/goshikq-agent-framework` (old `axiom-agent-framework` URL redirects)
- [ ] `git push -u origin main` authorized
- [ ] GitHub Actions run on `ubuntu-latest` recorded: rust, python-sdk, typescript, studio, docs, integration, packaging, playwright, docker-smoke, security-scans
- [ ] Docker smoke log attached to the milestone
- [ ] Isolated SDK install job green
- [ ] JWKS OIDC tests green (local RSA and, if available, hosted IdP)
- [ ] `cargo audit` / `pip-audit` / gitleaks reviewed; critical vulns fixed or they block the tag
- [ ] CHANGELOG dated
- [ ] LICENSE Apache-2.0 still accurate (`docs/release/license-review.md`)
- [ ] Platform matrix still honest
- [ ] No SOC 2 / HIPAA / PCI language in README

## Publishing (separate authorization)

- [ ] crates.io (optional)
- [ ] PyPI `goshikq-ai` (optional)
- [ ] npm `@goshikq/sdk` (optional)
- [ ] GitHub Release + source archive
- [ ] Container registry images

Do not publish from this cycle.
