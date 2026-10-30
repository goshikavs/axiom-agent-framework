# Changelog

## Unreleased — v0.9.1 (not published)

Release-gate closure on top of v0.9. Not a v1.0 tag. Docker smoke and GitHub Actions have not run on this authoring host.

### Added

- RS256 JWKS OIDC interoperability tests (issuer, audience, expiry, signature, key rotation, alg restrictions, cross-tenant)
- Native runtime OTLP/HTTP JSON export (`AXIOM_OTEL_EXPORTER_OTLP_ENDPOINT`) with redaction and sampling; inbound ingest unchanged
- Owner-only `POST /api/v1/admin/retention/purge` (dry-run default; skips active parents)
- Studio Dockerfile and compose `studio` profile
- Isolated Python wheel / TypeScript tarball install tests outside the repository
- GitHub Actions jobs for rust, SDKs, Studio, Playwright, Postgres integration, docker-smoke, secret scan, audits (not executed)
- `docs/milestones/v0.9.1-report.md`, `docs/release/v1.0-readiness.md`, `docs/release/release-checklist.md`, platform matrix

### Changed

- Unknown JWT `kid` forces a JWKS refresh (key rotation)
- HMAC tokens rejected unless loopback `AXIOM_OIDC_ALLOW_TEST_HMAC`
- Playwright inspector security test seeds via `pw-missing-telemetry` instead of skipping

## Unreleased — v0.9 (not published)

Production hardening on top of v0.1–v0.8. Packages, images, and GitHub releases are **not** published in this cycle.

### Added

- `AXIOM_AUTH_MODE=oidc` with JWKS/issuer/audience validation; loopback HMAC test mode
- Project membership, RBAC, API key rotate/revoke/expiry, security events
- Registered workflow TOOL dispatch through the worker protocol
- Spawned AGENT nodes fail `WORKER_UNAVAILABLE` instead of succeeding without a worker
- `/api/v1/runtime/metrics`, JSON logs, pool/lease/recovery counters
- Tenant isolation and reliability tests
- Docker files (image smoke untested without Docker Desktop on the authoring host)

### Security

- Server-side permission checks on sensitive routes
- Worker poll verifies project ownership before using an in-memory handle
- A2A message size cap; untrusted verification claims remain ignored
