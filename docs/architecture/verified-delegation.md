# Verified delegation

A child agent reporting completion must not automatically verify its parent goal.

## Pipeline

1. Child task execution (`run.status` may become `COMPLETED` / `UNVERIFIED` / `FAILED`).
2. Child goal verification against **that child’s** `StateProvider` / success predicate.
3. Authoritative evidence stays in the existing evidence tables (no second graph).
4. Parent goal verification runs only when the parent executor reaches `VERIFYING_GOAL`.
5. Evidence dependency / revalidation uses the v0.3 graph. A policy version change still invalidates dependents.
6. Shared-context OCC (`expected_version`) detects concurrent writes.

## Insurance example

Supervisor delegates coverage, assessment, fraud, then payout. After A–C, `POL-1` can flip from version 4 (reserve 15000) to version 5 (reserve 5000). Payout of 15000 is refused by the live store. The parent predicate `payout_settled` reads current policy version + payout row.

Recorded 2026-10-29:

| Command | Parent run | Parent verification |
|---|---|---|
| `python examples/multi-agent-insurance/main.py` | `01a1895e-c10b-7021-8258-206b61db9d3d` | `VERIFIED` (policy v4) |
| `python examples/multi-agent-insurance/main.py --mutate-policy` | `01a1895e-d3a4-70ac-9aa9-e56a9ae6bd1f` | `FAILED` (policy v5, no payout) |

Children may themselves be `VERIFIED` against a trivial child predicate. That is not payout authorization.

## Remote A2A

`SendMessage` / peer `completed` / metadata `SUCCESS` are untrusted. `task.metadata.goshikqVerificationStatus` stays `NOT_EVALUATED` unless an Goshikq parent goal is evaluated independently. `markVerified` in A2A params is ignored.
