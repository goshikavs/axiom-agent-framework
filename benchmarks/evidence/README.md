# Evidence-revalidation benchmarks

Independent strategies (not aliases):

| Id | Name | Algorithm |
|---|---|---|
| A | `none` | Empty result. Never revalidate. |
| B | `full_declared` | If any registered resource hits, recheck every declared decision, action, and approval. |
| C | `conventional_prewrite` | Version-check resources declared on the **action node** (`resource_id`). No graph walk. |
| D | `selective` | `goshikq_core::compute_impact` on the **declared** graph only. |

Ground truth is a BFS over the **true** graph (declared ∪ hidden edges) implemented in `crates/goshikq-bench/src/ground_truth.rs`. It does not call `compute_impact`. Hidden edges are withheld from A–D, so no strategy can detect unregistered dependencies.

## Reproduce

```powershell
scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc run -p goshikq-bench --release -- --sizes 10,100,1000,10000 --trials 10 --seed 42 --out benchmarks/evidence/results.json
```

Same seed → same scenarios. Output is JSON. Interpretation: `docs/benchmarks/evidence-verification.md`.

`tests/perf/bench_evidence.py` is a **live SDK latency** workload on the refund example. It is not this study.
