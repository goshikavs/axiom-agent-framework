# A2A interoperability (protocol 1.0 JSON-RPC subset)

Verified against [A2A Protocol 1.0](https://a2a-protocol.org) (major.minor, no patch in negotiation).

## Supported

| Surface | Binding |
|---|---|
| Agent Card | `GET /.well-known/agent-card.json` |
| JSON-RPC 2.0 | `POST /a2a` |
| `SendMessage` / `message/send` | Creates a deferred Goshikq run (`source_framework=a2a`) |
| `GetTask` / `tasks/get` | Maps run status → A2A task state |
| `CancelTask` / `tasks/cancel` | Existing run cancel + descendant cancel |

`supportedInterfaces[0].protocolBinding` is `JSONRPC`, `protocolVersion` is `1.0`.

Authentication uses the same `GOSHIKQ_AUTH_MODE` as the rest of the API (`none` locally, `Bearer` API key otherwise). Remote payloads are untrusted: `verification_status` / `markVerified` in params are ignored.

## Unsupported (not claimed)

- gRPC and HTTP+JSON REST bindings
- Streaming (`message/stream`)
- Push notifications
- Extended agent card
- `ListTasks`
- Treating A2A `completed` / SUCCESS as Goshikq `VERIFIED`

## Mapping

| Goshikq `run.status` | A2A `status.state` |
|---|---|
| `CREATED`, `PAUSED` | `submitted` |
| `WAITING_FOR_APPROVAL` | `input-required` |
| `COMPLETED`, `UNVERIFIED` | `completed` |
| `FAILED`, timeouts, budget | `failed` |
| `CANCELLED` | `canceled` |
| other active | `working` |

`metadata.goshikqVerificationStatus` is always the persisted Goshikq column. `completed` + `NOT_EVALUATED` is expected until a parent goal runs.

## Independent peer test

`examples/a2a-collaboration/peer.py` is a **non-Goshikq** process. `examples/a2a-collaboration/main.py` talks to it with `A2aClient`, then an Goshikq parent goal evaluates **local** state.

| Scenario | Remote state | Goshikq parent |
|---|---|---|
| `collaborate on inventory lookup` | `completed` | `01a1895e-c2e3-75ae-b700-0ed4f9543443` `VERIFIED` (local predicate, not the peer flag) |
| `FAIL please` | `failed` | `01a1895e-c74a-762e-aeec-9ea8ada542b9` `FAILED` |
