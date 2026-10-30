# Retail: return status changes before refund submission

```text
python examples/retail-refund/main.py --fault return_changed
```

1. `check_eligibility` records `order:ORD-2048` at the current version and a coverage-style eligibility decision.
2. The next authoritative snapshot (injected fault) sets `return_status=rejected` and bumps the order version.
3. Before `submit_refund`, Axiom revalidates. Eligibility no longer holds.
4. The processor is not called (`submit_count` stays 0). The goal is not verified.

Happy path is unchanged:

```text
python examples/retail-refund/main.py
# COMPLETED / VERIFIED, store_order_status: refunded
```
