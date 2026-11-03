# Threat model (v0.9)

This is a working model for external developer trials, not a certification. Goshikq does **not** claim immunity to prompt injection or universal enterprise compliance.

## Assets

- Run definitions, events, evidence, shared context
- API keys and OIDC tokens
- Approval decisions
- Workflow graphs and registered tool dispatch

## Actors

- Authenticated user (OIDC)
- Service identity (API key)
- Untrusted model / remote A2A peer
- Malicious workflow author
- Network attacker on the bind address

## Threats and mitigations

| Threat | Mitigation | Residual |
|---|---|---|
| Prompt injection | Tool calls are untrusted suggestions; `ActionRegistry` validates name/schema/permissions | Model still sees tool descriptions |
| Malicious tool descriptions | Descriptions are data; execution uses registered handlers only | Social engineering of reviewers |
| SSRF | Runtime does not fetch URLs from workflow JSON; JWKS HTTPS (loopback HTTP for tests) | Operator-configured JWKS URL |
| Cross-tenant access | Project filter + membership + tests | `none` mode is local owner of the default project |
| Untrusted telemetry | OTLP/A2A cannot set `verification_status` | High-volume ingest (rate limited) |
| Workflow import abuse | Schema validation, 128-node / 256-edge caps, body limit | Draft graphs can still be stored |
| Privilege escalation | Role checks server-side; operator cannot approve | Mis-seeded `owner` membership |
| Credential exposure | Hashed keys, JSON logs without Authorization, Studio proxy | Mis-set `GOSHIKQ_CAPTURE_MODEL_IO=full` |
| Replay | JWT `exp`/`nbf`; API key last-used; idempotency keys | Stolen unexpired JWT |
| Uncertain external writes | Action journal `DISPATCH_UNCERTAIN`; no blind retry of non-idempotent tools | Operator reconciliation |
| Approval misuse | `ApprovalDecide` permission; concurrent decide is idempotent | Stolen approver credential |
| Oversized requests | `GOSHIKQ_MAX_BODY_BYTES`, OTLP span cap, A2A text cap | Slowloris-style connections |
| Dependency vulns | Maintained crates/npm/pip; scan when publishing | Transitive CVEs |

## Dependency scanning

Use maintained scanners at release time (`cargo audit`, `pip-audit`, `bun audit`). This cycle records the threat model and critical authz fixes; scanner output is environment-specific and is not treated as a clean bill of health without a dated report.

## Explicit non-claims

- Prompt injection is not solved.
- MCP Streamable HTTP and OAuth are not implemented.
- Docker images on this host were not smoke-tested (Docker Desktop absent).
- Native Windows ARM64 linking remains untested.
