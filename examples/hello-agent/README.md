# Hello Agent

The smallest verified Axiom run. A deterministic mock model calls `record_greeting`, the runtime checks a `StateProvider`, and the goal is verified only when Ada's greeting exists in that state.

Requires a running runtime (`cargo run -p axiom-server`, default `http://127.0.0.1:7400`). No model API keys.

```bash
pip install -e ../../sdk/python
python main.py
```

Expected output (ids vary):

```
run_id:        ...
status:        COMPLETED
verification:  VERIFIED
steps:         2
```

A non-zero exit means the goal was not verified. Inspect the run in Studio at `http://127.0.0.1:3000/executions/<run_id>`.

## Failure demonstration

Comment out the `record_greeting` tool call in `main.py` and leave only `ModelTurn(content="I greeted Ada")`. The model claims success; the runtime ends `FAILED` or `UNVERIFIED` because the state snapshot has no greeting.
