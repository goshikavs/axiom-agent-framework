# Retail refund (simulated)

Simulated order database and payment processor. The agent looks up the order, checks eligibility, submits a refund, and polls settlement. The goal is **not** completed because the processor returned `accepted`; it is completed only when `order.status == refunded` **and** `refund.status == settled` in `SimulatedStore`.

```bash
pip install -e ../../sdk/python
python main.py
python main.py --fault processor_reject
python main.py --order ORD-LOCKED
```

| Command | Expected verification |
| --- | --- |
| `python main.py` | `VERIFIED` |
| `python main.py --fault processor_reject` | `FAILED` (processor declined; order `refund_failed`) |
| `python main.py --order ORD-LOCKED` | `FAILED` (ineligible; no write) |
| `python main.py --fault delayed_settlement` | May stay `UNVERIFIED`/`FAILED` until settlement; the accepted request is not treated as success |
| `python main.py --fault missing_evidence` | `UNVERIFIED` or `INCONCLUSIVE` — missing state is not success |

Open the run in Studio and compare **intended outcome**, **observed store snapshot**, and **verification evidence**.
