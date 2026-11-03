# Framework capability manifests

Every adapter publishes a `FrameworkCapabilityManifest` (`GET /api/v1/capabilities`). Studio Integrations shows the same records from `framework_integrations`.

Fields: framework name/version, adapter version, and support level (`supported` | `partial` | `unsupported`) for:

- traces
- tool events
- state snapshots
- approvals
- action interception
- pause/resume
- outcome verification
- replay

External adapters (OTLP, LangGraph, OpenAI Agents SDK) mark **action interception, approvals, and pause/resume as unsupported**. Goshikq does not claim to control those runtimes. Outcome verification is partial only when the producer sends an explicit `goshikq.state_snapshot` attribute or a native Goshikq goal is attached separately.

Do not treat a connected integration row as proof that every capability is live. Health is `status` plus `last_event_at` from persisted rows.
