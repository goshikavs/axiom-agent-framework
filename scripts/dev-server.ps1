# Start axiom-server with the local MSVC toolchain and embedded PostgreSQL.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
if (-not (Test-Path .env) -and (Test-Path .env.example)) {
    Copy-Item .env.example .env
}
$env:DATABASE_URL = if ($env:DATABASE_URL) { $env:DATABASE_URL } else { "embedded" }
& "$PSScriptRoot\with-msvc.ps1" cargo +stable-x86_64-pc-windows-msvc run -p axiom-server --release
