# Runtime API

While the server is running: `GET http://127.0.0.1:7400/api/v1/openapi.json`

A snapshot of that document is stored in `docs/api/openapi.json` (regenerate it from a live server; do not edit by hand).

Export without a database:

```bash
cargo run -p axiom-server -- --export-openapi docs/api/openapi.json
```
