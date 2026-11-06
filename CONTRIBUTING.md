# Contributing

Maintainer: [goshikavs](https://github.com/goshikavs).

1. Open an issue before large design changes.
2. Keep the vertical slice runnable: runtime + Python SDK + mock model + Studio.
3. Do not mark planned features as available. Update `docs/IMPLEMENTATION_STATUS.md`.
4. Tests: `cargo test --workspace`, `pytest` in `sdk/python`, `bun test` in `sdk/typescript`.
5. On Windows without `link.exe` on PATH, use `scripts/with-msvc.ps1`.
6. Do not commit `.env`, credentials, or captured production traces.
7. Third-party framework adapters must declare supported and unsupported capabilities explicitly.
8. Recapture Studio docs screenshots from the running app (`bun run screenshots:docs`). Do not commit AI-generated UI or production traces. See `docs/guides/studio-screenshots.md`.

License: Apache-2.0.
