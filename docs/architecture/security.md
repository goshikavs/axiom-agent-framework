# Threat model (v0.1)

Scope: local single-user runtime (`GOSHIKQ_AUTH_MODE=none`, loopback bind) and the `api_key` mode used for non-loopback binds.

| Threat | Mitigation in this alpha | Residual |
| --- | --- | --- |
| Prompt injection | Model output cannot change permissions, schemas, budgets, or transitions | Tool descriptions are untrusted; MCP not implemented yet |
| Tool-output poisoning | Results are data; predicates read StateProvider snapshots | Large tool results are truncated, not executed |
| Unauthorized actions | `ToolSpec.required_permissions`, approval gates, effect-based retries | Local mode has no real users |
| Cross-tenant access | Every query filters `project_id` | Single default project in local mode |
| Replay / duplicate writes | Idempotency keys passed to tools; duplicate event_id ingestion is a no-op | Tools must honor keys |
| Leaked telemetry | Default `GOSHIKQ_CAPTURE_MODEL_IO=redacted`; secret key/pattern redaction | `full` capture is an explicit choice |
| Untrusted remote agents | Not implemented | Do not equate remote status with verified success |
| SSRF via HTTP tools | No generic HTTP tool in the runtime process | User tools must apply their own allowlists |
| Stolen API keys | Keys stay in the SDK process; Studio proxies without them | `.env` must stay uncommitted |

Production must set `GOSHIKQ_AUTH_MODE=api_key`, bind only as intended, and treat traces as confidential.
