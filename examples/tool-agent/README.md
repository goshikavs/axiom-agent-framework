# Tool Agent

A typed Python tool with JSON Schema validation. The mock model first sends an integer `sku` (rejected by the runtime), then a valid string. The goal is verified against inventory state, not the model's summary.

```bash
pip install -e ../../sdk/python
python main.py
```

Expected: `status: COMPLETED`, `verification: VERIFIED`. Studio shows a `tool.failed` event with `invalid_arguments` followed by a successful `lookup_sku`.
