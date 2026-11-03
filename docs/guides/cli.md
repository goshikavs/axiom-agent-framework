# CLI

Binary: `goshikq` (`apps/cli`). Default endpoint `http://127.0.0.1:7400` (`GOSHIKQ_RUNTIME_URL`). Use `--api-key` / `GOSHIKQ_API_KEY` when `GOSHIKQ_AUTH_MODE=api_key`.

```text
goshikq doctor
goshikq agents list
goshikq runs list
goshikq runs inspect <run-id>
goshikq verify <run-id>
goshikq init my-agent --template python
goshikq init my-agent --template typescript
goshikq run examples/hello-agent
goshikq studio
goshikq workflows validate examples/visual-workflow/parallel-approval.json
goshikq workflows publish examples/visual-workflow/parallel-approval.json
goshikq workflows run <workflow-id>
```

Windows paths work when quoted (`goshikq run "C:\Users\...\examples\hello-agent"`). Unsupported commands are not advertised. Non-zero exit: validation failure exits `2`; HTTP/IO errors exit `1`.
