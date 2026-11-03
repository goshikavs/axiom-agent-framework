# TypeScript refund agent

Defines the agent and tools in TypeScript. The **Rust runtime** plans, retries, records events, and verifies the goal against the simulated store. No paid model credentials are used.

```bash
# runtime must already be up (goshikq-server on :7400, Postgres)
cd examples/typescript-refund-agent
bun install
bun run start
```

Successful run prints `COMPLETED/VERIFIED` and a `run_id`. Open that id in Goshikq Studio (`http://127.0.0.1:3000/executions/<run_id>`).

## Demonstrations

| Command | What it shows |
| --- | --- |
| `bun run start` | Happy path: lookup → eligibility → refund → VERIFIED |
| `GOSHIKQ_REFUND_FAULT=tool_error bun run start` | Tool failure (`simulated processor outage`) |
| `GOSHIKQ_REFUND_FAULT=never_settles bun run start` | Pending external operation (`WAITING_FOR_EXTERNAL_STATE` / UNVERIFIED) |
| `GOSHIKQ_REFUND_FAULT=processor_reject bun run start` | Processor declined; goal not verified |

Events are written to PostgreSQL by goshikq-server. Studio never executes the refund.
