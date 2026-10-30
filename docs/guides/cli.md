# CLI

Binary: `axiom` (`apps/cli`). Default endpoint `http://127.0.0.1:7400` (`AXIOM_RUNTIME_URL`). Use `--api-key` / `AXIOM_API_KEY` when `AXIOM_AUTH_MODE=api_key`.

```text
axiom doctor
axiom agents list
axiom runs list
axiom runs inspect <run-id>
axiom verify <run-id>
axiom init my-agent --template python
axiom init my-agent --template typescript
axiom run examples/hello-agent
axiom studio
axiom workflows validate examples/visual-workflow/parallel-approval.json
axiom workflows publish examples/visual-workflow/parallel-approval.json
axiom workflows run <workflow-id>
```

Windows paths work when quoted (`axiom run "C:\Users\...\examples\hello-agent"`). Unsupported commands are not advertised. Non-zero exit: validation failure exits `2`; HTTP/IO errors exit `1`.
