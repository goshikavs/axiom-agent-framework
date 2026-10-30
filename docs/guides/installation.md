# Installation

Tested on Windows x64 (MSVC) with PostgreSQL on `127.0.0.1:55432`.

## Runtime

```text
# PostgreSQL
# DATABASE_URL=postgres://postgres:axiom@127.0.0.1:55432/axiom

scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc build -p axiom-server
$env:DATABASE_URL="postgres://postgres:axiom@127.0.0.1:55432/axiom"
$env:AXIOM_BIND="127.0.0.1:7400"
$env:AXIOM_AUTH_MODE="none"
.\target\debug\axiom-server.exe
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

1. Start Postgres and `axiom-server` as above.
2. `python examples/hello-agent/main.py`
3. Open Studio Executions and confirm `VERIFIED`.
