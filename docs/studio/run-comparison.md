# Run comparison

URL: `/executions/compare`

Both sides are `GET /api/v1/runs/{id}` plus events. Compared fields include framework, model, goal, status, verification, duration, tokens, cost, dispatch, and tool status.

Event matching:

1. **identity** — same `span_id` and `event_type`
2. **heuristic** — same `event_type` + tool + model
3. **unmatched** — shown explicitly

Uncertain matches are labeled. No side effects.

![Goshikq Studio run comparison](../assets/studio/goshikq-run-comparison.webp)
*Side-by-side `hello-agent` and `refund-supervisor`, both `VERIFIED`. Unmatched events are labeled.*
