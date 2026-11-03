# Supported platform matrix (v0.9.1)

Classifications: **TESTED** (ran on this OS/arch), **BUILD_ONLY** (compiled or CI-defined, not executed here), **EXPERIMENTAL**, **UNSUPPORTED**.

| Platform | Runtime (`goshikq-server`) | Python SDK | TypeScript SDK | Studio | Classification |
| --- | --- | --- | --- | --- | --- |
| Windows x64 (MSVC) | Native binary via `stable-x86_64-pc-windows-msvc` | 3.11–3.14 | Bun 1.2+ | Next.js 15 | **TESTED** |
| Windows ARM64 host compiling x64 MSVC | Same x64 binary | Same | Same | Same | **TESTED** (x64 binary on ARM64 Windows) |
| Windows ARM64 native (`aarch64-pc-windows-msvc`) | Not built | n/a | n/a | n/a | **UNSUPPORTED** |
| Linux x64 | Dockerfile + GitHub Actions `ubuntu-latest` | CI job defined | CI job defined | CI job defined | **BUILD_ONLY** until a GitHub Actions run is recorded |
| macOS (Intel or Apple Silicon) | Not built on this host | Not run | Not run | Not run | **UNSUPPORTED** |

Native ARM64 macOS/Windows runtimes are not claimed. A smaller verified matrix (Windows x64) is the initial release surface.

GitHub Actions workflows exist under `.github/workflows/ci.yml` but have **not** executed: this repository has no commits, remotes, or GitHub project yet.
