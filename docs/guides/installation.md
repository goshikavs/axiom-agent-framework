# Installation

Tested on Windows x64 (MSVC) with PostgreSQL on `127.0.0.1:55432`.

## Runtime

```text
# PostgreSQL
# DATABASE_URL=postgres://postgres:goshikq@127.0.0.1:55432/goshikq

scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc build -p goshikq-server
$env:DATABASE_URL="postgres://postgres:goshikq@127.0.0.1:55432/goshikq"
$env:GOSHIKQ_BIND="127.0.0.1:7400"
$env:GOSHIKQ_AUTH_MODE="none"
.\target\debug\goshikq-server.exe
```

On ARM64 Windows, compile the x86_64 target as above. Native ARM64 `link.exe` is not used.

## Studio

```text
cd apps/studio
bun install
bun run dev
# http://127.0.0.1:3000
```

## SDKs (local, unpublished)

```text
pip install -e sdk/python
cd sdk/typescript && bun install
```

Do not publish packages without owner authorization.

## Three-minute offline quickstart

1. Start Postgres and `goshikq-server` as above.
2. `python examples/hello-agent/main.py`
3. Open Studio Executions and confirm `VERIFIED`.
